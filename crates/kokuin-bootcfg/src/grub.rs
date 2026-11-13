//! GRUB: `grub-mkconfig` builds `grub.cfg` from `/etc/default/grub` (shell variables)
//! and the scripts in `/etc/grub.d/`.
//!
//! The parameters go in `GRUB_CMDLINE_LINUX_DEFAULT`: it comes last on the kernel
//! command line (so kokuin's `init=` wins over an earlier one), and recovery entries
//! leave it out (they are not recorded). On Debian and its derivatives,
//! `grub-mkconfig` also reads `/etc/default/grub.d/*.cfg`: kokuin adds a file of its
//! own there for each [`Section`]. Elsewhere it adds a marked block per section at
//! the end of `/etc/default/grub`.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{Edit, Plan, Section};

const DEFAULTS: &str = "etc/default/grub";
/// GRUB's extra settings, on Debian and its derivatives.
const DROP_INS: &str = "etc/default/grub.d";

/// GRUB's configuration on a system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grub {
    /// `/etc/default/grub`.
    pub defaults_path: PathBuf,
    /// Its content.
    pub defaults: String,
    /// `/etc/default/grub.d/`, if `grub-mkconfig` reads it.
    pub drop_in_dir: Option<PathBuf>,
    /// The content of kokuin's files there, by section.
    pub drop_ins: BTreeMap<Section, String>,
    /// The command regenerating `grub.cfg`.
    pub regenerate: Vec<String>,
}

impl Grub {
    /// Reads GRUB's configuration under `root`. `None` when GRUB's configuration
    /// tools are not installed.
    pub fn read(root: &Path) -> Option<Self> {
        let defaults_path = root.join(DEFAULTS);
        let defaults = fs::read_to_string(&defaults_path).ok()?;
        let program = |name: &str| {
            ["usr/sbin", "usr/bin", "sbin", "bin"]
                .iter()
                .map(|dir| root.join(dir).join(name))
                .find(|path| path.is_file())
        };
        let mkconfig = program("grub-mkconfig").or_else(|| program("grub2-mkconfig"))?;
        let regenerate = match program("update-grub") {
            Some(update) => vec![absolute(root, &update)],
            None => {
                let output = ["boot/grub/grub.cfg", "boot/grub2/grub.cfg"]
                    .iter()
                    .find(|path| root.join(path).is_file())
                    .unwrap_or(&"boot/grub/grub.cfg");
                vec![absolute(root, &mkconfig), "-o".into(), format!("/{output}")]
            }
        };
        let reads_drop_ins =
            fs::read_to_string(&mkconfig).is_ok_and(|script| script.contains("default/grub.d"));
        let drop_in_dir = reads_drop_ins.then(|| root.join(DROP_INS));
        let drop_ins = drop_in_dir
            .iter()
            .flat_map(|dir| {
                Section::ALL.into_iter().filter_map(|section| {
                    let content = fs::read_to_string(dir.join(drop_in_name(section))).ok()?;
                    Some((section, content))
                })
            })
            .collect();
        Some(Self {
            defaults_path,
            defaults,
            drop_in_dir,
            drop_ins,
            regenerate,
        })
    }

    /// Whether a section is there.
    pub fn is_enabled(&self, section: Section) -> bool {
        self.drop_ins.contains_key(&section) || find_block(&self.defaults, section).is_some()
    }

    pub(crate) fn plan_enable(&self, section: Section, parameters: &str) -> Plan {
        let block = block(section, parameters);
        let mut edits = Vec::new();
        match &self.drop_in_dir {
            Some(dir) => {
                let current = self.drop_ins.get(&section);
                if current != Some(&block) {
                    edits.push(Edit {
                        path: dir.join(drop_in_name(section)),
                        before: current.cloned(),
                        after: Some(block),
                    });
                }
                // A block left by an earlier version goes away.
                if let Some(cleaned) = remove_block(&self.defaults, section) {
                    edits.push(self.defaults_edit(cleaned));
                }
            }
            None => {
                let base =
                    remove_block(&self.defaults, section).unwrap_or_else(|| self.defaults.clone());
                let after = add_block(&base, &block);
                if after != self.defaults {
                    edits.push(self.defaults_edit(after));
                }
            }
        }
        self.plan(edits)
    }

    pub(crate) fn plan_disable(&self, sections: &[Section]) -> Plan {
        let mut edits = Vec::new();
        let mut defaults = self.defaults.clone();
        for &section in sections {
            if let (Some(dir), Some(content)) = (&self.drop_in_dir, self.drop_ins.get(&section)) {
                edits.push(Edit {
                    path: dir.join(drop_in_name(section)),
                    before: Some(content.clone()),
                    after: None,
                });
            }
            if let Some(cleaned) = remove_block(&defaults, section) {
                defaults = cleaned;
            }
        }
        if defaults != self.defaults {
            edits.push(self.defaults_edit(defaults));
        }
        self.plan(edits)
    }

