//! Adds and removes kokuin's kernel parameters in the bootloader configuration.
//!
//! The kernel command line comes from the bootloader's configuration. To record a
//! boot, the kernel must start `kokuin-init` instead of the usual init: that is the
//! `init=` parameter. kokuin's parameters come in independent [`Section`]s.
//! [`plan_enable`] and [`plan_disable`] work out the change from the facts gathered
//! by [`System::read`], without touching anything; [`apply`] does it.
//!
//! Supported: GRUB. Elsewhere, [`manual_instructions`] tells how to do it by hand.

pub mod grub;

use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

/// Where kokuin-init is installed.
pub const KOKUIN_INIT: &str = "/usr/lib/kokuin/kokuin-init";

/// The kernel parameters that make the kernel start kokuin-init. `current_init` is
/// the `init=` already on the command line, if any: kokuin-init will start it.
pub fn parameters(current_init: Option<&str>) -> String {
    let mut parameters = format!("init={KOKUIN_INIT}");
    if let Some(init) = current_init.filter(|init| *init != KOKUIN_INIT) {
        parameters.push_str(&format!(" kokuin_init={init}"));
    }
    parameters
}

/// Marks a boot recorded with kernel tracing. A dotted parameter: the kernel does not
/// hand it to init.
pub const DEEP_MARKER: &str = "kokuin.deep=1";

/// What a deep boot asks the kernel, besides starting kokuin-init:
///
/// - `initcall_debug`: log each kernel init function, with its duration;
/// - `log_buf_len=16M`: room in the kernel log for these thousands of lines;
/// - `trace_event=sched:sched_process_exec`: trace every program started, with its
///   file name, `trace_clock=mono` on the boot clock, in an 8 MB buffer per CPU.
pub const DEEP_TRACING: &str = "initcall_debug log_buf_len=16M \
    trace_event=sched:sched_process_exec trace_clock=mono trace_buf_size=8M";

/// The kernel parameters of a deep boot (see [`parameters`] for `current_init`).
pub fn deep_parameters(current_init: Option<&str>) -> String {
    format!("{} {DEEP_TRACING} {DEEP_MARKER}", parameters(current_init))
}

/// Whether a kernel command line is that of a deep boot.
pub fn is_deep_boot(cmdline: &str) -> bool {
    cmdline.split_whitespace().any(|word| word == DEEP_MARKER)
}

/// The `init=` of a kernel command line (the last one wins, as in the kernel).
pub fn init_parameter(cmdline: &str) -> Option<&str> {
    cmdline
        .split_whitespace()
        .filter_map(|word| word.strip_prefix("init="))
        .next_back()
}

/// A set of kernel parameters kokuin adds, and removes, as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Section {
    /// Record every boot (`kokuin-cli enable`).
    Recording,
    /// Record the next boot with kernel tracing (`kokuin-cli deep`).
    Deep,
}

impl Section {
    /// Every section.
    pub const ALL: [Section; 2] = [Section::Recording, Section::Deep];

    /// Its name in the bootloader configuration.
    pub fn name(self) -> &'static str {
        match self {
            Section::Recording => "kokuin",
            Section::Deep => "kokuin-deep",
        }
    }

    /// Who adds and removes it, for whoever reads the configuration.
    pub fn lifetime(self) -> &'static str {
        match self {
            Section::Recording => "added by `kokuin-cli enable`, removed by `kokuin-cli disable`",
            Section::Deep => {
                "added by `kokuin-cli deep` for one boot, removed after it or by \
                 `kokuin-cli deep --cancel`"
            }
        }
    }
}

/// What the system looks like, as far as the bootloader configuration goes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct System {
    /// GRUB, if its configuration tools are installed.
    pub grub: Option<grub::Grub>,
}

impl System {
    /// Looks at the running system.
    pub fn read() -> Self {
        Self {
            grub: grub::Grub::read(Path::new("/")),
        }
    }
}

/// A file to write or delete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Edit {
    /// The file.
    pub path: PathBuf,
    /// Its content now, `None` if it does not exist.
    pub before: Option<String>,
    /// Its content afterwards, `None` to delete it.
    pub after: Option<String>,
}

/// What enabling or disabling does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The bootloader, in words.
    pub bootloader: String,
    /// Files to change.
    pub edits: Vec<Edit>,
    /// The command that regenerates the bootloader's configuration afterwards.
    pub regenerate: Vec<String>,
}

impl Plan {
    /// Whether there is nothing to do.
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }
}

/// Why the change cannot be planned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanError {
    /// No supported bootloader: the parameters must be added by hand.
    Unsupported,
}

