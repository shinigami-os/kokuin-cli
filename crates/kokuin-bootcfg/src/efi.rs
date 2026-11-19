//! kokuin's EFI probe in the firmware's boot entries.
//!
//! The probe lives on the EFI system partition (ESP), in `\EFI\kokuin\probe.efi`,
//! with a boot entry of its own (`Boot####`, "kokuin probe") that is not in
//! `BootOrder`: the firmware never starts it by itself. Arming sets `BootNext` to
//! it: the firmware boots it once, then forgets.
//!
//! Variables go through efivarfs (`/sys/firmware/efi/efivars/Name-GUID`): 4 bytes of
//! attributes, then the value. The kernel lets `Boot####` and `BootNext` be written
//! and removed there.
//!
//! As for GRUB, each operation is first a list of [`Action`]s, shown, then applied.

use std::{fs, path::PathBuf};

use kokuin_sources::efi::{
    self, LOAD_OPTION_ACTIVE, Node, RawLoadOption, encode_device_path, utf16z,
};

/// The UEFI specification's own variables.
const GLOBAL: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// Non-volatile, boot services and runtime access.
const ATTRIBUTES: u32 = 0x7;
/// The name of the probe's boot entry.
pub use kokuin_probe_format::ENTRY_DESCRIPTION as DESCRIPTION;
/// Where the probe goes on the ESP, as the firmware writes paths.
pub const PROBE_PATH: &str = "\\EFI\\kokuin\\probe.efi";
/// Where the probe is installed on the system.
pub const PROBE_FILE: &str = "/usr/lib/kokuin/kokuin-probe.efi";

/// What a system looks like for the probe, under `root` (`/` for real).
#[derive(Debug, Clone)]
pub struct Firmware {
    root: PathBuf,
}

/// The probe's boot entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeEntry {
    /// Its number (`Boot0009`).
    pub number: u16,
    /// The entry it starts.
    pub target: Option<u16>,
}

/// One change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Copy the probe to the ESP.
    CopyProbe {
        /// The installed probe.
        from: PathBuf,
        /// Its place on the ESP.
        to: PathBuf,
    },
    /// Write a boot entry.
    WriteEntry {
        /// The entry's number.
        number: u16,
        /// The entry.
        option: RawLoadOption,
    },
    /// Boot this entry next time, once.
    SetBootNext(u16),
    /// Forget the one-time entry.
    ClearBootNext,
    /// Delete a boot entry.
    DeleteEntry(u16),
    /// Delete the probe from the ESP.
    RemoveProbe(PathBuf),
}

impl Action {
    /// The action in words.
    pub fn describe(&self) -> String {
        match self {
            Action::CopyProbe { from, to } => {
                format!("Copy {} to {}", from.display(), to.display())
            }
            Action::WriteEntry { number, option } => {
                let target = efi::utf16(&option.optional_data);
                format!(
                    "Create the boot entry Boot{number:04X} \u{201c}{}\u{201d} for {}, which starts {target} \
                     (not added to BootOrder)",
                    option.description, PROBE_PATH
                )
            }
            Action::SetBootNext(number) => {
                format!(
                    "Set BootNext to Boot{number:04X}: the next boot goes through the probe, once"
                )
            }
            Action::ClearBootNext => "Clear BootNext".to_owned(),
            Action::DeleteEntry(number) => format!("Delete the boot entry Boot{number:04X}"),
            Action::RemoveProbe(path) => format!("Delete {}", path.display()),
        }
    }
}

impl Firmware {
    /// The running system.
    pub fn live() -> Self {
        Self::under("/")
    }

    /// A system laid out under `root`, for tests.
    pub fn under(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, absolute: &str) -> PathBuf {
        self.root.join(absolute.trim_start_matches('/'))
    }

    fn variable_path(&self, name: &str, guid: &str) -> PathBuf {
        self.path(&format!("/sys/firmware/efi/efivars/{name}-{guid}"))
    }

    /// A global variable's value, without its attributes.
    fn variable(&self, name: &str) -> Option<Vec<u8>> {
        let bytes = fs::read(self.variable_path(name, GLOBAL)).ok()?;
        Some(bytes.get(4..)?.to_vec())
    }

    fn variable_u16(&self, name: &str) -> Option<u16> {
        let value = self.variable(name)?;
        Some(u16::from_le_bytes([*value.first()?, *value.get(1)?]))
    }

