//! EFI variables (`/sys/firmware/efi/efivars/`).

use kokuin_model::{Confidence, EventKind, SourceStatus, Stage};

use crate::{
    Context, Observation, Source, SourceOutput,
    efi::{parse_load_option, utf16},
    report,
};

const DIR: &str = "/sys/firmware/efi/efivars";
const EXPLAIN: &str = "firmware.efivars";

/// Vendor GUID of the variables defined by the UEFI specification.
const GLOBAL: &str = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// Vendor GUID of the systemd Boot Loader Interface variables.
const LOADER: &str = "4a67b082-0a4c-41cf-b6c7-440b29bb8c4f";

/// The EFI variables describing the boot.
pub struct EfiVars;

impl Source for EfiVars {
    fn id(&self) -> &'static str {
        "efivars"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn kernel_options(&self) -> &'static [&'static str] {
        &["CONFIG_EFI", "CONFIG_EFIVAR_FS"]
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        if !ctx.exists("/sys/firmware/efi") {
            return SourceOutput::empty(report::with_status(
                self.id(),
                EXPLAIN,
                SourceStatus::Unavailable,
                Some("the system was not booted through UEFI (no /sys/firmware/efi)".into()),
            ));
        }
        if let Err(err) = ctx.list_dir(DIR) {
            let mut report = report::io_error(self.id(), EXPLAIN, DIR, &err);
            report.reason = Some(format!(
                "efivarfs is not mounted on {DIR} (mount -t efivarfs efivarfs {DIR})"
            ));
            return SourceOutput::empty(report);
        }
        let vars = Vars { ctx };
        let observations = [
            boot_entry(&vars),
            boot_order(&vars),
            timeout(&vars),
            secure_boot(&vars),
        ]
        .into_iter()
        .flatten()
        .chain(loader_interface(&vars))
        .collect();
        SourceOutput {
            report: report::ok(self.id(), EXPLAIN),
            observations,
            processes: Vec::new(),
        }
    }
}

struct Vars<'a> {
    ctx: &'a Context,
}

impl Vars<'_> {
    /// Value of a variable, without its 4 bytes of attributes.
    fn get(&self, name: &str, guid: &str) -> Option<Vec<u8>> {
        let bytes = self.ctx.read_bytes(format!("{DIR}/{name}-{guid}")).ok()?;
        bytes.get(4..).map(<[u8]>::to_vec)
    }

    fn get_u16(&self, name: &str) -> Option<u16> {
        let value = self.get(name, GLOBAL)?;
        Some(u16::from_le_bytes(value.get(..2)?.try_into().ok()?))
    }

    fn get_u8(&self, name: &str) -> Option<u8> {
        self.get(name, GLOBAL)?.first().copied()
    }

    fn get_loader_string(&self, name: &str) -> Option<String> {
        Some(utf16(&self.get(name, LOADER)?)).filter(|s| !s.is_empty())
    }
}

fn variable(title: String, name: &str, stage: Stage, explain: &str) -> Observation {
    Observation::new(EventKind::Variable, title, None, Confidence::Inferred)
        .in_stage(stage)
        .explained_by(explain)
        .with("variable", name)
}

fn boot_entry(vars: &Vars) -> Option<Observation> {
    let current = vars.get_u16("BootCurrent")?;
    let name = format!("Boot{current:04X}");
    let option = vars
        .get(&name, GLOBAL)
        .and_then(|bytes| parse_load_option(&bytes));
    let mut title = format!("Firmware booted the entry {name}");
    let mut observation;
    match option {
        Some(option) => {
            title.push_str(&format!(" \u{201c}{}\u{201d}", option.description));
            if let Some(file) = &option.path.file {
                title.push_str(&format!(" ({file})"));
            }
            observation = variable(
                title,
                "BootCurrent",
                Stage::Firmware,
                "firmware.boot_entries",
            )
            .with("description", option.description);
            if let Some(file) = option.path.file {
                observation = observation.with("path", file);
            }
            if let Some(partition) = option.path.partition {
                observation = observation.with("partition", partition);
            }
            if option.path.firmware_volume {
                observation = observation.with("firmware_builtin", true);
            }
        }
        None => {
            observation = variable(
                title,
                "BootCurrent",
                Stage::Firmware,
                "firmware.boot_entries",
            )
        }
    }
    Some(observation.with("value", format!("{current:04X}")))
}

fn boot_order(vars: &Vars) -> Option<Observation> {
    let value = vars.get("BootOrder", GLOBAL)?;
    let entries: Vec<String> = value
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| format!("Boot{:04X}", u16::from_le_bytes([pair[0], pair[1]])))
        .collect();
    let title = format!("Boot order: {}", entries.join(", "));
    Some(
        variable(title, "BootOrder", Stage::Firmware, "firmware.boot_entries")
            .with("value", entries),
    )
}

