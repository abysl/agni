use super::prelude::{done, friendly_units, play, ready, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub fn ready_your_units(ctx: &mut Ctx, seat: u8) -> usize {
    let readied = friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| ready(ctx, *unit))
        .count();
    ctx.narrate(format!("{{seat {seat}}} readies {readied} units"));
    readied
}

fn hunt(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    ready_your_units(ctx, item.controller);
    done()
}

pub static CARD: Card = spell("On the Hunt", &[], &[play(&[], hunt)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const HUNT: u32 = 90;
    const THEIR_HUNT: u32 = 91;
    const SECOND: u32 = 92;
    const BODY_RUNE: u32 = 100;
    const CHAOS_RUNE: u32 = 101;

    fn hunt(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "On the Hunt", 1, 2);
        card.domain = vec!["Body".into(), "Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hunt(HUNT, 0));
        fixture.table.cards.push(hunt(THEIR_HUNT, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        let mut second = fixtures::unit(SECOND, fixtures::BF1, 0, "Scout", 2);
        second.exhausted = true;
        fixture.table.cards.push(second);
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.table.card_mut(fixtures::SPRITE).unwrap().exhausted = true;
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_targetless_sorcery() {
        assert!(std::ptr::eq(script_of("On the Hunt").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
    }

    #[test]
    fn every_friendly_unit_readies_wherever_it_stands_and_enemy_units_stay_exhausted() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HUNT).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.d · nothing is targeted");
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "nothing happens before it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted, "in the base");
        assert!(!ctx.card(SECOND).unwrap().exhausted, "at a battlefield too");
        assert!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "an enemy unit stays exhausted"
        );
        assert!(ctx.card(fixtures::SPRITE).unwrap().exhausted);
        for unit in [fixtures::VI, SECOND] {
            assert!(ctx
                .events
                .iter()
                .any(|event| matches!(event, Event::Readied { card, by: 0 } if *card == unit)));
        }
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} readies 2 units".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(HUNT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_unit_already_ready_raises_nothing() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = false;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HUNT).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Readied { .. }))
                .count(),
            1,
            "only the Scout changed state"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} readies 1 units".to_string()));
    }

    #[test]
    fn the_hunt_is_the_turn_players_and_needs_its_body_or_chaos_power() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_HUNT)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut poor = armed();
        poor.table.card_mut(CHAOS_RUNE).unwrap().domain = vec!["Body".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, HUNT)),
            Err(Refusal::NoPowerOf),
            "one Body and one Chaos, not two Body"
        );
    }
}