    /// Whether the system booted through UEFI.
    pub fn is_uefi(&self) -> bool {
        self.path("/sys/firmware/efi").exists()
    }

    /// Whether Secure Boot is on.
    pub fn secure_boot(&self) -> bool {
        self.variable("SecureBoot").and_then(|v| v.first().copied()) == Some(1)
    }

    /// The boot entries that exist, by number.
    fn entries(&self) -> Vec<u16> {
        let mut numbers: Vec<u16> = fs::read_dir(self.path("/sys/firmware/efi/efivars"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let number = name
                    .strip_prefix("Boot")?
                    .strip_suffix(&format!("-{GLOBAL}"))?;
                (number.len() == 4)
                    .then(|| u16::from_str_radix(number, 16).ok())
                    .flatten()
            })
            .collect();
        numbers.sort_unstable();
        numbers
    }

    /// The probe's boot entry, if it was installed.
    pub fn probe_entry(&self) -> Option<ProbeEntry> {
        self.entries().into_iter().find_map(|number| {
            let option = RawLoadOption::parse(&self.variable(&format!("Boot{number:04X}"))?)?;
            (option.description == DESCRIPTION).then(|| ProbeEntry {
                number,
                target: efi::utf16(&option.optional_data)
                    .strip_prefix("Boot")
                    .and_then(|n| u16::from_str_radix(n, 16).ok()),
            })
        })
    }

    /// Whether an entry exists and is not a probe's.
    fn is_usual_entry(&self, number: u16) -> bool {
        self.variable(&format!("Boot{number:04X}"))
            .and_then(|bytes| RawLoadOption::parse(&bytes))
            .is_some_and(|option| option.description != DESCRIPTION)
    }

    /// Whether the next boot goes through the probe.
    pub fn is_armed(&self) -> bool {
        let next = self.variable_u16("BootNext");
        next.is_some() && next == self.probe_entry().map(|e| e.number)
    }

    /// Installs the probe: copied to the ESP, with a boot entry starting the entry
    /// that booted this system.
    pub fn plan_install(&self) -> Result<Vec<Action>, String> {
        if !self.is_uefi() {
            return Err(
                "this system did not boot through UEFI: the probe is an EFI program".into(),
            );
        }
        if self.secure_boot() {
            return Err(
                "Secure Boot is on: the firmware only starts programs signed with a key it \
                 trusts, and the probe is not signed. Turn Secure Boot off to use it."
                    .into(),
            );
        }
        let probe = self.path(PROBE_FILE);
        if !probe.is_file() {
            return Err(format!("the probe is not installed at {PROBE_FILE}"));
        }
        let existing = self.probe_entry();
        let current = self.variable_u16("BootCurrent");
        // Installing again from a boot that went through the probe: keep its target.
        let target = match (&existing, current) {
            (Some(entry), Some(current)) if entry.number == current => entry.target,
            (_, current) => current,
        };
        let number = match &existing {
            Some(entry) => entry.number,
            None => self.free_entry().ok_or("no free boot entry number")?,
        };
        // The probe must start the system's usual entry, never itself.
        let target = target
            .filter(|&target| target != number && self.is_usual_entry(target))
            .ok_or(
                "the entry that booted this system is not known, or is the probe itself: \
                 boot once without the probe, then install it again",
            )?;
        let esp = self.esp()?;
        let option = RawLoadOption {
            attributes: LOAD_OPTION_ACTIVE,
            description: DESCRIPTION.into(),
            device_path: encode_device_path(&[esp.node, Node::file_path(PROBE_PATH)]),
            optional_data: utf16z(&format!("Boot{target:04X}")),
        };
        Ok(vec![
            Action::CopyProbe {
                from: probe,
                to: esp.mount.join("EFI/kokuin/probe.efi"),
            },
            Action::WriteEntry { number, option },
        ])
    }

    /// Arms the probe for the next boot.
    pub fn plan_arm(&self) -> Result<Vec<Action>, String> {
        let entry = self
            .probe_entry()
            .ok_or("the probe is not installed: run `kokuin-cli probe install` first")?;
        if self.secure_boot() {
            return Err("Secure Boot is on: the firmware would refuse the unsigned probe".into());
        }
        Ok(if self.is_armed() {
            Vec::new()
        } else {
            vec![Action::SetBootNext(entry.number)]
        })
    }

