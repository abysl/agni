use super::{Card, Static, KIND_BATTLEFIELD};

pub static CARD: Card = Card {
    name: "Rockfall Path",
    keywords: &[],
    abilities: &[],
    statics: &[Static::NoUnitsPlayedHere],
    replacement: None,
    additional: None,
    names: None,
    kind: Some(KIND_BATTLEFIELD),
    adds: None,
};
