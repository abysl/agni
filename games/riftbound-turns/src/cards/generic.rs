use super::prelude::triggered;
use super::{
    Ability, Card, Flow, Item, Keyword, Stage, Trigger, KIND_BATTLEFIELD, KIND_GEAR, KIND_LEGEND,
    KIND_RUNE, KIND_SPELL, KIND_UNIT,
};
use crate::engine::ctx::{Cause, Ctx, Killed};

const fn bare(name: &'static str, keywords: &'static [Keyword]) -> Card {
    Card {
        name,
        keywords,
        abilities: &[],
        statics: &[],
        replacement: None,
        additional: None,
        names: None,
        kind: None,
        adds: None,
    }
}

pub static UNIT: Card = bare("(unit)", &[]);
pub static GEAR: Card = bare("(gear)", &[]);
pub static SPELL: Card = bare("(spell)", &[]);
pub static RUNE: Card = bare("(rune)", &[]);
pub static LEGEND: Card = bare("(legend)", &[]);
pub static BATTLEFIELD: Card = bare("(battlefield)", &[]);
pub static SPRITE: Card = bare("Sprite", &[Keyword::Temporary]);
pub static GOLD: Card = bare("Gold", &[]);
pub static HIDDEN_UNIT: Card = bare("(unit)", &[Keyword::Hidden]);
pub static HIDDEN_GEAR: Card = bare("(gear)", &[Keyword::Hidden]);
pub static HIDDEN_SPELL: Card = bare("(spell)", &[Keyword::Hidden]);

fn expire(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let card = item.kind.source();
    if ctx.kill(card, Cause::Rule) == Killed::Yes {
        ctx.narrate(format!("{{card {card}}} is Temporary and dies"));
    }
    Flow::Done
}

pub static TEMPORARY: Ability = triggered(Trigger::BeginningPhase, &[], expire);

pub static ALL: &[&Card] = &[
    &UNIT,
    &GEAR,
    &SPELL,
    &RUNE,
    &LEGEND,
    &BATTLEFIELD,
    &HIDDEN_UNIT,
    &HIDDEN_GEAR,
    &HIDDEN_SPELL,
];

pub fn for_kind(kind: Option<&str>) -> Option<&'static Card> {
    Some(match kind {
        Some(KIND_GEAR) => &GEAR,
        Some(KIND_SPELL) => &SPELL,
        Some(KIND_RUNE) => &RUNE,
        Some(KIND_LEGEND) => &LEGEND,
        Some(KIND_BATTLEFIELD) => &BATTLEFIELD,
        Some(KIND_UNIT) | None => &UNIT,
        Some(_) => return None,
    })
}

pub fn hidden_for_kind(kind: Option<&str>) -> Option<&'static Card> {
    Some(match kind {
        Some(KIND_GEAR) => &HIDDEN_GEAR,
        Some(KIND_SPELL) => &HIDDEN_SPELL,
        Some(KIND_UNIT) | None => &HIDDEN_UNIT,
        _ => return for_kind(kind),
    })
}

pub fn is_generic(card: &Card) -> bool {
    ALL.iter().any(|held| std::ptr::eq(*held, card))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_bare_script_and_unknown_kinds_have_none() {
        for (kind, expected) in [
            (Some("Unit"), &UNIT),
            (Some("Gear"), &GEAR),
            (Some("Spell"), &SPELL),
            (Some("Rune"), &RUNE),
            (Some("Legend"), &LEGEND),
            (Some("Battlefield"), &BATTLEFIELD),
            (None, &UNIT),
        ] {
            let script = for_kind(kind).unwrap();
            assert!(std::ptr::eq(script, expected));
            assert!(is_generic(script));
            assert!(script.abilities.is_empty());
        }
        assert!(for_kind(Some("Other")).is_none());
        assert!(!is_generic(&SPRITE));
        assert!(!is_generic(&crate::cards::rockfall_path::CARD));
    }
}