    /// Disarms the probe if it is armed.
    pub fn plan_disarm(&self) -> Vec<Action> {
        if self.is_armed() {
            vec![Action::ClearBootNext]
        } else {
            Vec::new()
        }
    }

    /// Removes what [`Firmware::plan_install`] did.
    pub fn plan_remove(&self) -> Vec<Action> {
        let mut actions = self.plan_disarm();
        if let Some(entry) = self.probe_entry() {
            actions.push(Action::DeleteEntry(entry.number));
        }
        if let Ok(esp) = self.esp() {
            let probe = esp.mount.join("EFI/kokuin/probe.efi");
            if probe.exists() {
                actions.push(Action::RemoveProbe(probe));
            }
        }
        actions
    }

    /// Makes the changes, in order.
    pub fn apply(&self, actions: &[Action]) -> std::io::Result<()> {
        for action in actions {
            match action {
                Action::CopyProbe { from, to } => {
                    if let Some(dir) = to.parent() {
                        fs::create_dir_all(dir)?;
                    }
                    fs::copy(from, to)?;
                }
                Action::WriteEntry { number, option } => {
                    self.write_variable(&format!("Boot{number:04X}"), &option.encode())?;
                }
                Action::SetBootNext(number) => {
                    self.write_variable("BootNext", &number.to_le_bytes())?;
                }
                Action::ClearBootNext => fs::remove_file(self.variable_path("BootNext", GLOBAL))?,
                Action::DeleteEntry(number) => {
                    fs::remove_file(self.variable_path(&format!("Boot{number:04X}"), GLOBAL))?;
                }
                Action::RemoveProbe(path) => {
                    fs::remove_file(path)?;
                    // The folder goes too when it is empty.
                    if let Some(dir) = path.parent() {
                        let _ = fs::remove_dir(dir);
                    }
                }
            }
        }
        Ok(())
    }

    /// Writes a global variable in one go, as efivarfs needs.
    fn write_variable(&self, name: &str, value: &[u8]) -> std::io::Result<()> {
        let mut bytes = ATTRIBUTES.to_le_bytes().to_vec();
        bytes.extend(value);
        fs::write(self.variable_path(name, GLOBAL), bytes)
    }

    fn free_entry(&self) -> Option<u16> {
        let used = self.entries();
        (0..=u16::MAX).find(|n| !used.contains(n))
    }

