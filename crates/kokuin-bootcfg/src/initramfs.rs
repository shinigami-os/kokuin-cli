//! kokuin's hook in the initramfs: starts the recorder at the very beginning of the
//! boot, before the real root filesystem is mounted, when the kernel command line has
//! [`RECORD_MARKER`](crate::RECORD_MARKER).
//!
//! The hook files are in the repository's `dist/initramfs/`, one set per initramfs
//! generator, embedded here. Enabling writes them where the generator looks, then
//! regenerates the initramfs; disabling removes them and regenerates it again.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{Edit, Plan};

/// The tools that build the initramfs, in the order they are looked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generator {
    /// Fedora, the RHEL family, openSUSE, Void, Gentoo, Ubuntu since 25.10.
    Dracut,
    /// Arch.
    Mkinitcpio,
    /// Debian, Ubuntu before 25.10.
    InitramfsTools,
}

impl Generator {
    /// Its name.
    pub fn name(self) -> &'static str {
        match self {
            Generator::Dracut => "dracut",
            Generator::Mkinitcpio => "mkinitcpio",
            Generator::InitramfsTools => "initramfs-tools",
        }
    }
}

const UNIT: &str = include_str!("../../../dist/initramfs/kokuin-recorder.service");
const DRACUT_SETUP: &str = include_str!("../../../dist/initramfs/dracut/module-setup.sh");
const DRACUT_START: &str = include_str!("../../../dist/initramfs/dracut/kokuin-start.sh");
const DRACUT_CONF: &str = "# Added by `kokuin-cli enable`, removed by `kokuin-cli disable`.\nadd_dracutmodules+=\" kokuin \"\n";
const TOOLS_HOOK: &str = include_str!("../../../dist/initramfs/initramfs-tools/hook");
const TOOLS_INIT_TOP: &str = include_str!("../../../dist/initramfs/initramfs-tools/init-top");
const MKINITCPIO_INSTALL: &str = include_str!("../../../dist/initramfs/mkinitcpio/install");
const MKINITCPIO_HOOK: &str = include_str!("../../../dist/initramfs/mkinitcpio/hook");
const MKINITCPIO_CONF: &str =
    "# Added by `kokuin-cli enable`, removed by `kokuin-cli disable`.\nHOOKS+=(kokuin)\n";

/// A file of the hook.
struct HookFile {
    /// Where it goes.
    path: &'static str,
    content: &'static str,
    executable: bool,
    /// It lives in a folder of its own, removed with it.
    own_folder: bool,
}

const fn file(path: &'static str, content: &'static str) -> HookFile {
    HookFile {
        path,
        content,
        executable: false,
        own_folder: false,
    }
}

/// The initramfs generator of a system, and what kokuin installed for it.
#[derive(Debug, Clone)]
pub struct Initramfs {
    root: PathBuf,
    /// The generator found.
    pub generator: Generator,
    /// The system runs systemd: the unit goes on the real root too.
    systemd: bool,
}

impl Initramfs {
    /// The running system's generator, if kokuin knows it.
    pub fn read() -> Option<Self> {
        Self::under(Path::new("/"))
    }

    /// The generator of a system laid out under `root`.
    pub fn under(root: &Path) -> Option<Self> {
        let exists = |path: &str| root.join(path).exists();
        let program = |name: &str| {
            ["usr/bin", "usr/sbin", "bin", "sbin"]
                .iter()
                .any(|dir| root.join(dir).join(name).is_file())
        };
        let generator = if program("dracut") && exists("usr/lib/dracut/modules.d") {
            Generator::Dracut
        } else if program("mkinitcpio") {
            Generator::Mkinitcpio
        } else if program("update-initramfs") && exists("usr/share/initramfs-tools") {
            Generator::InitramfsTools
        } else {
            return None;
        };
        Some(Self {
            root: root.to_owned(),
            generator,
            systemd: exists("run/systemd/system"),
        })
    }

    fn files(&self) -> Vec<HookFile> {
        let mut files = match self.generator {
            Generator::Dracut => vec![
                HookFile {
                    executable: true,
                    own_folder: true,
                    ..file(
                        "usr/lib/dracut/modules.d/95kokuin/module-setup.sh",
                        DRACUT_SETUP,
                    )
                },
                HookFile {
                    executable: true,
                    own_folder: true,
                    ..file(
                        "usr/lib/dracut/modules.d/95kokuin/kokuin-start.sh",
                        DRACUT_START,
                    )
                },
                HookFile {
                    own_folder: true,
                    ..file(
                        "usr/lib/dracut/modules.d/95kokuin/kokuin-recorder.service",
                        UNIT,
                    )
                },
                file("etc/dracut.conf.d/kokuin.conf", DRACUT_CONF),
            ],
            Generator::Mkinitcpio => vec![
                file("etc/initcpio/install/kokuin", MKINITCPIO_INSTALL),
                file("etc/initcpio/hooks/kokuin", MKINITCPIO_HOOK),
                file("etc/mkinitcpio.conf.d/kokuin.conf", MKINITCPIO_CONF),
            ],
            Generator::InitramfsTools => vec![
                HookFile {
                    executable: true,
                    ..file("etc/initramfs-tools/hooks/kokuin", TOOLS_HOOK)
                },
                HookFile {
                    executable: true,
                    ..file(
                        "etc/initramfs-tools/scripts/init-top/kokuin",
                        TOOLS_INIT_TOP,
                    )
                },
            ],
        };
        // A process started by systemd in the initramfs keeps its unit afterwards.
        if self.systemd && self.generator != Generator::InitramfsTools {
            files.push(file("etc/systemd/system/kokuin-recorder.service", UNIT));
        }
        files
    }

