//! TPM measured-boot event log
//! (`/sys/kernel/security/tpm0/binary_bios_measurements`).
//!
//! Format: TCG PC Client Platform Firmware Profile. The first event uses the legacy
//! SHA-1 layout; when it is a "Spec ID Event03", the rest of the log uses the
//! "crypto agile" layout, with one digest per hash algorithm.

use std::collections::BTreeMap;

use kokuin_model::{Confidence, EventKind, SourceStatus, Stage};

use crate::{
    Context, Observation, Source, SourceOutput,
    binary::{Reader, hex},
    report,
    uefi::{guid, parse_device_path, parse_load_option, utf16},
};

const PATH: &str = "/sys/kernel/security/tpm0/binary_bios_measurements";
const EXPLAIN: &str = "firmware.measured_boot";

/// The TPM event log.
pub struct TpmEventLog;

impl Source for TpmEventLog {
    fn id(&self) -> &'static str {
        "tpm_eventlog"
    }

    fn explain_id(&self) -> &'static str {
        EXPLAIN
    }

    fn kernel_options(&self) -> &'static [&'static str] {
        &["CONFIG_TCG_TPM", "CONFIG_SECURITYFS"]
    }

    fn collect(&self, ctx: &Context) -> SourceOutput {
        let bytes = match ctx.read_bytes(PATH) {
            Ok(bytes) => bytes,
            Err(err) => {
                let mut report = report::io_error(self.id(), EXPLAIN, PATH, &err);
                if err.kind() == std::io::ErrorKind::NotFound {
                    report.reason = Some(missing_log_reason(ctx));
                }
                return SourceOutput::empty(report);
            }
        };
        let (events, error) = parse(&bytes);
        let observations = describe(&events);
        let report = match error {
            None => report::ok(self.id(), EXPLAIN),
            Some(error) => report::with_status(
                self.id(),
                EXPLAIN,
                if events.is_empty() {
                    SourceStatus::Failed
                } else {
                    SourceStatus::Partial
                },
                Some(format!("the log could only be read partly: {error}")),
            ),
        };
        SourceOutput {
            report,
            observations,
            processes: Vec::new(),
        }
    }
}

fn missing_log_reason(ctx: &Context) -> String {
    let has_tpm = ctx
        .list_dir("/sys/class/tpm")
        .is_ok_and(|entries| !entries.is_empty());
    if !has_tpm {
        "no TPM was found on this machine (or its driver is not loaded)".into()
    } else if ctx
        .list_dir("/sys/kernel/security")
        .is_ok_and(|entries| entries.is_empty())
    {
        "securityfs is not mounted (mount -t securityfs securityfs /sys/kernel/security)".into()
    } else {
        "the firmware did not give the kernel a TPM event log".into()
    }
}

/// One raw event of the log.
#[derive(Debug, Clone, PartialEq)]
pub struct TpmEvent {
    /// PCR the measurement was extended into.
    pub pcr: u32,
    /// Event type (`EV_*`).
    pub event_type: u32,
    /// Digests, by TCG algorithm id.
    pub digests: Vec<(u16, Vec<u8>)>,
    /// Event data.
    pub data: Vec<u8>,
}

