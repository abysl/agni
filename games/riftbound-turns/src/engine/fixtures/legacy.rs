use crate::state::GameBlob;
use agni_plugin_sdk::cbor::{Item, Reader, Writer};
use std::ops::Range;

const V11: u64 = 11;
const V11_CHAIN_FIELDS: usize = 13;
const V11_SEAT_FIELDS: usize = 14;
const V11_CARD_FIELDS: usize = 13;
const V11_NOTED_FIELDS: usize = 4;
const V11_PENDING_FIELDS: usize = 2;
const CHAIN_NOTED_FIELD: usize = 9;
const V11_PREVENTION_FIELDS: Range<usize> = 1..4;

pub fn encode_v11(blob: &GameBlob) -> Vec<u8> {
    project_v11(&blob.encode())
}

fn raw_value<'a>(reader: &mut Reader, bytes: &'a [u8]) -> &'a [u8] {
    let start = reader.position();
    reader.skip().expect("complete fixture value");
    &bytes[start..reader.position()]
}

fn project_row(reader: &mut Reader, bytes: &[u8], writer: &mut Writer, keep: Range<usize>) {
    let fields = reader.array_len().expect("fixture row");
    assert!(
        fields >= keep.end,
        "fixture row is shorter than its v11 fields"
    );
    writer.array(keep.len());
    for index in 0..fields {
        let value = raw_value(reader, bytes);
        if keep.contains(&index) {
            writer.raw(value);
        }
    }
}

fn chain_item(reader: &mut Reader, bytes: &[u8], writer: &mut Writer) {
    let fields = reader.array_len().expect("chain fixture row");
    assert!(fields >= V11_CHAIN_FIELDS);
    writer.array(V11_CHAIN_FIELDS);
    for index in 0..fields {
        let value = raw_value(reader, bytes);
        if index == CHAIN_NOTED_FIELD && matches!(Reader::new(value).item(), Some(Item::Array(_))) {
            project_row(&mut Reader::new(value), value, writer, 0..V11_NOTED_FIELDS);
        } else if index < V11_CHAIN_FIELDS {
            writer.raw(value);
        }
    }
}

fn pending_item(reader: &mut Reader, bytes: &[u8], writer: &mut Writer) {
    let fields = reader.array_len().expect("pending fixture row");
    assert!(fields >= V11_PENDING_FIELDS);
    writer.array(V11_PENDING_FIELDS);
    chain_item(reader, bytes, writer);
    writer.raw(raw_value(reader, bytes));
    for _ in V11_PENDING_FIELDS..fields {
        reader.skip().expect("complete pending fixture row");
    }
}

fn rows(reader: &mut Reader, bytes: &[u8], writer: &mut Writer, key: &str) {
    let count = reader.array_len().expect("fixture collection");
    writer.array(count);
    for _ in 0..count {
        match key {
            "ch" => chain_item(reader, bytes, writer),
            "q" => pending_item(reader, bytes, writer),
            "s" => project_row(reader, bytes, writer, 0..V11_SEAT_FIELDS),
            "c" => project_row(reader, bytes, writer, 0..V11_CARD_FIELDS),
            "pv" => project_row(reader, bytes, writer, V11_PREVENTION_FIELDS),
            _ => unreachable!("only versioned fixture collections"),
        }
    }
}

fn project_v11(bytes: &[u8]) -> Vec<u8> {
    let mut reader = Reader::new(bytes);
    let mut writer = Writer::new();
    let fields = reader.map_len().expect("fixture blob map");
    writer.map(fields);
    for _ in 0..fields {
        let start = reader.position();
        let Some(Item::Text(key)) = reader.item() else {
            panic!("non-text fixture blob key");
        };
        writer.raw(&bytes[start..reader.position()]);
        match key {
            "v" => {
                reader.skip().unwrap();
                writer.unsigned(V11);
            }
            "ch" | "q" | "s" | "c" | "pv" => rows(&mut reader, bytes, &mut writer, key),
            _ => writer.raw(raw_value(&mut reader, bytes)),
        }
    }
    assert_eq!(reader.position(), bytes.len());
    writer.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        Amount, ChainItem, DamageSource, Expiry, ItemKind, Limited, Needs, Noted, Origin, Pending,
        Prevention, Price,
    };
    use agni_plugin_sdk::blob::discriminator;

    #[test]
    fn v11_projection_preserves_resume_state_and_drops_later_fields() {
        let mut blob = GameBlob::default();
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: 90,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.stage = 3;
        item.noted = Some(Noted {
            zone: 9,
            might: 2,
            controller: 0,
            alone: true,
            buffed: true,
        });
        item.limited = Some(Limited {
            zones: vec![9],
            price: Price::Free,
        });
        item.ability_script = Some("Honest Broker".into());
        blob.chain.push(item.clone());
        blob.queue.push(Pending {
            item: item.clone(),
            needs: Needs::Choices,
        });
        blob.seat_mut(0).equipment_played = true;
        blob.card_state_mut(90).named = Some("fixture name".into());
        blob.preventions.push(Prevention {
            unit: None,
            source: DamageSource::Any,
            value: Amount::N(2),
            until: Expiry::EndOfTurn(1),
        });
        let bytes = encode_v11(&blob);
        assert_eq!(discriminator(&bytes), Some(("v", V11)));
        let restored = GameBlob::decode(&bytes).expect("shared fixture decodes as v11");
        item.ability_script = None;
        item.limited = None;
        item.noted.as_mut().unwrap().buffed = false;
        assert_eq!(restored.chain, [item.clone()]);
        assert_eq!(restored.queue[0].item, item);
        assert_eq!(restored.queue[0].needs, Needs::Choices);
        assert!(!restored.seat(0).equipment_played);
        assert_eq!(restored.named(90), Some("fixture name"));
        assert_eq!(restored.preventions, blob.preventions);
    }

    #[test]
    fn appended_row_fields_do_not_change_the_v11_projection() {
        for later_fields in [1, 2, 5] {
            let mut current = Writer::new();
            current.array(V11_CHAIN_FIELDS + later_fields);
            for _ in 0..V11_CHAIN_FIELDS {
                current.null();
            }
            for _ in 0..later_fields {
                current.array(2);
                current.text("future field");
                current.unsigned(42);
            }
            let bytes = current.finish();
            let mut reader = Reader::new(&bytes);
            let mut legacy = Writer::new();
            chain_item(&mut reader, &bytes, &mut legacy);
            assert_eq!(reader.position(), bytes.len());
            let mut expected = Writer::new();
            expected.array(V11_CHAIN_FIELDS);
            for _ in 0..V11_CHAIN_FIELDS {
                expected.null();
            }
            assert_eq!(legacy.finish(), expected.finish());
        }
    }
}