    fn defaults_edit(&self, after: String) -> Edit {
        Edit {
            path: self.defaults_path.clone(),
            before: Some(self.defaults.clone()),
            after: Some(after),
        }
    }

    fn plan(&self, edits: Vec<Edit>) -> Plan {
        Plan {
            bootloader: "GRUB".into(),
            regenerate: if edits.is_empty() {
                Vec::new()
            } else {
                self.regenerate.clone()
            },
            edits,
        }
    }
}

fn absolute(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    Path::new("/").join(relative).display().to_string()
}

/// kokuin's file for a section in `/etc/default/grub.d/`.
fn drop_in_name(section: Section) -> String {
    format!("{}.cfg", section.name())
}

/// First line of a section's block.
fn begin_line(section: Section) -> String {
    format!("# BEGIN {}: {}", section.name(), section.lifetime())
}

/// Last line of a section's block.
fn end_line(section: Section) -> String {
    format!("# END {}", section.name())
}

/// A section's settings, between its markers.
pub fn block(section: Section, parameters: &str) -> String {
    format!(
        "{}\nGRUB_CMDLINE_LINUX_DEFAULT=\"$GRUB_CMDLINE_LINUX_DEFAULT {parameters}\"\n{}\n",
        begin_line(section),
        end_line(section)
    )
}

/// Appends a block at the end, on lines of its own.
pub fn add_block(text: &str, block: &str) -> String {
    let mut out = text.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(block);
    out
}

/// The text without a section's block, or `None` if it has none.
pub fn remove_block(text: &str, section: Section) -> Option<String> {
    let (start, end) = find_block(text, section)?;
    Some(format!("{}{}", &text[..start], &text[end..]))
}