const EV_POST_CODE: u32 = 0x1;
const EV_NO_ACTION: u32 = 0x3;
const EV_SEPARATOR: u32 = 0x4;
const EV_ACTION: u32 = 0x5;
const EV_EVENT_TAG: u32 = 0x6;
const EV_S_CRTM_CONTENTS: u32 = 0x7;
const EV_S_CRTM_VERSION: u32 = 0x8;
const EV_CPU_MICROCODE: u32 = 0x9;
const EV_IPL: u32 = 0xd;
const EV_EFI_VARIABLE_DRIVER_CONFIG: u32 = 0x8000_0001;
const EV_EFI_VARIABLE_BOOT: u32 = 0x8000_0002;
const EV_EFI_BOOT_SERVICES_APPLICATION: u32 = 0x8000_0003;
const EV_EFI_BOOT_SERVICES_DRIVER: u32 = 0x8000_0004;
const EV_EFI_RUNTIME_SERVICES_DRIVER: u32 = 0x8000_0005;
const EV_EFI_GPT_EVENT: u32 = 0x8000_0006;
const EV_EFI_ACTION: u32 = 0x8000_0007;
const EV_EFI_PLATFORM_FIRMWARE_BLOB: u32 = 0x8000_0008;
const EV_EFI_HANDOFF_TABLES: u32 = 0x8000_0009;
const EV_EFI_PLATFORM_FIRMWARE_BLOB2: u32 = 0x8000_000a;
const EV_EFI_HANDOFF_TABLES2: u32 = 0x8000_000b;
const EV_EFI_VARIABLE_BOOT2: u32 = 0x8000_000c;
const EV_EFI_VARIABLE_AUTHORITY: u32 = 0x8000_00e0;

/// Name of an event type, as written in the TCG specification.
pub fn event_type_name(event_type: u32) -> Option<&'static str> {
    Some(match event_type {
        0x0 => "EV_PREBOOT_CERT",
        EV_POST_CODE => "EV_POST_CODE",
        0x2 => "EV_UNUSED",
        EV_NO_ACTION => "EV_NO_ACTION",
        EV_SEPARATOR => "EV_SEPARATOR",
        EV_ACTION => "EV_ACTION",
        EV_EVENT_TAG => "EV_EVENT_TAG",
        EV_S_CRTM_CONTENTS => "EV_S_CRTM_CONTENTS",
        EV_S_CRTM_VERSION => "EV_S_CRTM_VERSION",
        EV_CPU_MICROCODE => "EV_CPU_MICROCODE",
        0xa => "EV_PLATFORM_CONFIG_FLAGS",
        0xb => "EV_TABLE_OF_DEVICES",
        0xc => "EV_COMPACT_HASH",
        EV_IPL => "EV_IPL",
        0xe => "EV_IPL_PARTITION_DATA",
        0xf => "EV_NONHOST_CODE",
        0x10 => "EV_NONHOST_CONFIG",
        0x11 => "EV_NONHOST_INFO",
        0x12 => "EV_OMIT_BOOT_DEVICE_EVENTS",
        EV_EFI_VARIABLE_DRIVER_CONFIG => "EV_EFI_VARIABLE_DRIVER_CONFIG",
        EV_EFI_VARIABLE_BOOT => "EV_EFI_VARIABLE_BOOT",
        EV_EFI_BOOT_SERVICES_APPLICATION => "EV_EFI_BOOT_SERVICES_APPLICATION",
        EV_EFI_BOOT_SERVICES_DRIVER => "EV_EFI_BOOT_SERVICES_DRIVER",
        EV_EFI_RUNTIME_SERVICES_DRIVER => "EV_EFI_RUNTIME_SERVICES_DRIVER",
        EV_EFI_GPT_EVENT => "EV_EFI_GPT_EVENT",
        EV_EFI_ACTION => "EV_EFI_ACTION",
        EV_EFI_PLATFORM_FIRMWARE_BLOB => "EV_EFI_PLATFORM_FIRMWARE_BLOB",
        EV_EFI_HANDOFF_TABLES => "EV_EFI_HANDOFF_TABLES",
        EV_EFI_PLATFORM_FIRMWARE_BLOB2 => "EV_EFI_PLATFORM_FIRMWARE_BLOB2",
        EV_EFI_HANDOFF_TABLES2 => "EV_EFI_HANDOFF_TABLES2",
        EV_EFI_VARIABLE_BOOT2 => "EV_EFI_VARIABLE_BOOT2",
        0x8000_0010 => "EV_EFI_HCRTM_EVENT",
        EV_EFI_VARIABLE_AUTHORITY => "EV_EFI_VARIABLE_AUTHORITY",
        0x8000_00e1 => "EV_EFI_SPDM_FIRMWARE_BLOB",
        0x8000_00e2 => "EV_EFI_SPDM_FIRMWARE_CONFIG",
        _ => return None,
    })
}

