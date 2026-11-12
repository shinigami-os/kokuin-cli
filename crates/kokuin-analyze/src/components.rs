//! Completes the system description from the evidence of all sources: bootloader,
//! UEFI details, init system explanation.

use kokuin_kb::{Kb, KmsgTag};
use kokuin_model::{BootRecord, ComponentInfo, EventKind};
use serde_json::Value;

use crate::item::Item;

pub(crate) fn complete(
    record: &mut BootRecord,
    init: Option<ComponentInfo>,
    items: &[Item],
    kb: &Kb,
) {
    let explain = |id: String| kb.article(&id).map(|_| id);

    if let Some(uefi) = &mut record.boot.machine.uefi {
        if let Some(item) = items.iter().find(|i| i.tag == Some(KmsgTag::EfiInfo)) {
            uefi.vendor = string(item, "vendor");
            uefi.spec_version = string(item, "spec");
        }
        uefi.secure_boot = secure_boot(items);
    }

    record.boot.bootloader =
        detect_bootloader(items, record.boot.kernel.cmdline.as_deref()).map(|mut bootloader| {
            bootloader.explain_id =
                explain(format!("bootloader.{}", bootloader.id.replace('-', "_")));
            bootloader
        });
    record.boot.init = init.map(|mut init| {
        init.explain_id = explain(format!("init.{}", init.id.replace('-', "_")));
        init
    });
}

fn string(item: &Item, key: &str) -> Option<String> {
    item.obs.data.get(key)?.as_str().map(str::to_owned)
}

fn secure_boot(items: &[Item]) -> Option<bool> {
    let from_variable = items
        .iter()
        .find(|i| {
            i.source == "efivars" && i.obs.data.get("variable") == Some(&Value::from("SecureBoot"))
        })
        .and_then(|i| i.obs.data.get("value")?.as_bool());
    let from_kernel = || {
        let item = items.iter().find(|i| {
            i.obs.explain_id.as_deref() == Some("firmware.secure_boot") && i.source == "kmsg"
        })?;
        Some(string(item, "state")? == "enabled")
    };
    from_variable.or_else(from_kernel)
}

/// Known bootloaders, recognized in file names and identification strings.
const BOOTLOADERS: [(&str, &str, &str); 5] = [
    ("systemd-boot", "systemd-boot", "systemd-boot"),
    ("grub", "grub", "GNU GRUB"),
    ("refind", "refind", "rEFInd"),
    ("limine", "limine", "Limine"),
    ("shim", "shim", "shim"),
];

fn known(text: &str) -> Option<&'static str> {
    let lower = text.to_lowercase();
    BOOTLOADERS
        .iter()
        .find(|(pattern, _, _)| lower.contains(pattern))
        .map(|(_, id, _)| *id)
}

fn display_name(id: &str) -> &str {
    match id {
        "efistub" => "Linux EFI stub (no bootloader)",
        _ => BOOTLOADERS
            .iter()
            .find(|(_, known, _)| *known == id)
            .map_or(id, |(_, _, name)| name),
    }
}