    /// Whether kokuin's hook is installed.
    pub fn is_enabled(&self) -> bool {
        self.files()
            .iter()
            .all(|file| self.root.join(file.path).is_file())
    }

    /// Installs the hook, then regenerates the initramfs.
    pub fn plan_enable(&self) -> Plan {
        let edits = self
            .files()
            .into_iter()
            .filter_map(|file| {
                let path = self.root.join(file.path);
                let before = fs::read_to_string(&path).ok();
                if before.as_deref() == Some(file.content) {
                    return None;
                }
                let mut edit = Edit::new(path, before, Some(file.content.to_owned()));
                edit.executable = file.executable;
                edit.own_folder = file.own_folder;
                Some(edit)
            })
            .collect();
        self.plan(edits)
    }

    /// Removes the hook, then regenerates the initramfs.
    pub fn plan_disable(&self) -> Plan {
        let edits = self
            .files()
            .into_iter()
            .filter_map(|file| {
                let path = self.root.join(file.path);
                let before = fs::read_to_string(&path).ok()?;
                let mut edit = Edit::new(path, Some(before), None);
                edit.own_folder = file.own_folder;
                Some(edit)
            })
            .collect();
        self.plan(edits)
    }

    fn plan(&self, edits: Vec<Edit>) -> Plan {
        let regenerate = match self.generator {
            Generator::Dracut => vec!["dracut", "--force", "--regenerate-all"],
            Generator::Mkinitcpio => vec!["mkinitcpio", "-P"],
            Generator::InitramfsTools => vec!["update-initramfs", "-u", "-k", "all"],
        };
        Plan {
            title: format!("Initramfs: {}", self.generator.name()),
            regenerate: if edits.is_empty() {
                Vec::new()
            } else {
                regenerate.into_iter().map(str::to_owned).collect()
            },
            edits,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A system with one generator's program installed, under a fresh folder.
    fn system(name: &str, programs: &[&str], folders: &[&str]) -> (Initramfs, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("kokuin-initramfs-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("usr/bin")).unwrap();
        for program in programs {
            fs::write(root.join("usr/bin").join(program), "").unwrap();
        }
        for folder in folders {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        (Initramfs::under(&root).unwrap(), root)
    }

    #[test]
    fn finds_the_generator() {
        let (dracut, root) = system(
            "detect",
            &["dracut", "update-initramfs"],
            &["usr/lib/dracut/modules.d", "usr/share/initramfs-tools"],
        );
        fs::remove_dir_all(root).unwrap();
        // Ubuntu 26.04 keeps update-initramfs next to dracut: dracut wins.
        assert_eq!(dracut.generator, Generator::Dracut);
        let none =
            std::env::temp_dir().join(format!("kokuin-initramfs-{}-none", std::process::id()));
        assert!(Initramfs::under(&none).is_none());
    }

    #[test]
    fn installs_and_removes_the_dracut_module() {
        let (dracut, root) = system(
            "dracut",
            &["dracut"],
            &["usr/lib/dracut/modules.d", "run/systemd/system"],
        );
        let enable = dracut.plan_enable();
        assert_eq!(enable.title, "Initramfs: dracut");
        assert_eq!(enable.regenerate, ["dracut", "--force", "--regenerate-all"]);
        // The module's three files, the configuration, and the unit on the real root.
        assert_eq!(enable.edits.len(), 5);
        let setup = &enable.edits[0];
        assert!(setup.executable && setup.own_folder);
        assert!(
            setup
                .after
                .as_deref()
                .unwrap()
                .contains("inst_binary /usr/bin/kokuin-cli")
        );

        crate::apply(&Plan {
            regenerate: Vec::new(),
            ..enable
        })
        .unwrap();
        let module = root.join("usr/lib/dracut/modules.d/95kokuin/module-setup.sh");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&module).unwrap().permissions().mode() & 0o777,
            0o755
        );
        assert!(dracut.is_enabled());
        assert!(dracut.plan_enable().edits.is_empty());

        let disable = dracut.plan_disable();
        crate::apply(&Plan {
            regenerate: Vec::new(),
            ..disable
        })
        .unwrap();
        assert!(!dracut.is_enabled());
        assert!(!root.join("usr/lib/dracut/modules.d/95kokuin").exists());
        assert!(root.join("usr/lib/dracut/modules.d").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initramfs_tools_needs_no_unit_and_mkinitcpio_a_drop_in() {
        let (tools, root) = system(
            "tools",
            &["update-initramfs"],
            &["usr/share/initramfs-tools", "run/systemd/system"],
        );
        let paths: Vec<String> = tools
            .plan_enable()
            .edits
            .iter()
            .map(|e| e.path.strip_prefix(&root).unwrap().display().to_string())
            .collect();
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            paths,
            [
                "etc/initramfs-tools/hooks/kokuin",
                "etc/initramfs-tools/scripts/init-top/kokuin"
            ]
        );

        let (arch, root) = system("arch", &["mkinitcpio"], &[]);
        let plan = arch.plan_enable();
        fs::remove_dir_all(&root).unwrap();
        assert_eq!(plan.regenerate, ["mkinitcpio", "-P"]);
        assert!(
            plan.edits
                .iter()
                .any(|e| e.after.as_deref() == Some(MKINITCPIO_CONF))
        );
    }
}
