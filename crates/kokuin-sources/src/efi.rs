//! UEFI data structures shared by the EFI variables, the TPM log and kokuin's own
//! boot entries: strings, GUIDs, device paths, load options. Read here, and written
//! too (`RawLoadOption::encode`), so that boot entries can be created.

use crate::binary::Reader;

/// Decodes a UTF-16LE string up to its NUL terminator (or the end). Returns the
/// string and the number of bytes consumed, terminator included.
pub fn utf16_until_nul(bytes: &[u8]) -> (String, usize) {
    let mut units = Vec::new();
    let mut consumed = 0;
    for pair in bytes.as_chunks::<2>().0 {
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
pub fn utf16(bytes: &[u8]) -> String {
    utf16_until_nul(bytes).0
}

/// Formats a GUID stored in the UEFI mixed-endian layout.
pub fn guid(bytes: &[u8]) -> Option<String> {
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

/// The UEFI mixed-endian bytes of a GUID written as text, the reverse of [`guid`].
pub fn guid_bytes(text: &str) -> Option<[u8; 16]> {
    let hex: String = text.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 {
        return None;
    }
    let mut plain = [0u8; 16];
    for (i, byte) in plain.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hex.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    // The first three fields are stored little-endian.
    let mut bytes = plain;
    bytes[..4].reverse();
    bytes[4..6].reverse();
    bytes[6..8].reverse();
    Some(bytes)
}

/// A hard drive node: a GPT partition, by number, position and GUID. `start` and
/// `size` count the disk's logical blocks.
pub fn hard_drive_node(number: u32, start: u64, size: u64, partition_guid: [u8; 16]) -> Node {
    let mut body = number.to_le_bytes().to_vec();
    body.extend(start.to_le_bytes());
    body.extend(size.to_le_bytes());
    body.extend(partition_guid);
    // Partition format: GPT; signature type: GUID.
    body.extend([2, 2]);
    Node {
        kind: MEDIA,
        subtype: MEDIA_HARD_DRIVE,
        body,
    }
}

/// What kokuin uses from a UEFI device path.
#[derive(Debug, Default, PartialEq)]
pub struct DevicePath {
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
pub fn parse_device_path(bytes: &[u8]) -> DevicePath {
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
pub struct LoadOption {
    /// The name shown in the firmware's boot menu.
    pub description: String,
    /// Where the program is.
    pub path: DevicePath,
}

/// Parses an `EFI_LOAD_OPTION`.
pub fn parse_load_option(bytes: &[u8]) -> Option<LoadOption> {
    let raw = RawLoadOption::parse(bytes)?;
    Some(LoadOption {
        path: parse_device_path(&raw.device_path),
        description: raw.description,
    })
}

/// `LOAD_OPTION_ACTIVE`: the entry can be booted.
pub const LOAD_OPTION_ACTIVE: u32 = 0x1;

/// An `EFI_LOAD_OPTION` as stored, to read and write boot entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLoadOption {
    /// Flags (`LOAD_OPTION_ACTIVE`...).
    pub attributes: u32,
    /// The name shown in the firmware's boot menu.
    pub description: String,
    /// The device path of the program, end node included.
    pub device_path: Vec<u8>,
    /// Data handed to the program.
    pub optional_data: Vec<u8>,
}

impl RawLoadOption {
    /// Reads an `EFI_LOAD_OPTION`: attributes, device path length, description
    /// (UTF-16, NUL-terminated), device path, optional data.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader::new(bytes);
        let attributes = reader.u32()?;
        let path_len = usize::from(reader.u16()?);
        let (description, consumed) = utf16_until_nul(reader.rest());
        reader.take(consumed)?;
        let device_path = reader.take(path_len).unwrap_or(reader.rest()).to_vec();
        Some(Self {
            attributes,
            description,
            device_path,
            optional_data: reader.rest().to_vec(),
        })
    }

    /// The bytes of the `EFI_LOAD_OPTION`.
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = self.attributes.to_le_bytes().to_vec();
        let path_len = u16::try_from(self.device_path.len()).unwrap_or(u16::MAX);
        bytes.extend(path_len.to_le_bytes());
        bytes.extend(utf16z(&self.description));
        bytes.extend(&self.device_path);
        bytes.extend(&self.optional_data);
        bytes
    }
}

/// One node of a device path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// Type (media, hardware, end...).
    pub kind: u8,
    /// Sub-type within the type.
    pub subtype: u8,
    /// The node's data, after its 4-byte header.
    pub body: Vec<u8>,
}

impl Node {
    /// A file path node (`\EFI\kokuin\probe.efi`).
    pub fn file_path(path: &str) -> Self {
        Self {
            kind: MEDIA,
            subtype: MEDIA_FILE_PATH,
            body: utf16z(path),
        }
    }

    fn is_file_path(&self) -> bool {
        (self.kind, self.subtype) == (MEDIA, MEDIA_FILE_PATH)
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
        bytes.extend([self.kind, self.subtype]);
        let len = u16::try_from(self.body.len() + 4).unwrap_or(u16::MAX);
        bytes.extend(len.to_le_bytes());
        bytes.extend(&self.body);
    }
}

/// The nodes of a device path, up to its end node (not included).
pub fn device_path_nodes(bytes: &[u8]) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut reader = Reader::new(bytes);
    while let (Some(kind), Some(subtype), Some(len)) = (reader.u8(), reader.u8(), reader.u16()) {
        if kind == END || len < 4 {
            break;
        }
        let Some(body) = reader.take(usize::from(len) - 4) else {
            break;
        };
        nodes.push(Node {
            kind,
            subtype,
            body: body.to_vec(),
        });
    }
    nodes
}