    /// The EFI system partition: where it is mounted, and its device path node.
    fn esp(&self) -> Result<Esp, String> {
        let mounts = fs::read_to_string(self.path("/proc/mounts"))
            .map_err(|err| format!("cannot read /proc/mounts: {err}"))?;
        let (device, mount) = mounts
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                Some((fields.next()?, fields.next()?, fields.next()?))
            })
            .filter(|(_, mount, kind)| {
                matches!(*kind, "vfat" | "msdos")
                    && matches!(*mount, "/boot/efi" | "/efi" | "/boot")
                    && self.path(mount).join("EFI").is_dir()
            })
            .map(|(device, mount, _)| (device.to_owned(), mount.to_owned()))
            .next()
            .ok_or("no EFI system partition is mounted on /boot/efi, /efi or /boot")?;
        let name = fs::canonicalize(self.path(&device))
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| device.rsplit('/').next().unwrap_or_default().to_owned());
        let block = self.path(&format!("/sys/class/block/{name}"));
        let number = |file: &str| -> Option<u64> {
            fs::read_to_string(block.join(file))
                .ok()?
                .trim()
                .parse()
                .ok()
        };
        let (Some(partition), Some(start), Some(size)) =
            (number("partition"), number("start"), number("size"))
        else {
            return Err(format!(
                "{device} is not a disk partition the kernel describes"
            ));
        };
        // sysfs counts 512-byte sectors; the device path counts the disk's blocks.
        let block_size = fs::read_to_string(block.join("../queue/logical_block_size"))
            .ok()
            .and_then(|v| v.trim().parse::<u64>().ok())
            .unwrap_or(512)
            .max(512);
        let guid = self
            .partition_guid(&name)
            .or_else(|| self.gpt_guid(&block, partition, block_size))
            .ok_or_else(|| {
                format!(
                    "the GPT partition GUID of {device} is unknown (no /dev/disk/by-partuuid, and the GPT could not be read)"
                )
            })?;
        Ok(Esp {
            mount: self.path(&mount),
            node: efi::hard_drive_node(
                u32::try_from(partition).unwrap_or_default(),
                start * 512 / block_size,
                size * 512 / block_size,
                guid,
            ),
        })
    }

    /// The GPT partition GUID read from the disk's own partition table, for systems whose udev makes no
    /// `/dev/disk/by-partuuid` (Kira's eudev has no blkid builtin). Needs read access to the disk (root).
    fn gpt_guid(&self, block: &std::path::Path, partition: u64, block_size: u64) -> Option<[u8; 16]> {
        use std::io::{Read, Seek, SeekFrom};
        let disk = fs::canonicalize(block).ok()?.parent()?.file_name()?.to_str()?.to_owned();
        let mut file = fs::File::open(self.path(&format!("/dev/{disk}"))).ok()?;
        let mut header = [0u8; 92];
        file.seek(SeekFrom::Start(block_size)).ok()?;
        file.read_exact(&mut header).ok()?;
        if &header[..8] != b"EFI PART" || partition == 0 {
            return None;
        }
        let entries = u64::from_le_bytes(header[72..80].try_into().ok()?);
        let entry_size = u64::from(u32::from_le_bytes(header[84..88].try_into().ok()?));
        let mut entry = [0u8; 32];
        file.seek(SeekFrom::Start(entries * block_size + (partition - 1) * entry_size)).ok()?;
        file.read_exact(&mut entry).ok()?;
        // the partition's unique GUID, in the mixed-endian form the device path wants as it is
        let guid: [u8; 16] = entry[16..32].try_into().ok()?;
        (guid != [0; 16]).then_some(guid)
    }

    /// The GPT partition GUID of a partition, from udev's `/dev/disk/by-partuuid`.
    fn partition_guid(&self, name: &str) -> Option<[u8; 16]> {
        let dir = self.path("/dev/disk/by-partuuid");
        fs::read_dir(&dir).ok()?.flatten().find_map(|entry| {
            let target = fs::read_link(entry.path()).ok()?;
            (target.file_name()?.to_str()? == name)
                .then(|| efi::guid_bytes(entry.file_name().to_str()?))
                .flatten()
        })
    }
}

/// The EFI system partition.
struct Esp {
    mount: PathBuf,
    node: Node,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ESP_GUID: &str = "ba072a52-3b27-4807-a6df-a15f978e7486";

