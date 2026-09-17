use super::prelude::{done, grant_this_turn, location_of, play, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const GRANTED: Keyword = Keyword::Assault(1);

pub fn other_friendly_units_here(ctx: &Ctx, seat: u8, me: u32) -> Vec<u32> {
    let Some(here) = location_of(ctx, me) else {
        return Vec::new();
    };
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| *unit != me && ctx.controller(*unit) == seat)
        .collect()
}

pub fn rally(ctx: &mut Ctx, item: &Item, keyword: Keyword, word: &str) -> Flow {
    let me = item.kind.source();
    let others = other_friendly_units_here(ctx, item.controller, me);
    if others.is_empty() {
        ctx.narrate(format!("{{card {me}}} has no other friendly unit here"));
        return done();
    }
    for unit in &others {
        grant_this_turn(ctx, *unit, keyword);
    }
    ctx.narrate(format!(
        "{{card {me}}} gives {} other friendly unit{} here [{word}] this turn",
        others.len(),
        if others.len() == 1 { "" } else { "s" }
    ));
    done()
}

fn roar(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    rally(ctx, item, GRANTED, "Assault")
}

pub static CARD: Card = unit("Lord Broadmane", &[Keyword::Ambush], &[play(&[], roar)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, play as play_engine, settle};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const BROADMANE: u32 = 90;
    const ALLY: u32 = 91;
    const THEIR_BRUTE: u32 = 92;

    fn broadmane(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(BROADMANE, zone, seat, "Lord Broadmane", 5)
        }
    }

    fn pride() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(broadmane(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        for id in 46..=49 {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Fury", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn enter(ctx: &mut Ctx, to: Location) {
        play_engine::begin(ctx, 0, BROADMANE, Origin::Hand, Some(to)).unwrap();
        settle(ctx).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing is chosen");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BROADMANE
        ));
    }

    fn has_assault(ctx: &Ctx, unit: u32) -> bool {
        ctx.has_keyword(unit, GRANTED)
    }

    #[test]
    fn the_script_prints_ambush_and_one_untargeted_play_trigger() {
        assert!(std::ptr::eq(script_of("Lord Broadmane").unwrap(), &CARD));
        assert_eq!(CARD.name, "Lord Broadmane");
        assert_eq!(CARD.keywords, [Keyword::Ambush]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_none());
        assert_eq!(GRANTED, Keyword::Assault(1));
        let mut fixture = pride();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.ambush_locations(0, BROADMANE),
            [Location::Battlefield(fixtures::BF1)],
            "the reaction play reaches the battlefield where you have units"
        );
    }

    #[test]
    fn played_to_a_battlefield_the_other_friendly_units_there_gain_assault_for_the_turn() {
        let mut fixture = pride();
        let mut ctx = fixture.ctx();
        enter(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert!(!has_assault(&ctx, ALLY), "the grant waits for the trigger");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(has_assault(&ctx, ALLY));
        assert!(
            !has_assault(&ctx, THEIR_BRUTE),
            "an enemy here gets nothing"
        );
        assert!(
            !has_assault(&ctx, fixtures::VI),
            "a friendly unit elsewhere gets nothing"
        );
        assert!(!has_assault(&ctx, BROADMANE), "other units, not himself");
        assert_eq!(
            ctx.state_of(ALLY).unwrap().granted,
            [(GRANTED, this_turn(&ctx))]
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BROADMANE}}} gives 1 other friendly unit here [Assault] this turn"
        )));
        phases::end_turn(&mut ctx).unwrap();
        assert!(!has_assault(&ctx, ALLY), "the grant lapses with the turn");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn alone_at_his_location_he_has_nobody_to_rally() {
        let mut fixture = pride();
        let mut ctx = fixture.ctx();
        enter(&mut ctx, Location::Base(0));
        ctx.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        ctx.table.card_mut(fixtures::VI).unwrap().seat = 0;
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BROADMANE}}} has no other friendly unit here"
        )));
        assert!(!has_assault(&ctx, ALLY));
        assert!(!has_assault(&ctx, fixtures::VI));
    }

    #[test]
    fn the_units_here_are_read_as_the_trigger_resolves() {
        let mut fixture = pride();
        let mut ctx = fixture.ctx();
        enter(&mut ctx, Location::Battlefield(fixtures::BF1));
        ctx.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixtures::pass_until_open(&mut ctx);
        assert!(has_assault(&ctx, ALLY));
        assert!(
            has_assault(&ctx, fixtures::VI),
            "she arrived before the trigger resolved"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BROADMANE}}} gives 2 other friendly units here [Assault] this turn"
        )));
    }
}