const SPEC_ID_SIGNATURE: &[u8] = b"Spec ID Event03\0";
const SHA1: u16 = 0x0004;
const SHA256: u16 = 0x000b;

/// Parses a log. Returns the events read and, if the log is damaged, why reading
/// stopped.
pub fn parse(bytes: &[u8]) -> (Vec<TpmEvent>, Option<String>) {
    let mut reader = Reader::new(bytes);
    let mut events = Vec::new();
    let Some(first) = read_legacy_event(&mut reader) else {
        return (events, Some("the first event is truncated".into()));
    };
    let digest_sizes = (first.event_type == EV_NO_ACTION
        && first.data.starts_with(SPEC_ID_SIGNATURE))
    .then(|| parse_spec_id(&first.data));
    if digest_sizes.is_none() {
        events.push(first);
    }
    while !reader.is_empty() && !is_padding(reader.rest()) {
        let offset = reader.position();
        let event = match &digest_sizes {
            Some(Some(sizes)) => read_crypto_agile_event(&mut reader, sizes),
            Some(None) => return (events, Some("the Spec ID header is invalid".into())),
            None => read_legacy_event(&mut reader),
        };
        match event {
            Some(event) => events.push(event),
            None => {
                return (
                    events,
                    Some(format!("event at byte {offset} is truncated or invalid")),
                );
            }
        }
    }
    (events, None)
}

/// Unused space at the end of a log area is filled with 0x00 or 0xFF.
fn is_padding(rest: &[u8]) -> bool {
    rest.iter().all(|&b| b == 0) || rest.iter().all(|&b| b == 0xff)
}

fn read_legacy_event(reader: &mut Reader) -> Option<TpmEvent> {
    let pcr = reader.u32()?;
    let event_type = reader.u32()?;
    let digest = reader.take(20)?.to_vec();
    let size = reader.u32()? as usize;
    let data = reader.take(size)?.to_vec();
    Some(TpmEvent {
        pcr,
        event_type,
        digests: vec![(SHA1, digest)],
        data,
    })
}

/// Digest size of each algorithm, from the Spec ID event.
fn parse_spec_id(data: &[u8]) -> Option<BTreeMap<u16, usize>> {
    let mut reader = Reader::new(data);
    reader.take(SPEC_ID_SIGNATURE.len())?;
    let _platform_class = reader.u32()?;
    let _version = reader.take(3)?;
    let _uintn_size = reader.u8()?;
    let count = reader.u32()?;
    let mut sizes = BTreeMap::new();
    for _ in 0..count {
        let algorithm = reader.u16()?;
        let size = reader.u16()?;
        sizes.insert(algorithm, usize::from(size));
    }
    Some(sizes)
}

fn read_crypto_agile_event(reader: &mut Reader, sizes: &BTreeMap<u16, usize>) -> Option<TpmEvent> {
    let pcr = reader.u32()?;
    let event_type = reader.u32()?;
    let count = reader.u32()?;
    let mut digests = Vec::new();
    for _ in 0..count {
        let algorithm = reader.u16()?;
        let digest = reader.take(*sizes.get(&algorithm)?)?.to_vec();
        digests.push((algorithm, digest));
    }
    let size = reader.u32()? as usize;
    let data = reader.take(size)?.to_vec();
    Some(TpmEvent {
        pcr,
        event_type,
        digests,
        data,
    })
}

