//! kokuin's EFI probe: the first program the firmware starts on a probed boot.
//!
//! `kokuin-cli probe arm` points `BootNext` at it, for one boot. It:
//!
//! 1. notes the CPU's time counter (TSC), which counts from power-on;
//! 2. notes what the firmware set up: who made it, memory, console, Secure Boot;
//! 3. finds the entry to start: the one named in its own load options
//!    (`Boot0002`, written by `kokuin-cli probe install`), else the first active
//!    boot entry of `BootOrder` that is not itself;
//! 4. writes all that, with the time counter again, in the `KokuinProbe` variable;
//! 5. starts that entry (GRUB, usually), which never returns on success.
//!
//! If anything goes wrong it returns, and the firmware goes on with its boot order:
//! the probe must never keep the machine from booting.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::{format, string::String, string::ToString, vec::Vec};
use core::time::Duration;

use kokuin_probe_format::{ENTRY_DESCRIPTION, FORMAT, GUID, Report, VARIABLE};
use uefi::{
    CString16, Guid, Status,
    boot::{self, LoadImageSource, MemoryType},
    mem::memory_map::MemoryMap,
    proto::{
        BootPolicy,
        device_path::{DevicePath, LoadedImageDevicePath},
        loaded_image::LoadedImage,
    },
    runtime::{self, VariableAttributes, VariableVendor},
    system,
};

/// The CPU's time counter. It counts from power-on, at a fixed rate on any CPU of
/// the last fifteen years.
#[allow(unsafe_code)]
fn tsc() -> u64 {
    // SAFETY: `rdtsc` only reads the time stamp counter, which every x86-64 CPU has;
    // it touches no memory.
    unsafe { core::arch::x86_64::_rdtsc() }
}

#[uefi::entry]
fn main() -> Status {
    let tsc_start = tsc();
    let mut report = Report {
        format: FORMAT,
        tsc_start,
        ..Report::default()
    };
    describe_firmware(&mut report);
    report.tsc_hz = Some(measure_tsc_hz());

    let boot_current = global_u16("BootCurrent");
    report.boot_current = boot_current;
    let Some((entry, option)) = requested_entry(boot_current).or_else(|| next_entry(boot_current))
    else {
        report.handover_error = Some("no other boot entry in BootOrder".into());
        write_report(&report);
        return Status::NOT_FOUND;
    };
    report.handover_entry = Some(entry);
    report.handover_description = Some(option.description);

    // An entry for a whole disk (no file in its path) boots that disk's standard
    // fallback program: take it from the probe's own partition, the ESP.
    let own = own_device_path();
    let path = if has_file(&option.device_path) {
        Some(match &own {
            Some(own) => full_path(&option.device_path, own),
            None => option.device_path,
        })
    } else {
        own.map(|own| with_file(&own, FALLBACK_PROGRAM))
    };
    let image = path
        .ok_or_else(|| "its device path is not known".to_string())
        .and_then(|bytes| load(&bytes));
    let image = match image {
        Ok(image) => image,
        Err(error) => {
            report.handover_error = Some(error);
            write_report(&report);
            return Status::LOAD_ERROR;
        }
    };

    report.tsc_handover = Some(tsc());
    write_report(&report);
    // On success, the next program never returns: it boots the system.
    if let Err(err) = boot::start_image(image) {
        report.handover_error = Some(format!("it stopped with {:?}", err.status()));
        write_report(&report);
    }
    Status::LOAD_ERROR
}

/// The program a firmware boots from a disk when told nothing more.
const FALLBACK_PROGRAM: &str = "\\EFI\\BOOT\\BOOTX64.EFI";

/// Loads the program at a device path, without starting it.
fn load(device_path: &[u8]) -> Result<uefi::Handle, String> {
    let path = <&DevicePath>::try_from(device_path)
        .map_err(|_| "its device path is not valid".to_string())?;
    boot::load_image(
        boot::image_handle(),
        LoadImageSource::FromDevicePath {
            device_path: path,
            boot_policy: BootPolicy::ExactMatch,
        },
    )
    .map_err(|err| format!("the firmware could not load it: {:?}", err.status()))
}

