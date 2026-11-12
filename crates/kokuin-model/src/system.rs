use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which boot this record describes, and on what system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BootInfo {
    /// Random identifier the kernel gives to each boot
    /// (`/proc/sys/kernel/random/boot_id`).
    pub boot_id: String,
    /// Wall-clock time of the kernel start (boot clock `0`), in RFC 3339 format.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    /// Host name of the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    /// The kernel that booted.
    pub kernel: KernelInfo,
    /// The distribution, from `/etc/os-release`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsInfo>,
    /// The machine and its firmware.
    pub machine: MachineInfo,
    /// The bootloader that started the kernel, when it could be identified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bootloader: Option<ComponentInfo>,
    /// The init system (PID 1), when it could be identified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub init: Option<ComponentInfo>,
}

/// The kernel that booted.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KernelInfo {
    /// Kernel release (`uname -r`).
    pub release: String,
    /// Kernel build version string (`uname -v`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Kernel command line (`/proc/cmdline`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmdline: Option<String>,
}

/// The distribution, from `/etc/os-release`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OsInfo {
    /// `ID` (for example `kira`, `debian`, `alpine`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// `ID_LIKE`: distributions this one derives from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub id_like: Vec<String>,
    /// `NAME`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `VERSION_ID`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_id: Option<String>,
    /// `PRETTY_NAME`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pretty_name: Option<String>,
}

/// The machine and its firmware.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MachineInfo {
    /// CPU architecture (`uname -m`, for example `x86_64`).
    pub arch: String,
    /// Kind of firmware that booted the machine.
    pub firmware: FirmwareKind,
    /// UEFI details, when the firmware is UEFI.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uefi: Option<UefiInfo>,
    /// Hardware identification from the SMBIOS/DMI tables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dmi: Option<DmiInfo>,
}

/// Kind of firmware that booted the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FirmwareKind {
    /// UEFI firmware.
    Uefi,
    /// Legacy PC BIOS (or UEFI in legacy/CSM mode).
    Bios,
    /// Something else (device tree based boards...), or not detected.
    #[serde(other)]
    Unknown,
}

/// UEFI firmware details.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UefiInfo {
    /// Firmware vendor (`/sys/firmware/efi/fw_vendor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vendor: Option<String>,
    /// UEFI specification version the firmware implements (for example `2.70`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_version: Option<String>,
    /// Bitness of the firmware (32 or 64), which can differ from the kernel's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform_bits: Option<u8>,
    /// Whether Secure Boot was enabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secure_boot: Option<bool>,
}

/// Hardware identification from the SMBIOS/DMI tables (`/sys/class/dmi/id`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DmiInfo {
    /// Machine manufacturer (`sys_vendor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sys_vendor: Option<String>,
    /// Machine model (`product_name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    /// Motherboard manufacturer (`board_vendor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_vendor: Option<String>,
    /// Motherboard model (`board_name`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub board_name: Option<String>,
    /// Firmware vendor (`bios_vendor`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bios_vendor: Option<String>,
    /// Firmware version (`bios_version`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bios_version: Option<String>,
    /// Firmware release date (`bios_date`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bios_date: Option<String>,
}

/// A software component of the boot chain (bootloader, init system).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComponentInfo {
    /// Stable lowercase identifier (for example `grub`, `systemd-boot`, `runit`,
    /// `systemd`, `openrc`). The known identifiers are listed in `docs/format.md`.
    pub id: String,
    /// Human-readable name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Version, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Identifier of the explanation of this component in the knowledge base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain_id: Option<String>,
}