/// Turns raw events into observations, in log order.
///
/// Events before the firmware starts the boot program belong to the firmware stage,
/// the following ones to the bootloader stage. Runs of separators are merged.
pub fn describe(events: &[TpmEvent]) -> Vec<Observation> {
    let mut stage = Stage::Firmware;
    let mut observations: Vec<Observation> = Vec::new();
    let mut separators: Vec<u32> = Vec::new();

    for event in events {
        if event.event_type == EV_NO_ACTION {
            continue;
        }
        if starts_boot_program(event) {
            stage = Stage::Bootloader;
        }
        if event.event_type == EV_SEPARATOR {
            separators.push(event.pcr);
            continue;
        }
        if !separators.is_empty() {
            observations.push(separator_observation(&separators, stage));
            separators.clear();
        }
        observations.push(describe_event(event, stage));
    }
    if !separators.is_empty() {
        observations.push(separator_observation(&separators, stage));
    }
    observations
}

fn starts_boot_program(event: &TpmEvent) -> bool {
    match event.event_type {
        EV_EFI_ACTION => text(&event.data).starts_with("Calling EFI Application"),
        EV_EFI_BOOT_SERVICES_APPLICATION => event.pcr == 4,
        _ => false,
    }
}

fn separator_observation(pcrs: &[u32], stage: Stage) -> Observation {
    let list = pcrs
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Observation::new(
        EventKind::Measurement,
        format!("Firmware closed its measurements (separator in PCR {list})"),
        None,
        Confidence::Inferred,
    )
    .in_stage(stage)
    .explained_by(EXPLAIN)
    .with("event_type", "EV_SEPARATOR")
    .with("pcrs", pcrs.to_vec())
}

fn describe_event(event: &TpmEvent, stage: Stage) -> Observation {
    let mut explain = EXPLAIN;
    let mut message = None;
    let mut extra: Vec<(&str, serde_json::Value)> = Vec::new();
    let pcr = event.pcr;
    let data = &event.data;

    let title = match event.event_type {
        EV_EFI_ACTION | EV_ACTION => {
            let action = text(data);
            let title = match action.as_str() {
                "Calling EFI Application from Boot Option" => {
                    "Firmware started the boot program".into()
                }
                "Returning from EFI Application from Boot Option" => {
                    "A boot program returned to the firmware (it failed or exited)".into()
                }
                "Exit Boot Services Invocation" => "Bootloader called ExitBootServices".into(),
                "Exit Boot Services Returned with Success" => {
                    explain = "firmware.exit_boot_services";
                    "Firmware handed the machine over to the OS".into()
                }
                _ => format!("Firmware action: {action}"),
            };
            message = Some(action);
            title
        }
        EV_POST_CODE => match printable(data) {
            Some(what) => format!("Firmware measured its {what}"),
            None => "Firmware measured part of its code".into(),
        },
        EV_S_CRTM_VERSION => {
            let version = utf16(data);
            extra.push(("version", version.clone().into()));
            format!("Firmware version {version} measured")
        }
        EV_S_CRTM_CONTENTS => "Firmware measured its first code (S-CRTM)".into(),
        EV_CPU_MICROCODE => "CPU microcode update measured".into(),
        EV_EFI_PLATFORM_FIRMWARE_BLOB => "Firmware measured one of its volumes".into(),
        EV_EFI_PLATFORM_FIRMWARE_BLOB2 => match blob2_description(data) {
            Some(description) => format!("Firmware measured its component {description}"),
            None => "Firmware measured one of its volumes".into(),
        },
        EV_EFI_HANDOFF_TABLES | EV_EFI_HANDOFF_TABLES2 => {
            "Firmware measured the tables it hands to the OS (SMBIOS, ACPI)".into()
        }
        EV_EFI_GPT_EVENT => {
            let partitions = data
                .get(92..100)
                .map(|b| u64::from_le_bytes(b.try_into().unwrap()));
            match partitions {
                Some(n) => format!(
                    "Firmware measured the partition table of the boot disk ({n} partitions)"
                ),
                None => "Firmware measured the partition table of the boot disk".into(),
            }
        }
        EV_EFI_VARIABLE_DRIVER_CONFIG
        | EV_EFI_VARIABLE_BOOT
        | EV_EFI_VARIABLE_BOOT2
        | EV_EFI_VARIABLE_AUTHORITY => {
            let (title, variable, variable_explain) = describe_variable(event);
            explain = variable_explain;
            if let Some(variable) = variable {
                extra.push(("variable", variable.into()));
            }
            title
        }
        EV_EFI_BOOT_SERVICES_APPLICATION
        | EV_EFI_BOOT_SERVICES_DRIVER
        | EV_EFI_RUNTIME_SERVICES_DRIVER => {
            // UEFI_IMAGE_LOAD_EVENT: location u64, length u64, link address u64,
            // device path length u64, device path.
            let path = data.get(32..).map(parse_device_path).unwrap_or_default();
            let what = if event.event_type == EV_EFI_BOOT_SERVICES_APPLICATION {
                "boot program"
            } else {
                "driver from an add-on card or the firmware"
            };
            match (path.file, path.firmware_volume) {
                (Some(file), _) => {
                    let title = format!("Firmware measured the {what} {file}");
                    extra.push(("path", file.into()));
                    title
                }
                (None, true) => format!("Firmware measured a {what} built into the firmware"),
                (None, false) => format!("Firmware measured a {what}"),
            }
        }
        EV_IPL => {
            let content = text(data);
            let title = describe_ipl(pcr, &content, &mut explain, &mut extra);
            message = Some(content);
            title
        }
        EV_EVENT_TAG => {
            // Linux EFI stub: tag id u32, size u32, description.
            let description = data.get(8..).map(text).unwrap_or_default();
            explain = "bootloader.efistub";
            message = Some(description.clone());
            format!("Kernel EFI stub measured {description}")
        }
        other => {
            let name =
                event_type_name(other).map_or_else(|| format!("event 0x{other:x}"), str::to_owned);
            format!("Measurement {name} in PCR {pcr}")
        }
    };

    let mut observation =
        Observation::new(EventKind::Measurement, title, None, Confidence::Inferred)
            .in_stage(stage)
            .explained_by(explain)
            .with("pcr", pcr)
            .with(
                "event_type",
                event_type_name(event.event_type)
                    .map_or_else(|| format!("0x{:x}", event.event_type), str::to_owned),
            );
    if let Some((algorithm, digest)) = preferred_digest(event) {
        observation = observation
            .with("digest_algorithm", algorithm)
            .with("digest", hex(digest));
    }
    for (key, value) in extra {
        observation = observation.with(key, value);
    }
    observation.message = message;
    observation
}

