//! Identity of the boot and of the system: kernel, distribution, machine.

use kokuin_model::{
    BootInfo, DmiInfo, FirmwareKind, KernelInfo, MachineInfo, OsInfo, SourceReport, SourceStatus,
    UefiInfo,
};

use crate::{Context, format_wall_clock, report};

/// What [`collect`] found.
#[derive(Debug, Clone)]
pub struct SystemInfo {
    /// Boot identity. `bootloader` and `init` are left empty: the analysis fills them
    /// from the evidence of all sources.
    pub boot: BootInfo,
    /// Report of the DMI source.
    pub dmi_report: SourceReport,
}

/// Reads the identity of the boot and of the system.
pub fn collect(ctx: &Context) -> SystemInfo {
    let text = |path: &str| {
        ctx.read_text(path)
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };

    let arch = text("/proc/sys/kernel/arch").unwrap_or_else(|| std::env::consts::ARCH.to_owned());
    let (dmi, dmi_report) = dmi(ctx);
    let boot = BootInfo {
        boot_id: text("/proc/sys/kernel/random/boot_id").unwrap_or_else(|| "unknown".into()),
        started_at: ctx.boot_epoch_unix_ns().map(format_wall_clock),
        hostname: text("/proc/sys/kernel/hostname"),
        kernel: KernelInfo {
            release: text("/proc/sys/kernel/osrelease").unwrap_or_else(|| "unknown".into()),
            version: text("/proc/sys/kernel/version"),
            cmdline: text("/proc/cmdline"),
        },
        os: os_release(ctx),
        machine: MachineInfo {
            firmware: firmware_kind(ctx, &arch),
            uefi: ctx.exists("/sys/firmware/efi").then(|| UefiInfo {
                platform_bits: text("/sys/firmware/efi/fw_platform_size")
                    .and_then(|s| s.parse().ok()),
                ..UefiInfo::default()
            }),
            dmi,
            arch,
        },
        bootloader: None,
        init: None,
    };
    SystemInfo { boot, dmi_report }
}

fn firmware_kind(ctx: &Context, arch: &str) -> FirmwareKind {
    if ctx.exists("/sys/firmware/efi") {
        FirmwareKind::Uefi
    } else if matches!(arch, "x86_64" | "i386" | "i486" | "i586" | "i686") {
        FirmwareKind::Bios
    } else {
        FirmwareKind::Unknown
    }
}

fn os_release(ctx: &Context) -> Option<OsInfo> {
    let text = ctx
        .read_text("/etc/os-release")
        .or_else(|_| ctx.read_text("/usr/lib/os-release"))
        .ok()?;
    Some(parse_os_release(&text))
}

/// Parses the `KEY=value` lines of an os-release file.
fn parse_os_release(text: &str) -> OsInfo {
    let mut info = OsInfo::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches(['"', '\'']).to_owned();
        match key.trim() {
            "ID" => info.id = Some(value),
            "ID_LIKE" => info.id_like = value.split_whitespace().map(str::to_owned).collect(),
            "NAME" => info.name = Some(value),
            "VERSION_ID" => info.version_id = Some(value),
            "PRETTY_NAME" => info.pretty_name = Some(value),
            _ => {}
        }
    }
    info
}

const DMI_DIR: &str = "/sys/class/dmi/id";

fn dmi(ctx: &Context) -> (Option<DmiInfo>, SourceReport) {
    let explain = "firmware.dmi";
    if !ctx.exists(DMI_DIR) {
        let reason = format!("{DMI_DIR} does not exist: the firmware has no SMBIOS tables");
        return (
            None,
            report::with_status("dmi", explain, SourceStatus::Unavailable, Some(reason)),
        );
    }
    let field = |name: &str| {
        ctx.read_text(format!("{DMI_DIR}/{name}"))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let info = DmiInfo {
        sys_vendor: field("sys_vendor"),
        product_name: field("product_name"),
        board_vendor: field("board_vendor"),
        board_name: field("board_name"),
        bios_vendor: field("bios_vendor"),
        bios_version: field("bios_version"),
        bios_date: field("bios_date"),
    };
    (Some(info), report::ok("dmi", explain))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_os_release() {
        let info =
            parse_os_release("NAME=\"Kira Linux\"\nID=kira\nID_LIKE=\"alpine void\"\n# comment\n");
        assert_eq!(info.name.as_deref(), Some("Kira Linux"));
        assert_eq!(info.id.as_deref(), Some("kira"));
        assert_eq!(info.id_like, ["alpine", "void"]);
    }
}
