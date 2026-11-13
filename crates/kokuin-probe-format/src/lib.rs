//! What kokuin's EFI probe leaves for Linux.
//!
//! The probe runs before the bootloader, notes the CPU's time counter (TSC) and what
//! the firmware set up, then writes a [`Report`] in the EFI variable
//! [`VARIABLE`]`-`[`GUID`]. Linux reads it back from
//! `/sys/firmware/efi/efivars/`.
//!
//! The variable holds text: one `key=value` per line, the format version first.
//! Readers skip keys they do not know, so a newer probe stays readable. It stays
//! small (well under a kilobyte), as firmware limits the size of variables.

#![no_std]

extern crate alloc;

use alloc::{
    format,
    string::{String, ToString},
};

/// Name of the variable.
pub const VARIABLE: &str = "KokuinProbe";
/// kokuin's vendor GUID, which owns the variable.
pub const GUID: &str = "abec3a02-ce3a-4d49-804e-61195467c9c3";
/// Version of the format.
pub const FORMAT: u32 = 1;

/// What the probe saw.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Version of the format.
    pub format: u32,
    /// The time counter when the probe started.
    pub tsc_start: u64,
    /// The time counter when the probe handed over to the next program.
    pub tsc_handover: Option<u64>,
    /// The counter's frequency, measured by the probe against the firmware's
    /// `Stall` (Linux measures it more precisely).
    pub tsc_hz: Option<u64>,
    /// Who made the firmware.
    pub firmware_vendor: Option<String>,
    /// The firmware's own revision number.
    pub firmware_revision: Option<u32>,
    /// The UEFI specification the firmware follows (`2.70`).
    pub uefi_revision: Option<String>,
    /// Whether Secure Boot is on.
    pub secure_boot: Option<bool>,
    /// Memory the firmware describes, in bytes.
    pub memory_total: Option<u64>,
    /// Of which free for the operating system, in bytes.
    pub memory_usable: Option<u64>,
    /// The console's text mode (`80x25`).
    pub console: Option<String>,
    /// The boot entry the probe was started from (`Boot0005`).
    pub boot_current: Option<u16>,
    /// The boot entry the probe handed over to.
    pub handover_entry: Option<u16>,
    /// Its name in the boot menu.
    pub handover_description: Option<String>,
    /// Why the handover failed, if it did.
    pub handover_error: Option<String>,
    /// How many programs and drivers the firmware had loaded, the probe included.
    pub loaded_images: Option<u64>,
    /// The probe's own file (`\EFI\kokuin\probe.efi`).
    pub probe_path: Option<String>,
}

/// Why a variable could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The first line is not `format=...`.
    NotAReport,
    /// A format this version does not read.
    Format(u32),
}

impl Report {
    /// The text of the variable.
    pub fn encode(&self) -> String {
        let mut text = format!("format={}\n", self.format);
        let mut line = |key: &str, value: Option<String>| {
            if let Some(value) = value {
                text.push_str(key);
                text.push('=');
                text.push_str(&one_line(&value));
                text.push('\n');
            }
        };
        line("tsc_start", Some(self.tsc_start.to_string()));
        line("tsc_handover", self.tsc_handover.map(|v| v.to_string()));
        line("tsc_hz", self.tsc_hz.map(|v| v.to_string()));
        line("firmware_vendor", self.firmware_vendor.clone());
        line(
            "firmware_revision",
            self.firmware_revision.map(|v| v.to_string()),
        );
        line("uefi_revision", self.uefi_revision.clone());
        line(
            "secure_boot",
            self.secure_boot.map(|v| u8::from(v).to_string()),
        );
        line("memory_total", self.memory_total.map(|v| v.to_string()));
        line("memory_usable", self.memory_usable.map(|v| v.to_string()));
        line("console", self.console.clone());
        line(
            "boot_current",
            self.boot_current.map(|v| format!("{v:04X}")),
        );
        line(
            "handover_entry",
            self.handover_entry.map(|v| format!("{v:04X}")),
        );
        line("handover_description", self.handover_description.clone());
        line("handover_error", self.handover_error.clone());
        line("loaded_images", self.loaded_images.map(|v| v.to_string()));
        line("probe_path", self.probe_path.clone());
        text
    }

    /// Reads the text of the variable.
    pub fn decode(text: &str) -> Result<Self, DecodeError> {
        let mut lines = text.lines();
        let format = lines
            .next()
            .and_then(|line| line.strip_prefix("format="))
            .and_then(|v| v.trim().parse::<u32>().ok())
            .ok_or(DecodeError::NotAReport)?;
        if format != FORMAT {
            return Err(DecodeError::Format(format));
        }
        let mut report = Report {
            format,
            ..Report::default()
        };
        for line in lines {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let text = || Some(value.to_string());
            let number = || value.parse::<u64>().ok();
            let entry = || u16::from_str_radix(value, 16).ok();
            match key {
                "tsc_start" => report.tsc_start = number().unwrap_or_default(),
                "tsc_handover" => report.tsc_handover = number(),
                "tsc_hz" => report.tsc_hz = number(),
                "firmware_vendor" => report.firmware_vendor = text(),
                "firmware_revision" => report.firmware_revision = value.parse().ok(),
                "uefi_revision" => report.uefi_revision = text(),
                "secure_boot" => report.secure_boot = Some(value == "1"),
                "memory_total" => report.memory_total = number(),
                "memory_usable" => report.memory_usable = number(),
                "console" => report.console = text(),
                "boot_current" => report.boot_current = entry(),
                "handover_entry" => report.handover_entry = entry(),
                "handover_description" => report.handover_description = text(),
                "handover_error" => report.handover_error = text(),
                "loaded_images" => report.loaded_images = number(),
                "probe_path" => report.probe_path = text(),
                _ => {}
            }
        }
        Ok(report)
    }
}

/// A value on one line.
fn one_line(value: &str) -> String {
    value.replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report {
            format: FORMAT,
            tsc_start: 1_234_567_890,
            tsc_handover: Some(1_334_567_890),
            tsc_hz: Some(2_803_200_000),
            firmware_vendor: Some("EDK II".into()),
            firmware_revision: Some(65536),
            uefi_revision: Some("2.70".into()),
            secure_boot: Some(false),
            memory_total: Some(2 << 30),
            memory_usable: Some(1 << 30),
            console: Some("80x25".into()),
            boot_current: Some(0x0005),
            handover_entry: Some(0x0001),
            handover_description: Some("Alpine\nLinux".into()),
            handover_error: None,
            loaded_images: Some(42),
            probe_path: Some("\\EFI\\kokuin\\probe.efi".into()),
        }
    }

    #[test]
    fn a_report_survives_the_variable() {
        let text = report().encode();
        assert!(text.starts_with("format=1\ntsc_start=1234567890\n"));
        assert!(text.contains("boot_current=0005\n"));
        let back = Report::decode(&text).unwrap();
        // Values are kept on one line.
        assert_eq!(back.handover_description.as_deref(), Some("Alpine Linux"));
        assert_eq!(
            Report {
                handover_description: report().handover_description.map(|d| d.replace('\n', " ")),
                ..report()
            },
            back
        );
    }

    #[test]
    fn stays_small() {
        assert!(report().encode().len() < 1024);
    }

    #[test]
    fn skips_unknown_keys_and_refuses_other_formats() {
        let back = Report::decode("format=1\ntsc_start=5\nnew_key=x\n").unwrap();
        assert_eq!(back.tsc_start, 5);
        assert_eq!(Report::decode("format=9\n"), Err(DecodeError::Format(9)));
        assert_eq!(Report::decode("hello"), Err(DecodeError::NotAReport));
    }
}
