#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item<'a> {
    Unsigned(u64),
    Negative(u64),
    Bytes(&'a [u8]),
    Text(&'a str),
    Array(usize),
    Map(usize),
    Tag(u64),
    Simple(u8),
    Opaque,
}

pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    fn byte(&mut self) -> Option<u8> {
        let byte = *self.bytes.get(self.pos)?;
        self.pos += 1;
        Some(byte)
    }

    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let end = self.pos.checked_add(len)?;
        let slice = self.bytes.get(self.pos..end)?;
        self.pos = end;
        Some(slice)
    }

    fn argument(&mut self, info: u8) -> Option<u64> {
        Some(match info {
            0..=23 => info as u64,
            24 => self.byte()? as u64,
            25 => u16::from_be_bytes(self.take(2)?.try_into().ok()?) as u64,
            26 => u32::from_be_bytes(self.take(4)?.try_into().ok()?) as u64,
            27 => u64::from_be_bytes(self.take(8)?.try_into().ok()?),
            _ => return None,
        })
    }

    pub fn item(&mut self) -> Option<Item<'a>> {
        let head = self.byte()?;
        let (major, info) = (head >> 5, head & 0x1f);
        Some(match major {
            0 => Item::Unsigned(self.argument(info)?),
            1 => Item::Negative(self.argument(info)?),
            2 => {
                let len = usize::try_from(self.argument(info)?).ok()?;
                Item::Bytes(self.take(len)?)
            }
            3 => {
                let len = usize::try_from(self.argument(info)?).ok()?;
                Item::Text(core::str::from_utf8(self.take(len)?).ok()?)
            }
            4 => Item::Array(usize::try_from(self.argument(info)?).ok()?),
            5 => Item::Map(usize::try_from(self.argument(info)?).ok()?),
            6 => Item::Tag(self.argument(info)?),
            _ => match info {
                0..=24 => Item::Simple(self.argument(info)? as u8),
                25 => {
                    self.take(2)?;
                    Item::Opaque
                }
                26 => {
                    self.take(4)?;
                    Item::Opaque
                }
                27 => {
                    self.take(8)?;
                    Item::Opaque
                }
                _ => return None,
            },
        })
    }

    pub fn skip(&mut self) -> Option<()> {
        let mut pending: usize = 1;
        while pending > 0 {
            pending -= 1;
            match self.item()? {
                Item::Array(len) => pending = pending.checked_add(len)?,
                Item::Map(len) => pending = pending.checked_add(len.checked_mul(2)?)?,
                Item::Tag(_) => pending = pending.checked_add(1)?,
                _ => {}
            }
        }
        Some(())
    }

    pub fn map_len(&mut self) -> Option<usize> {
        match self.item()? {
            Item::Map(len) => Some(len),
            _ => None,
        }
    }

    pub fn key(&mut self) -> Option<&'a str> {
        match self.item()? {
            Item::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn unsigned(&mut self) -> Option<u64> {
        match self.item()? {
            Item::Unsigned(value) => Some(value),
            _ => None,
        }
    }

    pub fn boolean(&mut self) -> Option<bool> {
        match self.item()? {
            Item::Simple(21) => Some(true),
            Item::Simple(20) => Some(false),
            _ => None,
        }
    }

    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        match self.item()? {
            Item::Bytes(bytes) => Some(bytes),
            _ => None,
        }
    }

    pub fn array_len(&mut self) -> Option<usize> {
        match self.item()? {
            Item::Array(len) => Some(len),
            _ => None,
        }
    }
}

pub struct Writer {
    out: Vec<u8>,
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

impl Writer {
    pub fn new() -> Self {
        Self { out: Vec::new() }
    }

    pub fn head(&mut self, major: u8, argument: u64) {
        let shifted = major << 5;
        if argument < 24 {
            self.out.push(shifted | argument as u8);
        } else if argument <= u8::MAX as u64 {
            self.out.push(shifted | 24);
            self.out.push(argument as u8);
        } else if argument <= u16::MAX as u64 {
            self.out.push(shifted | 25);
            self.out.extend((argument as u16).to_be_bytes());
        } else if argument <= u32::MAX as u64 {
            self.out.push(shifted | 26);
            self.out.extend((argument as u32).to_be_bytes());
        } else {
            self.out.push(shifted | 27);
            self.out.extend(argument.to_be_bytes());
        }
    }

    pub fn map(&mut self, len: usize) {
        self.head(5, len as u64);
    }

    pub fn array(&mut self, len: usize) {
        self.head(4, len as u64);
    }

    pub fn unsigned(&mut self, value: u64) {
        self.head(0, value);
    }

    pub fn signed(&mut self, value: i64) {
        if value < 0 {
            self.head(1, (-1 - value) as u64);
        } else {
            self.head(0, value as u64);
        }
    }

    pub fn text(&mut self, text: &str) {
        self.head(3, text.len() as u64);
        self.out.extend(text.as_bytes());
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        self.head(2, bytes.len() as u64);
        self.out.extend(bytes);
    }

    pub fn bool(&mut self, value: bool) {
        self.out.push(if value { 0xf5 } else { 0xf4 });
    }

    pub fn null(&mut self) {
        self.out.push(0xf6);
    }

    pub fn raw(&mut self, encoded: &[u8]) {
        self.out.extend(encoded);
    }

    pub fn len(&self) -> usize {
        self.out.len()
    }

    pub fn is_empty(&self) -> bool {
        self.out.is_empty()
    }

    pub fn finish(self) -> Vec<u8> {
        self.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_document_is_walked_and_skipped_without_floats_being_read() {
        let mut writer = Writer::new();
        writer.map(3);
        writer.text("a");
        writer.bytes(&[1, 2, 3]);
        writer.text("b");
        writer.map(1);
        writer.text("inner");
        writer.bool(true);
        writer.text("c");
        writer.null();
        let mut doc = writer.finish();
        doc.extend([0xfb, 0, 0, 0, 0, 0, 0, 0, 0]);
        let mut reader = Reader::new(&doc);
        assert_eq!(reader.map_len(), Some(3));
        assert_eq!(reader.key(), Some("a"));
        assert_eq!(reader.bytes(), Some(&[1u8, 2, 3][..]));
        assert_eq!(reader.key(), Some("b"));
        assert_eq!(reader.skip(), Some(()));
        assert_eq!(reader.key(), Some("c"));
        assert_eq!(reader.item(), Some(Item::Simple(22)));
        assert_eq!(reader.item(), Some(Item::Opaque));
        assert_eq!(reader.item(), None);
    }

    #[test]
    fn wide_arguments_encode_and_decode_in_every_width() {
        for value in [
            0u64,
            23,
            24,
            255,
            256,
            65535,
            65536,
            u32::MAX as u64,
            u64::MAX,
        ] {
            let mut writer = Writer::new();
            writer.head(0, value);
            let bytes = writer.finish();
            assert_eq!(Reader::new(&bytes).unsigned(), Some(value));
        }
    }

    #[test]
    fn a_truncated_document_yields_none_instead_of_panicking() {
        assert_eq!(Reader::new(&[0x5a, 0xff, 0xff, 0xff, 0xff]).skip(), None);
        assert_eq!(Reader::new(&[0x82, 0x01]).skip(), None);
        assert_eq!(Reader::new(&[0x1f]).item(), None);
    }
}