    /// An Alpine-like system: booted from Boot0002, the ESP on vda1.
    fn system(name: &str, secure_boot: bool) -> (Firmware, PathBuf) {
        let root = std::env::temp_dir().join(format!("kokuin-efi-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let at = |p: &str| root.join(p.trim_start_matches('/'));
        let vars = at("/sys/firmware/efi/efivars");
        fs::create_dir_all(&vars).unwrap();
        let var = |name: &str, value: &[u8]| {
            let mut bytes = ATTRIBUTES.to_le_bytes().to_vec();
            bytes.extend(value);
            fs::write(vars.join(format!("{name}-{GLOBAL}")), bytes).unwrap();
        };
        var("BootCurrent", &2u16.to_le_bytes());
        var("SecureBoot", &[u8::from(secure_boot)]);
        for number in [0u16, 1, 2] {
            let option = RawLoadOption {
                attributes: LOAD_OPTION_ACTIVE,
                description: format!("entry {number}"),
                device_path: encode_device_path(&[]),
                optional_data: Vec::new(),
            };
            var(&format!("Boot{number:04X}"), &option.encode());
        }
        fs::create_dir_all(at("/proc")).unwrap();
        fs::write(
            at("/proc/mounts"),
            "/dev/vda2 / ext4 rw 0 0\n/dev/vda1 /boot/efi vfat rw 0 0\n",
        )
        .unwrap();
        fs::create_dir_all(at("/boot/efi/EFI/BOOT")).unwrap();
        fs::create_dir_all(at("/dev")).unwrap();
        fs::write(at("/dev/vda1"), "").unwrap();
        fs::create_dir_all(at("/sys/class/block/vda/queue")).unwrap();
        fs::write(at("/sys/class/block/vda/queue/logical_block_size"), "512\n").unwrap();
        fs::create_dir_all(at("/sys/class/block/vda1")).unwrap();
        // sysfs's partitions sit inside their disk: vda1/.. is vda.
        fs::remove_dir(at("/sys/class/block/vda1")).unwrap();
        fs::create_dir_all(at("/sys/class/block/vda/vda1")).unwrap();
        std::os::unix::fs::symlink(at("/sys/class/block/vda/vda1"), at("/sys/class/block/vda1"))
            .unwrap();
        for (file, value) in [("partition", "1"), ("start", "1024"), ("size", "1024")] {
            fs::write(at(&format!("/sys/class/block/vda/vda1/{file}")), value).unwrap();
        }
        fs::create_dir_all(at("/dev/disk/by-partuuid")).unwrap();
        std::os::unix::fs::symlink(
            "../../vda1",
            at(&format!("/dev/disk/by-partuuid/{ESP_GUID}")),
        )
        .unwrap();
        fs::create_dir_all(at("/usr/lib/kokuin")).unwrap();
        fs::write(at(PROBE_FILE), "MZ probe").unwrap();
        (Firmware::under(&root), root)
    }

    #[test]
    fn reads_the_partition_guid_from_the_gpt_when_udev_makes_no_link() {
        let (firmware, root) = system("gpt", false);
        fs::remove_dir_all(root.join("dev/disk")).unwrap();
        let mut disk = vec![0u8; 512 * 4];
        disk[512..520].copy_from_slice(b"EFI PART");
        disk[512 + 72..512 + 80].copy_from_slice(&2u64.to_le_bytes());
        disk[512 + 84..512 + 88].copy_from_slice(&128u32.to_le_bytes());
        disk[1024 + 16..1024 + 32].copy_from_slice(&efi::guid_bytes(ESP_GUID).unwrap());
        fs::write(root.join("dev/vda"), disk).unwrap();
        let install = firmware.plan_install().unwrap();
        let Action::WriteEntry { option, .. } = &install[1] else {
            panic!("an entry is written");
        };
        let path = efi::parse_device_path(&option.device_path);
        assert_eq!(path.partition.as_deref(), Some(ESP_GUID));
    }

    #[test]
    fn installs_arms_and_removes_the_probe() {
        let (firmware, root) = system("install", false);
        let install = firmware.plan_install().unwrap();
        let Action::WriteEntry { number, option } = &install[1] else {
            panic!("an entry is written");
        };
        assert_eq!(*number, 3);
        assert_eq!(efi::utf16(&option.optional_data), "Boot0002");
        let path = efi::parse_device_path(&option.device_path);
        assert_eq!(path.file.as_deref(), Some(PROBE_PATH));
        assert_eq!(path.partition.as_deref(), Some(ESP_GUID));
        assert!(matches!(&install[0], Action::CopyProbe { to, .. } if to.starts_with(&root)));
        firmware.apply(&install).unwrap();
        assert_eq!(
            firmware.probe_entry(),
            Some(ProbeEntry {
                number: 3,
                target: Some(2)
            })
        );
        // Installing again changes nothing but the copy and the same entry.
        assert!(matches!(
            &firmware.plan_install().unwrap()[1],
            Action::WriteEntry { number: 3, .. }
        ));

        assert!(!firmware.is_armed());
        firmware.apply(&firmware.plan_arm().unwrap()).unwrap();
        assert!(firmware.is_armed());
        assert!(firmware.plan_arm().unwrap().is_empty());

        let remove = firmware.plan_remove();
        assert_eq!(remove.len(), 3);
        firmware.apply(&remove).unwrap();
        assert_eq!(firmware.probe_entry(), None);
        assert!(!firmware.is_armed());
        assert!(!root.join("boot/efi/EFI/kokuin").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn never_points_the_probe_at_itself() {
        let (firmware, root) = system("itself", false);
        // Booted through an entry that no longer exists, with the number the probe
        // would get.
        let vars = root.join("sys/firmware/efi/efivars");
        let mut current = ATTRIBUTES.to_le_bytes().to_vec();
        current.extend(3u16.to_le_bytes());
        fs::write(vars.join(format!("BootCurrent-{GLOBAL}")), current).unwrap();
        let err = firmware.plan_install().unwrap_err();
        fs::remove_dir_all(root).unwrap();
        assert!(err.contains("boot once without the probe"));
    }

    #[test]
    fn refuses_with_secure_boot_on() {
        let (firmware, root) = system("secure", true);
        let err = firmware.plan_install().unwrap_err();
        fs::remove_dir_all(root).unwrap();
        assert!(err.contains("Secure Boot is on"));
    }
}