/// Byte range of a section's block, from its first line to the end of its last line.
/// Markers are whole lines: `# END kokuin` must not match `# END kokuin-deep`.
fn find_block(text: &str, section: Section) -> Option<(usize, usize)> {
    let begin = format!("# BEGIN {}:", section.name());
    let end = end_line(section);
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        let content = line.trim_end_matches('\n');
        match start {
            None if content.starts_with(&begin) => start = Some(offset),
            Some(start) if content == end => return Some((start, offset + line.len())),
            _ => {}
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAMETERS: &str = "init=/usr/lib/kokuin/kokuin-init";
    const DEEP: &str = "init=/usr/lib/kokuin/kokuin-init initcall_debug kokuin.deep=1";
    const ALPINE: &str = "GRUB_TIMEOUT=2\nGRUB_DISABLE_SUBMENU=y\n\
        GRUB_CMDLINE_LINUX_DEFAULT=\"modules=sd-mod,usb-storage,ext4 quiet\"\n";

    fn grub(drop_ins: bool) -> Grub {
        Grub {
            defaults_path: PathBuf::from("/etc/default/grub"),
            defaults: ALPINE.into(),
            drop_in_dir: drop_ins.then(|| PathBuf::from("/etc/default/grub.d")),
            drop_ins: BTreeMap::new(),
            regenerate: vec!["/usr/sbin/update-grub".into()],
        }
    }

    /// The GRUB configuration once a plan is applied.
    fn applied(grub: &Grub, plan: &Plan) -> Grub {
        let mut grub = grub.clone();
        for edit in &plan.edits {
            if edit.path == grub.defaults_path {
                grub.defaults = edit.after.clone().unwrap();
                continue;
            }
            let section = Section::ALL
                .into_iter()
                .find(|s| edit.path.ends_with(drop_in_name(*s)))
                .unwrap();
            match &edit.after {
                Some(after) => grub.drop_ins.insert(section, after.clone()),
                None => grub.drop_ins.remove(&section),
            };
        }
        grub
    }

    #[test]
    fn adds_a_block_and_removes_exactly_it() {
        let grub = grub(false);
        let plan = grub.plan_enable(Section::Recording, PARAMETERS);
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(plan.regenerate, ["/usr/sbin/update-grub"]);
        let enabled = applied(&grub, &plan);
        assert!(enabled.defaults.starts_with(ALPINE));
        assert!(enabled.defaults.contains(
            "GRUB_CMDLINE_LINUX_DEFAULT=\"$GRUB_CMDLINE_LINUX_DEFAULT init=/usr/lib/kokuin/kokuin-init\""
        ));
        assert!(enabled.is_enabled(Section::Recording));
        assert!(!enabled.is_enabled(Section::Deep));

        let disabled = applied(&enabled, &enabled.plan_disable(&[Section::Recording]));
        assert_eq!(disabled.defaults, ALPINE);
    }

    #[test]
    fn keeps_the_markers_of_earlier_versions() {
        let block = block(Section::Recording, PARAMETERS);
        assert!(block.starts_with(
            "# BEGIN kokuin: added by `kokuin-cli enable`, removed by `kokuin-cli disable`\n"
        ));
        assert!(block.ends_with("\n# END kokuin\n"));
    }

    #[test]
    fn sections_come_and_go_independently() {
        let grub = grub(false);
        let recording = applied(&grub, &grub.plan_enable(Section::Recording, PARAMETERS));
        let both = applied(&recording, &recording.plan_enable(Section::Deep, DEEP));
        assert!(both.is_enabled(Section::Recording) && both.is_enabled(Section::Deep));

        let without_deep = applied(&both, &both.plan_disable(&[Section::Deep]));
        assert_eq!(without_deep.defaults, recording.defaults);
        let without_recording = applied(&both, &both.plan_disable(&[Section::Recording]));
        assert!(without_recording.is_enabled(Section::Deep));
        assert!(!without_recording.is_enabled(Section::Recording));
        let none = applied(
            &without_recording,
            &without_recording.plan_disable(&[Section::Deep]),
        );
        assert_eq!(none.defaults, ALPINE);
        // Both at once, in one change of the file.
        let plan = both.plan_disable(&Section::ALL);
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(applied(&both, &plan).defaults, ALPINE);
    }

    #[test]
    fn enabling_twice_changes_nothing() {
        let grub = grub(false);
        let once = applied(&grub, &grub.plan_enable(Section::Recording, PARAMETERS));
        assert!(once.plan_enable(Section::Recording, PARAMETERS).is_empty());
    }

    #[test]
    fn new_parameters_replace_the_old_block() {
        let grub = grub(false);
        let once = applied(&grub, &grub.plan_enable(Section::Recording, PARAMETERS));
        let changed = applied(
            &once,
            &once.plan_enable(
                Section::Recording,
                "init=/usr/lib/kokuin/kokuin-init kokuin_init=/sbin/openrc-init",
            ),
        );
        assert_eq!(changed.defaults.matches("# BEGIN kokuin:").count(), 1);
        assert!(changed.defaults.contains("kokuin_init=/sbin/openrc-init"));
    }

    #[test]
    fn uses_a_file_per_section_where_grub_reads_them() {
        let grub = grub(true);
        let plan = grub.plan_enable(Section::Deep, DEEP);
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(
            plan.edits[0].path,
            PathBuf::from("/etc/default/grub.d/kokuin-deep.cfg")
        );
        assert_eq!(plan.edits[0].before, None);

        let enabled = applied(&grub, &plan);
        assert!(enabled.is_enabled(Section::Deep));
        assert!(!enabled.is_enabled(Section::Recording));
        let disable = enabled.plan_disable(&[Section::Deep]);
        assert_eq!(disable.edits.len(), 1);
        assert_eq!(disable.edits[0].after, None);
    }

    #[test]
    fn disabling_what_is_not_enabled_does_nothing() {
        let plan = grub(true).plan_disable(&[Section::Recording]);
        assert!(plan.is_empty());
        assert!(plan.regenerate.is_empty());
    }

    #[test]
    fn a_file_without_final_newline_gets_one_before_the_block() {
        let text = add_block("GRUB_TIMEOUT=2", &block(Section::Recording, PARAMETERS));
        assert!(text.starts_with("GRUB_TIMEOUT=2\n# BEGIN kokuin"));
        assert_eq!(
            remove_block(&text, Section::Recording).as_deref(),
            Some("GRUB_TIMEOUT=2\n")
        );
    }

    #[test]
    fn reads_a_system_layout() {
        let root = std::env::temp_dir().join(format!("kokuin-grub-{}", std::process::id()));
        fs::create_dir_all(root.join("etc/default/grub.d")).unwrap();
        fs::create_dir_all(root.join("usr/sbin")).unwrap();
        fs::create_dir_all(root.join("boot/grub2")).unwrap();
        fs::write(root.join(DEFAULTS), ALPINE).unwrap();
        fs::write(
            root.join("usr/sbin/grub2-mkconfig"),
            "for x in ${sysconfdir}/default/grub.d/*.cfg ; do . \"$x\"; done\n",
        )
        .unwrap();
        fs::write(root.join("boot/grub2/grub.cfg"), "").unwrap();
        fs::write(root.join("etc/default/grub.d/kokuin-deep.cfg"), "deep").unwrap();
        let grub = Grub::read(&root);
        fs::remove_dir_all(&root).unwrap();

        let grub = grub.unwrap();
        assert_eq!(grub.defaults, ALPINE);
        assert_eq!(grub.drop_in_dir, Some(root.join(DROP_INS)));
        assert_eq!(
            grub.drop_ins.get(&Section::Deep).map(String::as_str),
            Some("deep")
        );
        assert!(!grub.drop_ins.contains_key(&Section::Recording));
        assert_eq!(
            grub.regenerate,
            ["/usr/sbin/grub2-mkconfig", "-o", "/boot/grub2/grub.cfg"]
        );
    }
}
