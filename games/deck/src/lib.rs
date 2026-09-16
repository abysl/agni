pub mod snapshot;

pub use snapshot::{DeckIdentity, Snapshot, SnapshotCard, SnapshotZone};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CardName(pub String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckEntry<C> {
    pub card: C,
    pub count: u32,
}

pub fn push<C: PartialEq>(zone: &mut Vec<DeckEntry<C>>, card: C, count: u32) {
    if let Some(entry) = zone.iter_mut().find(|entry| entry.card == card) {
        entry.count += count;
    } else {
        zone.push(DeckEntry { card, count });
    }
}

pub fn total<C>(entries: &[DeckEntry<C>]) -> u32 {
    entries.iter().map(|entry| entry.count).sum()
}

pub fn expand<C, T>(entries: &[DeckEntry<C>], mut each: impl FnMut(&C) -> T) -> Vec<T> {
    let mut out = Vec::with_capacity(total(entries) as usize);
    for entry in entries {
        for _ in 0..entry.count {
            out.push(each(&entry.card));
        }
    }
    out
}

pub fn flatten<C>(entries: &[DeckEntry<C>], name: impl Fn(&C) -> &str) -> Vec<CardName> {
    expand(entries, |card| CardName(name(card).to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushing_the_same_card_twice_merges_its_count() {
        let mut zone = Vec::new();
        push(&mut zone, "Ambush", 2);
        push(&mut zone, "Ledge Hopper", 1);
        push(&mut zone, "Ambush", 1);
        assert_eq!(zone.len(), 2);
        assert_eq!(zone[0].count, 3);
        assert_eq!(total(&zone), 4);
    }

    #[test]
    fn expansion_repeats_each_card_by_its_count() {
        let zone = vec![
            DeckEntry {
                card: "Ambush",
                count: 2,
            },
            DeckEntry {
                card: "Spare Blade",
                count: 1,
            },
        ];
        let names = flatten(&zone, |card| card);
        assert_eq!(
            names,
            vec![
                CardName("Ambush".into()),
                CardName("Ambush".into()),
                CardName("Spare Blade".into()),
            ]
        );
        assert_eq!(expand(&zone, |card| card.len()), vec![6, 6, 11]);
    }
}
