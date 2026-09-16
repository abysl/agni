use crate::cbor::{Item, Reader, Writer};

#[derive(Default)]
pub struct MapWriter {
    body: Writer,
    fields: usize,
}

impl MapWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn field(&mut self, key: &str) -> &mut Writer {
        self.fields += 1;
        self.body.text(key);
        &mut self.body
    }

    pub fn len(&self) -> usize {
        self.fields
    }

    pub fn is_empty(&self) -> bool {
        self.fields == 0
    }

    pub fn write_into(self, writer: &mut Writer) {
        writer.map(self.fields);
        writer.raw(&self.body.finish());
    }

    pub fn finish(self) -> Vec<u8> {
        let mut writer = Writer::new();
        self.write_into(&mut writer);
        writer.finish()
    }
}

pub struct MapReader<'r, 'a> {
    reader: &'r mut Reader<'a>,
    left: usize,
    broken: bool,
}

impl<'r, 'a> MapReader<'r, 'a> {
    pub fn open(reader: &'r mut Reader<'a>) -> Option<Self> {
        let left = reader.map_len()?;
        Some(Self {
            reader,
            left,
            broken: false,
        })
    }

    pub fn field(&mut self) -> Option<&'a str> {
        if self.broken || self.left == 0 {
            return None;
        }
        self.left -= 1;
        match self.reader.item() {
            Some(Item::Text(key)) => Some(key),
            _ => {
                self.broken = true;
                None
            }
        }
    }

    pub fn value(&mut self) -> &mut Reader<'a> {
        self.reader
    }

    pub fn skip_unknown(&mut self) -> Option<()> {
        self.reader.skip()
    }

    pub fn remaining(&self) -> usize {
        self.left
    }

    pub fn finish(self) -> Option<()> {
        if self.broken {
            return None;
        }
        for _ in 0..self.left {
            self.reader.skip()?;
            self.reader.skip()?;
        }
        Some(())
    }
}

pub fn discriminator(bytes: &[u8]) -> Option<(&str, u64)> {
    let mut reader = Reader::new(bytes);
    let len = reader.map_len()?;
    if len == 0 {
        return None;
    }
    let key = reader.key()?;
    let value = reader.unsigned()?;
    Some((key, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> Vec<u8> {
        let mut map = MapWriter::new();
        map.field("v").unsigned(4);
        map.field("m").unsigned(1);
        let lines = map.field("lg");
        lines.array(2);
        lines.text("one");
        lines.text("two");
        map.field("np").unsigned(300);
        map.finish()
    }

    #[test]
    fn a_map_is_written_with_its_field_count_and_read_back_by_key() {
        let bytes = document();
        assert_eq!(bytes[0], 0xa4);
        assert_eq!(discriminator(&bytes), Some(("v", 4)));
        let mut reader = Reader::new(&bytes);
        let mut map = MapReader::open(&mut reader).unwrap();
        let mut version = 0;
        let mut mode = 0;
        let mut lines = Vec::new();
        let mut next = 0;
        while let Some(key) = map.field() {
            match key {
                "v" => version = map.value().unsigned().unwrap(),
                "m" => mode = map.value().unsigned().unwrap(),
                "lg" => {
                    for _ in 0..map.value().array_len().unwrap() {
                        let Some(Item::Text(line)) = map.value().item() else {
                            panic!("a line");
                        };
                        lines.push(line.to_string());
                    }
                }
                "np" => next = map.value().unsigned().unwrap(),
                _ => map.skip_unknown().unwrap(),
            }
        }
        assert_eq!(map.finish(), Some(()));
        assert_eq!((version, mode, next), (4, 1, 300));
        assert_eq!(lines, ["one", "two"]);
    }

    #[test]
    fn unknown_keys_are_skipped_and_leftovers_are_drained_by_finish() {
        let bytes = document();
        let mut reader = Reader::new(&bytes);
        let mut map = MapReader::open(&mut reader).unwrap();
        let mut version = 0;
        while let Some(key) = map.field() {
            match key {
                "v" => version = map.value().unsigned().unwrap(),
                _ => map.skip_unknown().unwrap(),
            }
        }
        assert_eq!(map.remaining(), 0);
        assert_eq!(map.finish(), Some(()));
        assert_eq!(version, 4);
        assert_eq!(reader.position(), bytes.len());

        let mut reader = Reader::new(&bytes);
        let mut map = MapReader::open(&mut reader).unwrap();
        assert_eq!(map.field(), Some("v"));
        assert_eq!(map.value().unsigned(), Some(4));
        assert_eq!(map.remaining(), 3);
        assert_eq!(map.finish(), Some(()));
        assert_eq!(reader.position(), bytes.len());
    }

    #[test]
    fn a_non_text_key_or_a_missing_map_is_malformed() {
        let mut writer = Writer::new();
        writer.map(1);
        writer.unsigned(7);
        writer.unsigned(8);
        let bytes = writer.finish();
        let mut reader = Reader::new(&bytes);
        let mut map = MapReader::open(&mut reader).unwrap();
        assert_eq!(map.field(), None);
        assert_eq!(map.finish(), None);
        let mut reader = Reader::new(&[0x81, 0x01]);
        assert!(MapReader::open(&mut reader).is_none());
        assert_eq!(discriminator(&[0xa0]), None);
        assert_eq!(discriminator(&[0x00]), None);
        assert!(MapWriter::new().is_empty());
        let mut nested = Writer::new();
        let mut inner = MapWriter::new();
        inner.field("x").bool(true);
        assert_eq!(inner.len(), 1);
        inner.write_into(&mut nested);
        assert_eq!(nested.finish(), [0xa1, 0x61, b'x', 0xf5]);
    }
}