/// Identifies the bootloader, from the most to the least reliable evidence.
fn detect_bootloader(items: &[Item], cmdline: Option<&str>) -> Option<ComponentInfo> {
    let variable = |name: &str| {
        items.iter().find(|i| {
            i.source == "efivars" && i.obs.data.get("variable") == Some(&Value::from(name))
        })
    };
    let component = |id: &str, version: Option<String>, via_shim: bool| {
        let name = display_name(id);
        let name = if via_shim && id != "shim" {
            format!("{name} (started by shim)")
        } else {
            name.to_owned()
        };
        ComponentInfo {
            id: id.to_owned(),
            name: Some(name),
            version,
            explain_id: None,
        }
    };

    // Boot programs the firmware measured, and the entry it booted.
    let boot_programs: Vec<String> = items
        .iter()
        .filter(|i| i.source == "tpm_eventlog" && i.obs.kind == EventKind::Measurement)
        .filter(|i| {
            i.obs.data.get("event_type") == Some(&Value::from("EV_EFI_BOOT_SERVICES_APPLICATION"))
        })
        .filter_map(|i| string(i, "path"))
        .collect();
    let boot_entry_path = variable("BootCurrent").and_then(|i| string(i, "path"));
    let via_shim = boot_programs
        .iter()
        .chain(&boot_entry_path)
        .any(|p| known(p) == Some("shim"));

    // 1. The bootloader identifies itself (Boot Loader Interface).
    if let Some(info) = variable("LoaderInfo").and_then(|i| string(i, "value")) {
        let id = known(&info).unwrap_or("unknown");
        let version = info.split_whitespace().nth(1).map(str::to_owned);
        return Some(component(id, version, via_shim));
    }
    // 2. GRUB recorded its commands in the TPM log.
    if items
        .iter()
        .any(|i| i.obs.data.contains_key("grub_command"))
    {
        return Some(component("grub", None, via_shim));
    }
    // 3. A known bootloader among the measured boot programs (after shim).
    if let Some(id) = boot_programs
        .iter()
        .filter_map(|p| known(p))
        .find(|&id| id != "shim")
    {
        return Some(component(id, None, via_shim));
    }
    // 4. The boot entry file.
    if let Some(path) = &boot_entry_path {
        let file = path.rsplit('\\').next().unwrap_or(path).to_lowercase();
        if let Some(id) = known(&file) {
            return Some(component(id, None, via_shim));
        }
        if file.contains("vmlinuz") || file.contains("linux") || file.contains("bzimage") {
            return Some(component("efistub", None, false));
        }
    }
    // 5. GRUB (and syslinux) add BOOT_IMAGE= to the kernel command line.
    if cmdline.is_some_and(|c| {
        c.split_whitespace()
            .any(|arg| arg.starts_with("BOOT_IMAGE="))
    }) {
        return Some(component("grub", None, via_shim));
    }
    via_shim.then(|| component("shim", None, false))
}

#[cfg(test)]
mod tests {
    use kokuin_model::Confidence;
    use kokuin_sources::Observation;

    use super::*;

    fn item(source: &str, kind: EventKind, data: &[(&str, &str)]) -> Item {
        let mut obs = Observation::new(kind, "x", None, Confidence::Inferred);
        for (key, value) in data {
            obs = obs.with(key, *value);
        }
        Item::new(obs, source)
    }

    #[test]
    fn detects_grub_behind_shim() {
        let items = [
            item(
                "tpm_eventlog",
                EventKind::Measurement,
                &[
                    ("event_type", "EV_EFI_BOOT_SERVICES_APPLICATION"),
                    ("path", "\\EFI\\ubuntu\\shimx64.efi"),
                ],
            ),
            item(
                "tpm_eventlog",
                EventKind::Measurement,
                &[
                    ("event_type", "EV_EFI_BOOT_SERVICES_APPLICATION"),
                    ("path", "\\EFI\\ubuntu\\grubx64.efi"),
                ],
            ),
        ];
        let found = detect_bootloader(&items, None).unwrap();
        assert_eq!(found.id, "grub");
        assert_eq!(found.name.as_deref(), Some("GNU GRUB (started by shim)"));
    }

    #[test]
    fn detects_systemd_boot_from_loader_info() {
        let items = [item(
            "efivars",
            EventKind::Variable,
            &[("variable", "LoaderInfo"), ("value", "systemd-boot 257.1")],
        )];
        let found = detect_bootloader(&items, None).unwrap();
        assert_eq!(
            (found.id.as_str(), found.version.as_deref()),
            ("systemd-boot", Some("257.1"))
        );
    }

    #[test]
    fn falls_back_to_the_kernel_command_line() {
        assert_eq!(
            detect_bootloader(&[], Some("BOOT_IMAGE=/vmlinuz root=/dev/sda1"))
                .unwrap()
                .id,
            "grub"
        );
        assert!(detect_bootloader(&[], Some("root=/dev/sda1")).is_none());
    }
}