fn timeout(vars: &Vars) -> Option<Observation> {
    let seconds = vars.get_u16("Timeout")?;
    let title = format!("Firmware boot menu timeout: {seconds} s");
    Some(
        variable(title, "Timeout", Stage::Firmware, "firmware.boot_entries").with("value", seconds),
    )
}

fn secure_boot(vars: &Vars) -> Option<Observation> {
    let enabled = vars.get_u8("SecureBoot")? == 1;
    let title = format!(
        "Secure Boot is {}",
        if enabled { "enabled" } else { "disabled" }
    );
    let mut observation = variable(title, "SecureBoot", Stage::Firmware, "firmware.secure_boot")
        .with("value", enabled);
    if let Some(setup_mode) = vars.get_u8("SetupMode") {
        observation = observation.with("setup_mode", setup_mode == 1);
    }
    Some(observation)
}

/// Variables of the Boot Loader Interface, written by systemd-boot and compatible
/// bootloaders, and by systemd-stub.
fn loader_interface(vars: &Vars) -> Vec<Observation> {
    let explain = "bootloader.systemd_boot";
    let mut found = Vec::new();
    if let Some(info) = vars.get_loader_string("LoaderInfo") {
        found.push(
            variable(
                format!("Bootloader: {info}"),
                "LoaderInfo",
                Stage::Bootloader,
                explain,
            )
            .with("value", info),
        );
    }
    if let Some(entry) = vars.get_loader_string("LoaderEntrySelected") {
        found.push(
            variable(
                format!("Bootloader menu entry: {entry}"),
                "LoaderEntrySelected",
                Stage::Bootloader,
                explain,
            )
            .with("value", entry),
        );
    }
    if let Some(image) = vars.get_loader_string("LoaderImageIdentifier") {
        found.push(
            variable(
                format!("Bootloader file: {image}"),
                "LoaderImageIdentifier",
                Stage::Bootloader,
                explain,
            )
            .with("path", image),
        );
    }
    for (name, what) in [
        ("LoaderTimeInitUSec", "started"),
        ("LoaderTimeExecUSec", "started the kernel"),
    ] {
        if let Some(us) = vars
            .get_loader_string(name)
            .and_then(|s| s.parse::<u64>().ok())
        {
            let title = format!(
                "Bootloader {what} {:.3} s after the CPU reset",
                us as f64 / 1e6
            );
            found.push(variable(title, name, Stage::Bootloader, explain).with("value_us", us));
        }
    }
    if let Some(stub) = vars.get_loader_string("StubInfo") {
        found.push(
            variable(
                format!("Kernel started by {stub}"),
                "StubInfo",
                Stage::Bootloader,
                "bootloader.efistub",
            )
            .with("value", stub),
        );
    }
    found
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::efi::{
        tests::{end_node, file_path_node},
        utf16z,
    };

    fn write_var(root: &std::path::Path, name: &str, guid: &str, value: &[u8]) {
        let dir = root.join("sys/firmware/efi/efivars");
        fs::create_dir_all(&dir).unwrap();
        let mut bytes = 7u32.to_le_bytes().to_vec();
        bytes.extend(value);
        fs::write(dir.join(format!("{name}-{guid}")), bytes).unwrap();
    }

    #[test]
    fn reads_boot_entry_secure_boot_and_loader_info() {
        let root = std::env::temp_dir().join(format!("kokuin-efivars-{}", std::process::id()));
        let mut path = file_path_node("\\EFI\\kira\\grubx64.efi");
        path.extend(end_node());
        let mut option = 1u32.to_le_bytes().to_vec();
        option.extend((path.len() as u16).to_le_bytes());
        option.extend(utf16z("Kira Linux"));
        option.extend(path);
        write_var(&root, "BootCurrent", GLOBAL, &1u16.to_le_bytes());
        write_var(&root, "Boot0001", GLOBAL, &option);
        write_var(&root, "SecureBoot", GLOBAL, &[0]);
        write_var(&root, "LoaderInfo", LOADER, &utf16z("systemd-boot 257"));

        let output = EfiVars.collect(&Context::with_root(&root));
        fs::remove_dir_all(&root).unwrap();

        let titles: Vec<_> = output
            .observations
            .iter()
            .map(|o| o.title.as_str())
            .collect();
        assert_eq!(
            titles,
            [
                "Firmware booted the entry Boot0001 \u{201c}Kira Linux\u{201d} (\\EFI\\kira\\grubx64.efi)",
                "Secure Boot is disabled",
                "Bootloader: systemd-boot 257",
            ]
        );
        assert_eq!(output.report.status, SourceStatus::Ok);
    }

    #[test]
    fn explains_a_bios_boot() {
        let output = EfiVars.collect(&Context::with_root("/nonexistent"));
        assert_eq!(output.report.status, SourceStatus::Unavailable);
        assert!(
            output
                .report
                .reason
                .unwrap()
                .contains("not booted through UEFI")
        );
    }
}