/// A device path from its nodes, with the end node added.
pub fn encode_device_path(nodes: &[Node]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for node in nodes {
        node.encode(&mut bytes);
    }
    bytes.extend([END, 0xff, 4, 0]);
    bytes
}

/// The same device path pointing to another file on the same partition: its file
/// nodes are replaced by one for `file`.
pub fn with_file(device_path: &[u8], file: &str) -> Vec<u8> {
    let mut nodes: Vec<Node> = device_path_nodes(device_path)
        .into_iter()
        .filter(|node| !node.is_file_path())
        .collect();
    nodes.push(Node::file_path(file));
    encode_device_path(&nodes)
}

/// A string in UTF-16LE with its NUL terminator, as UEFI stores them.
pub fn utf16z(text: &str) -> Vec<u8> {
    text.encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn file_path_node(path: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        Node::file_path(path).encode(&mut bytes);
        bytes
    }

    pub(crate) fn end_node() -> Vec<u8> {
        vec![END, 0xff, 4, 0]
    }

    /// Partition 1 of a GPT disk.
    fn hard_drive_node() -> Node {
        let guid = guid_bytes("c12a7328-f81f-11d2-ba4b-00a0c93ec93b").unwrap();
        super::hard_drive_node(1, 2048, 1_048_576, guid)
    }

    #[test]
    fn guids_go_both_ways() {
        let text = "8be4df61-93ca-11d2-aa0d-00e098032b8c";
        assert_eq!(guid(&guid_bytes(text).unwrap()).as_deref(), Some(text));
        assert_eq!(guid_bytes("not a guid"), None);
    }

    #[test]
    fn a_boot_entry_survives_encoding() {
        let entry = RawLoadOption {
            attributes: LOAD_OPTION_ACTIVE,
            description: "kokuin probe".into(),
            device_path: encode_device_path(&[
                hard_drive_node(),
                Node::file_path("\\EFI\\kokuin\\probe.efi"),
            ]),
            optional_data: vec![1, 2, 3],
        };
        let bytes = entry.encode();
        assert_eq!(RawLoadOption::parse(&bytes).as_ref(), Some(&entry));
        let summary = parse_load_option(&bytes).unwrap();
        assert_eq!(summary.description, "kokuin probe");
        assert_eq!(
            summary.path.file.as_deref(),
            Some("\\EFI\\kokuin\\probe.efi")
        );
        assert_eq!(
            summary.path.partition.as_deref(),
            Some("c12a7328-f81f-11d2-ba4b-00a0c93ec93b")
        );
    }

    #[test]
    fn points_a_device_path_to_another_file() {
        let grub = encode_device_path(&[
            hard_drive_node(),
            Node::file_path("\\EFI\\alpine\\grubx64.efi"),
        ]);
        let probe = with_file(&grub, "\\EFI\\kokuin\\probe.efi");
        let nodes = device_path_nodes(&probe);
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0], hard_drive_node());
        assert_eq!(
            parse_device_path(&probe).file.as_deref(),
            Some("\\EFI\\kokuin\\probe.efi")
        );
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