/// Titles for EV_IPL events, written by bootloaders (GRUB: PCR 8 and 9) and by
/// systemd-stub (PCR 11, 12).
fn describe_ipl(
    pcr: u32,
    content: &str,
    explain: &mut &'static str,
    extra: &mut Vec<(&str, serde_json::Value)>,
) -> String {
    if let Some(command) = content.strip_prefix("grub_cmd: ") {
        *explain = "bootloader.grub_commands";
        extra.push(("grub_command", command.into()));
        format!("GRUB ran: {}", shorten(command, 100))
    } else if let Some(cmdline) = content.strip_prefix("kernel_cmdline: ") {
        *explain = "bootloader.grub_commands";
        format!("GRUB set the kernel command line: {}", shorten(cmdline, 80))
    } else if pcr == 9 {
        *explain = "bootloader.grub_commands";
        extra.push(("path", content.into()));
        format!("GRUB read the file {content}")
    } else {
        *explain = "stage.bootloader";
        format!("Bootloader measured: {}", shorten(content, 100))
    }
}

/// Title, variable name and explanation of an EFI variable measurement.
fn describe_variable(event: &TpmEvent) -> (String, Option<String>, &'static str) {
    // UEFI_VARIABLE_DATA: vendor GUID [16], name length u64 (chars), data length u64,
    // name (UTF-16), data.
    let mut reader = Reader::new(&event.data);
    let parsed = (|| {
        let vendor = guid(reader.take(16)?)?;
        let name_len = usize::try_from(reader.u64()?).ok()?;
        let data_len = usize::try_from(reader.u64()?).ok()?;
        let name = utf16(reader.take(name_len.checked_mul(2)?)?);
        let value = reader.take(data_len).unwrap_or(reader.rest());
        Some((vendor, name, value))
    })();
    let Some((_vendor, name, value)) = parsed else {
        return (
            "Firmware measured an EFI variable".into(),
            None,
            "firmware.efivars",
        );
    };

    let title = match event.event_type {
        EV_EFI_VARIABLE_AUTHORITY => {
            return (
                format!("Secure Boot: a program was verified with a key from {name}"),
                Some(name),
                "firmware.secure_boot",
            );
        }
        EV_EFI_VARIABLE_DRIVER_CONFIG if name == "SecureBoot" => {
            let state = if value.first() == Some(&1) {
                "enabled"
            } else {
                "disabled"
            };
            return (
                format!("Firmware measured the Secure Boot state: {state}"),
                Some(name),
                "firmware.secure_boot",
            );
        }
        EV_EFI_VARIABLE_DRIVER_CONFIG => {
            return (
                format!("Firmware measured the Secure Boot key list {name}"),
                Some(name),
                "firmware.secure_boot",
            );
        }
        _ if name == "BootOrder" => {
            let entries: Vec<String> = value
                .chunks_exact(2)
                .map(|pair| format!("Boot{:04X}", u16::from_le_bytes([pair[0], pair[1]])))
                .collect();
            format!("Firmware measured the boot order: {}", entries.join(", "))
        }
        _ => match parse_load_option(value) {
            Some(option) if name.starts_with("Boot") => {
                format!(
                    "Firmware measured the boot entry {name} \u{201c}{}\u{201d}",
                    option.description
                )
            }
            _ => format!("Firmware measured the variable {name}"),
        },
    };
    (title, Some(name), "firmware.boot_entries")
}

