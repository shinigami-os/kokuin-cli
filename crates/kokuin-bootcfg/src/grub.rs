//! GRUB: `grub-mkconfig` builds `grub.cfg` from `/etc/default/grub` (shell variables)
//! and the scripts in `/etc/grub.d/`.
//!
//! The parameters go in `GRUB_CMDLINE_LINUX_DEFAULT`: it comes last on the kernel
//! command line (so kokuin's `init=` wins over an earlier one), and recovery entries
//! leave it out (they are not recorded). On Debian and its derivatives,
//! `grub-mkconfig` also reads `/etc/default/grub.d/*.cfg`: kokuin adds its own file
//! there. Elsewhere it adds a marked block at the end of `/etc/default/grub`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{Edit, Plan};

/// First line of kokuin's block.
const BEGIN: &str = "# BEGIN kokuin: added by `kokuin-cli enable`, removed by `kokuin-cli disable`";
/// Last line of kokuin's block.
const END: &str = "# END kokuin";
/// kokuin's file among GRUB's extra settings, where supported.
const DROP_IN: &str = "etc/default/grub.d/kokuin.cfg";
const DEFAULTS: &str = "etc/default/grub";

/// GRUB's configuration on a system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grub {
    /// `/etc/default/grub`.
    pub defaults_path: PathBuf,
    /// Its content.
    pub defaults: String,
    /// kokuin's file in `/etc/default/grub.d/`, if `grub-mkconfig` reads that folder.
    pub drop_in_path: Option<PathBuf>,
    /// The content of that file, if it exists.
    pub drop_in: Option<String>,
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
        let drop_in_path = reads_drop_ins.then(|| root.join(DROP_IN));
        let drop_in = drop_in_path
            .as_ref()
            .and_then(|path| fs::read_to_string(path).ok());
        Some(Self {
            defaults_path,
            defaults,
            drop_in_path,
            drop_in,
            regenerate,
        })
    }

    /// Whether kokuin's settings are there.
    pub fn is_enabled(&self) -> bool {
        self.drop_in.is_some() || find_block(&self.defaults).is_some()
    }

    pub(crate) fn plan_enable(&self, parameters: &str) -> Plan {
        let block = block(parameters);
        let mut edits = Vec::new();
        match &self.drop_in_path {
            Some(path) => {
                if self.drop_in.as_deref() != Some(block.as_str()) {
                    edits.push(Edit {
                        path: path.clone(),
                        before: self.drop_in.clone(),
                        after: Some(block),
                    });
                }
                // A block left by an earlier version goes away.
                if let Some(cleaned) = remove_block(&self.defaults) {
                    edits.push(self.defaults_edit(cleaned));
                }
            }
            None => {
                let base = remove_block(&self.defaults).unwrap_or_else(|| self.defaults.clone());
                let after = add_block(&base, &block);
                if after != self.defaults {
                    edits.push(self.defaults_edit(after));
                }
            }
        }
        self.plan(edits)
    }

    pub(crate) fn plan_disable(&self) -> Plan {
        let mut edits = Vec::new();
        if let (Some(path), Some(content)) = (&self.drop_in_path, &self.drop_in) {
            edits.push(Edit {
                path: path.clone(),
                before: Some(content.clone()),
                after: None,
            });
        }
        if let Some(cleaned) = remove_block(&self.defaults) {
            edits.push(self.defaults_edit(cleaned));
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

/// kokuin's settings, between its markers.
pub fn block(parameters: &str) -> String {
    format!(
        "{BEGIN}\nGRUB_CMDLINE_LINUX_DEFAULT=\"$GRUB_CMDLINE_LINUX_DEFAULT {parameters}\"\n{END}\n"
    )
}

/// Appends the block at the end, on lines of its own.
pub fn add_block(text: &str, block: &str) -> String {
    let mut out = text.to_owned();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(block);
    out
}

/// The text without kokuin's block, or `None` if it has none.
pub fn remove_block(text: &str) -> Option<String> {
    let (start, end) = find_block(text)?;
    Some(format!("{}{}", &text[..start], &text[end..]))
}

/// Byte range of the block, from its first line to the end of its last line.
fn find_block(text: &str) -> Option<(usize, usize)> {
    let start = text.find(BEGIN)?;
    let end_marker = start + text[start..].find(END)?;
    let end = text[end_marker..]
        .find('\n')
        .map_or(text.len(), |n| end_marker + n + 1);
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAMETERS: &str = "init=/usr/lib/kokuin/kokuin-init";
    const ALPINE: &str = "GRUB_TIMEOUT=2\nGRUB_DISABLE_SUBMENU=y\n\
        GRUB_CMDLINE_LINUX_DEFAULT=\"modules=sd-mod,usb-storage,ext4 quiet\"\n";

    fn grub(drop_ins: bool) -> Grub {
        Grub {
            defaults_path: PathBuf::from("/etc/default/grub"),
            defaults: ALPINE.into(),
            drop_in_path: drop_ins.then(|| PathBuf::from("/etc/default/grub.d/kokuin.cfg")),
            drop_in: None,
            regenerate: vec!["/usr/sbin/update-grub".into()],
        }
    }

    #[test]
    fn adds_a_block_and_removes_exactly_it() {
        let grub = grub(false);
        let plan = grub.plan_enable(PARAMETERS);
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(plan.regenerate, ["/usr/sbin/update-grub"]);
        let enabled = plan.edits[0].after.clone().unwrap();
        assert!(enabled.starts_with(ALPINE));
        assert!(enabled.contains(
            "GRUB_CMDLINE_LINUX_DEFAULT=\"$GRUB_CMDLINE_LINUX_DEFAULT init=/usr/lib/kokuin/kokuin-init\""
        ));

        let enabled_grub = Grub {
            defaults: enabled,
            ..grub
        };
        assert!(enabled_grub.is_enabled());
        let disabled = enabled_grub.plan_disable();
        assert_eq!(disabled.edits[0].after.as_deref(), Some(ALPINE));
    }

    #[test]
    fn enabling_twice_changes_nothing() {
        let grub = grub(false);
        let once = grub.plan_enable(PARAMETERS).edits[0].after.clone().unwrap();
        let again = Grub {
            defaults: once,
            ..grub
        };
        assert!(again.plan_enable(PARAMETERS).is_empty());
    }

    #[test]
    fn new_parameters_replace_the_old_block() {
        let grub = grub(false);
        let once = grub.plan_enable(PARAMETERS).edits[0].after.clone().unwrap();
        let changed = Grub {
            defaults: once,
            ..grub
        }
        .plan_enable("init=/usr/lib/kokuin/kokuin-init kokuin_init=/sbin/openrc-init");
        let after = changed.edits[0].after.as_deref().unwrap();
        assert_eq!(after.matches(BEGIN).count(), 1);
        assert!(after.contains("kokuin_init=/sbin/openrc-init"));
    }

    #[test]
    fn uses_its_own_file_where_grub_reads_them() {
        let grub = grub(true);
        let plan = grub.plan_enable(PARAMETERS);
        assert_eq!(plan.edits.len(), 1);
        assert_eq!(
            plan.edits[0].path,
            PathBuf::from("/etc/default/grub.d/kokuin.cfg")
        );
        assert_eq!(plan.edits[0].before, None);

        let enabled = Grub {
            drop_in: plan.edits[0].after.clone(),
            ..grub
        };
        assert!(enabled.is_enabled());
        let disable = enabled.plan_disable();
        assert_eq!(disable.edits.len(), 1);
        assert_eq!(disable.edits[0].after, None);
    }

    #[test]
    fn disabling_what_is_not_enabled_does_nothing() {
        let plan = grub(true).plan_disable();
        assert!(plan.is_empty());
        assert!(plan.regenerate.is_empty());
    }

    #[test]
    fn a_file_without_final_newline_gets_one_before_the_block() {
        let text = add_block("GRUB_TIMEOUT=2", &block(PARAMETERS));
        assert!(text.starts_with("GRUB_TIMEOUT=2\n# BEGIN kokuin"));
        assert_eq!(remove_block(&text).as_deref(), Some("GRUB_TIMEOUT=2\n"));
    }

    #[test]
    fn reads_a_system_layout() {
        let root = std::env::temp_dir().join(format!("kokuin-grub-{}", std::process::id()));
        fs::create_dir_all(root.join("etc/default")).unwrap();
        fs::create_dir_all(root.join("usr/sbin")).unwrap();
        fs::create_dir_all(root.join("boot/grub2")).unwrap();
        fs::write(root.join(DEFAULTS), ALPINE).unwrap();
        fs::write(root.join("usr/sbin/grub2-mkconfig"), "#!/bin/sh\n").unwrap();
        fs::write(root.join("boot/grub2/grub.cfg"), "").unwrap();
        let grub = Grub::read(&root);
        fs::remove_dir_all(&root).unwrap();

        let grub = grub.unwrap();
        assert_eq!(grub.defaults, ALPINE);
        assert_eq!(grub.drop_in_path, None);
        assert_eq!(
            grub.regenerate,
            ["/usr/sbin/grub2-mkconfig", "-o", "/boot/grub2/grub.cfg"]
        );
    }
}