/// The change that adds a section with `parameters` to every normal boot entry.
pub fn plan_enable(system: &System, section: Section, parameters: &str) -> Result<Plan, PlanError> {
    let grub = system.grub.as_ref().ok_or(PlanError::Unsupported)?;
    Ok(grub.plan_enable(section, parameters))
}

/// The change that removes sections [`plan_enable`] added, in one go.
pub fn plan_disable(system: &System, sections: &[Section]) -> Result<Plan, PlanError> {
    let grub = system.grub.as_ref().ok_or(PlanError::Unsupported)?;
    Ok(grub.plan_disable(sections))
}

/// Whether a section is in the bootloader configuration.
pub fn is_enabled(system: &System, section: Section) -> bool {
    system
        .grub
        .as_ref()
        .is_some_and(|grub| grub.is_enabled(section))
}

/// Writes the files, then regenerates the bootloader configuration. The first time
/// kokuin changes a file, the original is saved next to it, with `.kokuin-backup`
/// added to its name; the backup goes once the file is back to the original.
pub fn apply(plan: &Plan) -> io::Result<()> {
    for edit in &plan.edits {
        // The backup keeps the file as it was before kokuin ever changed it.
        let backup = PathBuf::from(format!("{}.kokuin-backup", edit.path.display()));
        if let (Some(before), Some(_)) = (&edit.before, &edit.after) {
            if !backup.exists() {
                fs::write(&backup, before)?;
            }
        }
        match &edit.after {
            Some(after) => {
                if let Some(parent) = edit.path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&edit.path, after)?;
                // Back to the original: the backup has nothing more to keep.
                if fs::read_to_string(&backup).is_ok_and(|original| original == *after) {
                    fs::remove_file(&backup)?;
                }
            }
            None => match fs::remove_file(&edit.path) {
                Err(err) if err.kind() != io::ErrorKind::NotFound => return Err(err),
                _ => {}
            },
        }
    }
    let Some((program, args)) = plan.regenerate.split_first() else {
        return Ok(());
    };
    let output = Command::new(program).args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "`{}` failed: {}",
            plan.regenerate.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

/// How to add the parameters by hand, for a bootloader kokuin cannot configure.
pub fn manual_instructions(parameters: &str) -> String {
    format!(
        "Add these parameters to the kernel command line of your boot entry:\n\n    \
         {parameters}\n\n\
         With systemd-boot, add them to the `options` line of the entry in \
         /boot/loader/entries/ (or to /etc/kernel/cmdline if the entries are generated). \
         With an EFI stub entry, recreate it with efibootmgr and these parameters.\n\
         To stop recording, remove them again."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_an_existing_init() {
        assert_eq!(parameters(None), "init=/usr/lib/kokuin/kokuin-init");
        assert_eq!(
            parameters(Some("/sbin/openrc-init")),
            "init=/usr/lib/kokuin/kokuin-init kokuin_init=/sbin/openrc-init"
        );
        assert_eq!(
            parameters(Some(KOKUIN_INIT)),
            "init=/usr/lib/kokuin/kokuin-init"
        );
    }

    #[test]
    fn a_deep_boot_is_marked() {
        let parameters = deep_parameters(None);
        assert!(parameters.starts_with("init=/usr/lib/kokuin/kokuin-init initcall_debug "));
        assert!(is_deep_boot(&format!("ro quiet {parameters}")));
        assert!(!is_deep_boot("ro quiet kokuin.deep=0"));
    }

    #[test]
    fn finds_the_last_init_parameter() {
        assert_eq!(init_parameter("ro quiet"), None);
        assert_eq!(
            init_parameter("ro init=/sbin/a quiet init=/sbin/b"),
            Some("/sbin/b")
        );
    }

    #[test]
    fn keeps_the_original_until_it_is_restored() {
        let dir = std::env::temp_dir().join(format!("kokuin-apply-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("grub");
        let backup = dir.join("grub.kokuin-backup");
        fs::write(&path, "original\n").unwrap();
        let change = |before: &str, after: &str| Plan {
            bootloader: "test".into(),
            edits: vec![Edit {
                path: path.clone(),
                before: Some(before.into()),
                after: Some(after.into()),
            }],
            regenerate: Vec::new(),
        };

        apply(&change("original\n", "enabled\n")).unwrap();
        apply(&change("enabled\n", "enabled again\n")).unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), "original\n");
        apply(&change("enabled again\n", "original\n")).unwrap();
        let backup_left = backup.exists();
        fs::remove_dir_all(&dir).unwrap();
        assert!(!backup_left);
    }

    #[test]
    fn without_grub_it_is_done_by_hand() {
        let system = System::default();
        assert_eq!(
            plan_enable(&system, Section::Recording, "init=x"),
            Err(PlanError::Unsupported)
        );
        assert!(!is_enabled(&system, Section::Recording));
    }
}