fn blob2_description(data: &[u8]) -> Option<String> {
    let len = usize::from(*data.first()?);
    printable(data.get(1..1 + len)?)
}

/// The SHA-256 digest if there is one, else the first one.
fn preferred_digest(event: &TpmEvent) -> Option<(&'static str, &[u8])> {
    let name = |algorithm: u16| match algorithm {
        SHA1 => "sha1",
        SHA256 => "sha256",
        0x000c => "sha384",
        0x000d => "sha512",
        0x0012 => "sm3_256",
        _ => "unknown",
    };
    let (algorithm, digest) = event
        .digests
        .iter()
        .find(|(a, _)| *a == SHA256)
        .or_else(|| event.digests.first())?;
    Some((name(*algorithm), digest.as_slice()))
}

/// Event data as text: ASCII/UTF-8, or UTF-16 when it looks like it.
fn text(data: &[u8]) -> String {
    let looks_utf16 = data.len() >= 4 && data.get(1) == Some(&0) && data.get(3) == Some(&0);
    if looks_utf16 {
        return utf16(data);
    }
    String::from_utf8_lossy(data)
        .trim_end_matches('\0')
        .trim()
        .to_owned()
}

/// The text of `data` if it is all printable ASCII.
fn printable(data: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(data).ok()?.trim_end_matches('\0');
    (!text.is_empty() && text.chars().all(|c| c.is_ascii_graphic() || c == ' '))
        .then(|| text.to_owned())
}