/// The device path of the probe's own file: its partition, then the file.
fn own_device_path() -> Option<Vec<u8>> {
    let path = boot::open_protocol_exclusive::<LoadedImageDevicePath>(boot::image_handle()).ok()?;
    Some(path.as_bytes().to_vec())
}

/// Device path nodes: type, sub-type, length (header included), data.
const MEDIA: u8 = 0x04;
const MEDIA_FILE_PATH: u8 = 0x04;
const END: u8 = 0x7f;

/// The nodes of a device path, as (type, sub-type, whole node), up to the end node.
fn nodes(path: &[u8]) -> Vec<(u8, u8, &[u8])> {
    let mut nodes = Vec::new();
    let mut offset = 0;
    while let (Some(&kind), Some(&subtype), Some(len)) = (
        path.get(offset),
        path.get(offset + 1),
        path.get(offset + 2..offset + 4),
    ) {
        let len = usize::from(u16::from_le_bytes([len[0], len[1]]));
        if kind == END || len < 4 {
            break;
        }
        let Some(node) = path.get(offset..offset + len) else {
            break;
        };
        nodes.push((kind, subtype, node));
        offset += len;
    }
    nodes
}

/// A short-form path (a partition then a file, as distributions write their
/// entries) made whole, the way the firmware's boot manager does: when the partition
/// is in the probe's own full path, the part of that path before it goes in front.
/// `LoadImage` does not do it, and refuses short-form paths.
fn full_path(target: &[u8], own: &[u8]) -> Vec<u8> {
    let target_nodes = nodes(target);
    let Some(&(_, _, partition)) = target_nodes.first() else {
        return target.to_vec();
    };
    let own_nodes = nodes(own);
    let Some(at) = own_nodes.iter().position(|&(_, _, node)| node == partition) else {
        return target.to_vec();
    };
    if at == 0 {
        // Already whole.
        return target.to_vec();
    }
    let mut bytes: Vec<u8> = own_nodes[..at]
        .iter()
        .chain(&target_nodes)
        .flat_map(|(_, _, node)| node.iter().copied())
        .collect();
    bytes.extend([END, 0xff, 4, 0]);
    bytes
}

fn has_file(path: &[u8]) -> bool {
    nodes(path)
        .iter()
        .any(|&(kind, subtype, _)| (kind, subtype) == (MEDIA, MEDIA_FILE_PATH))
}

/// The same device path with its file replaced by `file`.
fn with_file(path: &[u8], file: &str) -> Vec<u8> {
    let mut bytes: Vec<u8> = nodes(path)
        .into_iter()
        .filter(|&(kind, subtype, _)| (kind, subtype) != (MEDIA, MEDIA_FILE_PATH))
        .flat_map(|(_, _, node)| node.iter().copied())
        .collect();
    let name: Vec<u8> = file
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    bytes.extend([MEDIA, MEDIA_FILE_PATH]);
    bytes.extend(((name.len() + 4) as u16).to_le_bytes());
    bytes.extend(name);
    bytes.extend([END, 0xff, 4, 0]);
    bytes
}

/// What the firmware set up before starting the probe.
fn describe_firmware(report: &mut Report) {
    report.firmware_vendor = Some(system::firmware_vendor().to_string());
    report.firmware_revision = Some(system::firmware_revision());
    report.uefi_revision = Some(format!("{}", system::uefi_revision()));
    report.secure_boot = global_variable("SecureBoot").map(|value| value.first() == Some(&1));

    if let Ok(map) = boot::memory_map(MemoryType::LOADER_DATA) {
        let (mut total, mut usable) = (0u64, 0u64);
        for descriptor in map.entries() {
            let bytes = descriptor.page_count * 4096;
            total += bytes;
            // What the operating system gets once the firmware is done.
            if matches!(
                descriptor.ty,
                MemoryType::CONVENTIONAL
                    | MemoryType::BOOT_SERVICES_CODE
                    | MemoryType::BOOT_SERVICES_DATA
                    | MemoryType::LOADER_CODE
                    | MemoryType::LOADER_DATA
            ) {
                usable += bytes;
            }
        }
        report.memory_total = Some(total);
        report.memory_usable = Some(usable);
    }

    report.console = system::with_stdout(|out| {
        out.current_mode()
            .ok()
            .flatten()
            .map(|mode| format!("{}x{}", mode.columns(), mode.rows()))
    });
    report.loaded_images = boot::find_handles::<LoadedImage>()
        .ok()
        .map(|handles| handles.len() as u64);
    report.probe_path = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle())
        .ok()
        .and_then(|image| image.file_path().map(|path| path.to_string()));
}

