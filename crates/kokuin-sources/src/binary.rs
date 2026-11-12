//! Reading little-endian binary structures (TPM log, EFI variables).

/// A cursor over bytes. Every read returns `None` past the end.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// Offset of the next byte to read.
    pub(crate) fn position(&self) -> usize {
        self.pos
    }

    /// The bytes not read yet.
    pub(crate) fn rest(&self) -> &'a [u8] {
        &self.data[self.pos..]
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub(crate) fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let bytes = self.data.get(self.pos..end)?;
        self.pos = end;
        Some(bytes)
    }

    pub(crate) fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    pub(crate) fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    pub(crate) fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
}

/// Lowercase hexadecimal.
pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_little_endian_and_stops_at_the_end() {
        let mut reader = Reader::new(&[1, 0, 2, 0, 0, 0, 9]);
        assert_eq!(reader.u16(), Some(1));
        assert_eq!(reader.u32(), Some(2));
        assert_eq!(reader.u16(), None);
        assert_eq!(reader.u8(), Some(9));
        assert!(reader.is_empty());
    }
}