fn shorten(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy(pcr: u32, event_type: u32, data: &[u8]) -> Vec<u8> {
        let mut bytes = pcr.to_le_bytes().to_vec();
        bytes.extend(event_type.to_le_bytes());
        bytes.extend([0u8; 20]);
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(data);
        bytes
    }

    fn agile(pcr: u32, event_type: u32, data: &[u8]) -> Vec<u8> {
        let mut bytes = pcr.to_le_bytes().to_vec();
        bytes.extend(event_type.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.extend(SHA256.to_le_bytes());
        bytes.extend([0xab; 32]);
        bytes.extend((data.len() as u32).to_le_bytes());
        bytes.extend(data);
        bytes
    }

    fn spec_id_header() -> Vec<u8> {
        let mut data = SPEC_ID_SIGNATURE.to_vec();
        data.extend(0u32.to_le_bytes()); // platform class
        data.extend([0, 2, 0, 2]); // version minor, major, errata, uintn size
        data.extend(1u32.to_le_bytes());
        data.extend(SHA256.to_le_bytes());
        data.extend(32u16.to_le_bytes());
        data.push(0); // vendor info size
        legacy(0, EV_NO_ACTION, &data)
    }

    fn sample_log() -> Vec<u8> {
        let mut log = spec_id_header();
        log.extend(agile(
            0,
            EV_S_CRTM_VERSION,
            &crate::uefi::tests::utf16z("1.0"),
        ));
        log.extend(agile(
            4,
            EV_EFI_ACTION,
            b"Calling EFI Application from Boot Option",
        ));
        for pcr in 0..=7 {
            log.extend(agile(pcr, EV_SEPARATOR, &[0, 0, 0, 0]));
        }
        let mut image = vec![0u8; 24];
        let mut path = crate::uefi::tests::file_path_node("\\EFI\\kira\\grubx64.efi");
        path.extend(crate::uefi::tests::end_node());
        image.extend((path.len() as u64).to_le_bytes());
        image.extend(path);
        log.extend(agile(4, EV_EFI_BOOT_SERVICES_APPLICATION, &image));
        log.extend(agile(
            8,
            EV_IPL,
            b"grub_cmd: linux /vmlinuz root=/dev/vda2\0",
        ));
        log.extend(agile(9, EV_IPL, b"/vmlinuz\0"));
        log.extend([0u8; 64]); // padding
        log
    }

    #[test]
    fn parses_a_crypto_agile_log() {
        let (events, error) = parse(&sample_log());
        assert_eq!(error, None);
        assert_eq!(events.len(), 13);
        assert_eq!(events[0].digests[0].1, [0xab; 32]);
    }

    #[test]
    fn describes_events_with_stages() {
        let (events, _) = parse(&sample_log());
        let found = describe(&events);
        let summary: Vec<_> = found
            .iter()
            .map(|o| (o.stage.unwrap(), o.title.as_str()))
            .collect();
        assert_eq!(
            summary,
            [
                (Stage::Firmware, "Firmware version 1.0 measured"),
                (Stage::Bootloader, "Firmware started the boot program"),
                (
                    Stage::Bootloader,
                    "Firmware closed its measurements (separator in PCR 0, 1, 2, 3, 4, 5, 6, 7)"
                ),
                (
                    Stage::Bootloader,
                    "Firmware measured the boot program \\EFI\\kira\\grubx64.efi"
                ),
                (Stage::Bootloader, "GRUB ran: linux /vmlinuz root=/dev/vda2"),
                (Stage::Bootloader, "GRUB read the file /vmlinuz"),
            ]
        );
        assert_eq!(
            found[4].data["grub_command"],
            "linux /vmlinuz root=/dev/vda2"
        );
        assert_eq!(found[3].data["digest_algorithm"], "sha256");
    }

    #[test]
    fn parses_a_legacy_sha1_log() {
        let mut log = legacy(0, EV_POST_CODE, b"POST CODE");
        log.extend(legacy(4, EV_SEPARATOR, &[0; 4]));
        let (events, error) = parse(&log);
        assert_eq!((events.len(), error), (2, None));
        assert_eq!(
            describe(&events)[0].title,
            "Firmware measured its POST CODE"
        );
    }

    #[test]
    fn reports_a_truncated_log() {
        let mut log = sample_log();
        log.truncate(log.len() - 70);
        let (events, error) = parse(&log);
        assert_eq!(events.len(), 12);
        assert!(error.unwrap().contains("truncated"));
    }
}
