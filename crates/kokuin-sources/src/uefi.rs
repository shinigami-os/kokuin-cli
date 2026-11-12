//! UEFI data structures shared by the EFI variables and the TPM log: strings, GUIDs,
//! device paths, load options.

use crate::binary::Reader;

/// Decodes a UTF-16LE string up to its NUL terminator (or the end). Returns the
/// string and the number of bytes consumed, terminator included.
pub(crate) fn utf16_until_nul(bytes: &[u8]) -> (String, usize) {
    let mut units = Vec::new();
    let mut consumed = 0;
    for pair in bytes.chunks_exact(2) {
        consumed += 2;
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    (String::from_utf16_lossy(&units), consumed)
}

/// Decodes a whole UTF-16LE buffer, stopping at the first NUL.
pub(crate) fn utf16(bytes: &[u8]) -> String {
    utf16_until_nul(bytes).0
}

/// Formats a GUID stored in the UEFI mixed-endian layout.
pub(crate) fn guid(bytes: &[u8]) -> Option<String> {
    let b: &[u8; 16] = bytes.get(..16)?.try_into().ok()?;
    Some(format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{}",
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u16::from_le_bytes([b[4], b[5]]),
        u16::from_le_bytes([b[6], b[7]]),
        b[8],
        b[9],
        crate::binary::hex(&b[10..16])
    ))
}

/// What kokuin uses from a UEFI device path.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct DevicePath {
    /// File path on the partition (`\EFI\kira\grubx64.efi`).
    pub file: Option<String>,
    /// GPT partition GUID.
    pub partition: Option<String>,
    /// The path points to an application built into the firmware.
    pub firmware_volume: bool,
}

const MEDIA: u8 = 0x04;
const MEDIA_HARD_DRIVE: u8 = 0x01;
const MEDIA_FILE_PATH: u8 = 0x04;
const MEDIA_FIRMWARE_FILE: u8 = 0x06;
const MEDIA_FIRMWARE_VOLUME: u8 = 0x07;
const END: u8 = 0x7f;

/// Parses a device path (a list of nodes) up to its end node.
pub(crate) fn parse_device_path(bytes: &[u8]) -> DevicePath {
    let mut path = DevicePath::default();
    let mut files: Vec<String> = Vec::new();
    let mut reader = Reader::new(bytes);
    while let (Some(kind), Some(subtype), Some(len)) = (reader.u8(), reader.u8(), reader.u16()) {
        let Some(body) = reader.take(usize::from(len).saturating_sub(4)) else {
            break;
        };
        match (kind, subtype) {
            (END, _) => break,
            (MEDIA, MEDIA_FILE_PATH) => files.push(utf16(body)),
            (MEDIA, MEDIA_HARD_DRIVE) => {
                // number u32, start u64, size u64, signature [16], format u8, type u8
                if body.get(37) == Some(&2) {
                    path.partition = guid(&body[20..36]);
                }
            }
            (MEDIA, MEDIA_FIRMWARE_FILE | MEDIA_FIRMWARE_VOLUME) => path.firmware_volume = true,
            _ => {}
        }
        if len < 4 {
            break;
        }
    }
    if !files.is_empty() {
        path.file = Some(files.join("\\").replace("\\\\", "\\"));
    }
    path
}

/// A UEFI boot entry (`Boot####` variable).
#[derive(Debug, PartialEq)]
pub(crate) struct LoadOption {
    pub description: String,
    pub path: DevicePath,
}

/// Parses an `EFI_LOAD_OPTION`.
pub(crate) fn parse_load_option(bytes: &[u8]) -> Option<LoadOption> {
    let mut reader = Reader::new(bytes);
    let _attributes = reader.u32()?;
    let path_len = usize::from(reader.u16()?);
    let (description, consumed) = utf16_until_nul(reader.rest());
    reader.take(consumed)?;
    let path = parse_device_path(reader.take(path_len).unwrap_or(reader.rest()));
    Some(LoadOption { description, path })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn utf16z(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    pub(crate) fn file_path_node(path: &str) -> Vec<u8> {
        let body = utf16z(path);
        let mut node = vec![MEDIA, MEDIA_FILE_PATH];
        node.extend(((body.len() + 4) as u16).to_le_bytes());
        node.extend(body);
        node
    }

    pub(crate) fn end_node() -> Vec<u8> {
        vec![END, 0xff, 4, 0]
    }

    #[test]
    fn formats_guids_in_uefi_byte_order() {
        let bytes = [
            0x61, 0xdf, 0xe4, 0x8b, 0xca, 0x93, 0xd2, 0x11, 0xaa, 0x0d, 0x00, 0xe0, 0x98, 0x03,
            0x2b, 0x8c,
        ];
        assert_eq!(
            guid(&bytes).unwrap(),
            "8be4df61-93ca-11d2-aa0d-00e098032b8c"
        );
    }

    #[test]
    fn parses_a_boot_entry() {
        let mut path = file_path_node("\\EFI\\kira\\grubx64.efi");
        path.extend(end_node());
        let mut bytes = 1u32.to_le_bytes().to_vec();
        bytes.extend((path.len() as u16).to_le_bytes());
        bytes.extend(utf16z("Kira Linux"));
        bytes.extend(path);

        let option = parse_load_option(&bytes).unwrap();
        assert_eq!(option.description, "Kira Linux");
        assert_eq!(
            option.path.file.as_deref(),
            Some("\\EFI\\kira\\grubx64.efi")
        );
    }
}