/// The entry named in the probe's load options (`Boot0002`), if it can be read and
/// is not the probe itself.
fn requested_entry(boot_current: Option<u16>) -> Option<(u16, LoadOption)> {
    let image = boot::open_protocol_exclusive::<LoadedImage>(boot::image_handle()).ok()?;
    let options = image.load_options_as_cstr16().ok()?.to_string();
    let number = options.trim().strip_prefix("Boot")?.get(..4)?;
    let entry = u16::from_str_radix(number, 16)
        .ok()
        .filter(|&entry| Some(entry) != boot_current)?;
    let option = parse_load_option(&global_variable(&format!("Boot{entry:04X}"))?)?;
    option.bootable.then_some((entry, option))
}

/// The counter's frequency, from how much it moves while the firmware waits 10 ms.
fn measure_tsc_hz() -> u64 {
    let before = tsc();
    boot::stall(Duration::from_millis(10));
    tsc().saturating_sub(before) * 100
}

/// `LOAD_OPTION_ACTIVE`.
const ACTIVE: u32 = 0x1;
/// The category bits; 0 is "boot", other values are apps the firmware shows only.
const CATEGORY: u32 = 0x1f00;

/// A boot entry, as much of it as the probe needs.
struct LoadOption {
    description: String,
    device_path: Vec<u8>,
    /// Active, in the boot category, and not a probe.
    bootable: bool,
}

/// The first entry of `BootOrder` that is active, meant to boot (not an app such as
/// the firmware's boot menu), and not the probe itself.
fn next_entry(boot_current: Option<u16>) -> Option<(u16, LoadOption)> {
    let order = global_variable("BootOrder")?;
    order
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .filter(|&entry| Some(entry) != boot_current)
        .find_map(|entry| {
            let bytes = global_variable(&format!("Boot{entry:04X}"))?;
            let option = parse_load_option(&bytes)?;
            option.bootable.then_some((entry, option))
        })
}

/// Reads an `EFI_LOAD_OPTION`: attributes (4 bytes), device path length (2),
/// description (UTF-16, NUL-terminated), device path.
fn parse_load_option(bytes: &[u8]) -> Option<LoadOption> {
    let attributes = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
    let path_len = usize::from(u16::from_le_bytes([*bytes.get(4)?, *bytes.get(5)?]));
    let mut units = Vec::new();
    let mut offset = 6;
    loop {
        let unit = u16::from_le_bytes([*bytes.get(offset)?, *bytes.get(offset + 1)?]);
        offset += 2;
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    let description = String::from_utf16_lossy(&units);
    Some(LoadOption {
        // Another probe entry would only start the probe again.
        bootable: attributes & ACTIVE != 0
            && attributes & CATEGORY == 0
            && description != ENTRY_DESCRIPTION,
        description,
        device_path: bytes.get(offset..offset + path_len)?.to_vec(),
    })
}

/// A variable of the UEFI specification (`BootOrder`, `SecureBoot`...).
fn global_variable(name: &str) -> Option<Vec<u8>> {
    let name = CString16::try_from(name).ok()?;
    let (data, _) = runtime::get_variable_boxed(&name, &VariableVendor::GLOBAL_VARIABLE).ok()?;
    Some(data.to_vec())
}

fn global_u16(name: &str) -> Option<u16> {
    let bytes = global_variable(name)?;
    Some(u16::from_le_bytes([*bytes.first()?, *bytes.get(1)?]))
}

/// Leaves the report for Linux: a volatile variable (gone at the next power-off),
/// readable after the firmware hands over.
fn write_report(report: &Report) {
    let (Ok(name), Ok(guid)) = (CString16::try_from(VARIABLE), Guid::try_parse(GUID)) else {
        return;
    };
    let attributes = VariableAttributes::BOOTSERVICE_ACCESS | VariableAttributes::RUNTIME_ACCESS;
    let _ = runtime::set_variable(
        &name,
        &VariableVendor(guid),
        attributes,
        report.encode().as_bytes(),
    );
}
