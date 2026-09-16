use super::prelude::{card_targets, done, play, ready, spell, target};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const READIES: u8 = 4;
pub const UNIT_GEAR_OR_RUNE: Filter = Filter::Or(&[Filter::Unit, Filter::Gear, Filter::Rune]);
pub const ACCELERATED: TargetSpec = target(
    UNIT_GEAR_OR_RUNE,
    0,
    READIES,
    TargetKind::Card,
    "up to 4 units, gear and/or runes to ready",
);

fn accelerate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let mut readied = 0;
    for card in card_targets(ctx, item) {
        if ready(ctx, card) {
            readied += 1;
            ctx.narrate(format!("{{card {card}}} is readied"));
        } else {
            ctx.narrate(format!("{{card {card}}} is already ready"));
        }
    }
    ctx.narrate(format!("{{seat {seat}}} readies {readied}"));
    done()
}

pub static CARD: Card = spell(
    "Acceleration Gate",
    &[],
    &[play(&[ACCELERATED], accelerate)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const GATE: u32 = 90;
    const THEIR_GATE: u32 = 91;
    const SPENT_GEAR: u32 = 92;
    const THEIR_SPENT_UNIT: u32 = 93;
    const BODY_RUNE: u32 = 46;
    const THEIR_BODY: u32 = 47;

    fn gate(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Acceleration Gate", 3, 1);
        card.domain = vec!["Mind".into(), "Body".into()];
        card
    }

    fn workshop() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(gate(GATE, 0));
        fixture.table.cards.push(gate(THEIR_GATE, 1));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.table.cards.push(CardInfo {
            exhausted: true,
            ..fixtures::gear(SPENT_GEAR, fixtures::BASE, 0, "Hextech Disc", 2)
        });
        fixture.table.cards.push(CardInfo {
            exhausted: true,
            ..fixtures::unit(THEIR_SPENT_UNIT, fixtures::BF2, 1, "Warden", 2)
        });
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(THEIR_BODY, 1, "Body", true));
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

    fn readied(ctx: &Ctx) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Readied { card, .. } => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_sorcery_over_up_to_four_units_gear_or_runes() {
        assert!(std::ptr::eq(script_of("Acceleration Gate").unwrap(), &CARD));
        assert_eq!(CARD.name, "Acceleration Gate");
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, &[ACCELERATED]);
        assert_eq!((ACCELERATED.min, ACCELERATED.max), (0, READIES));
        assert_eq!(ACCELERATED.kind, TargetKind::Card);
        assert_eq!(
            ACCELERATED.filter,
            Filter::Or(&[Filter::Unit, Filter::Gear, Filter::Rune])
        );
        assert!(ability.candidates.is_none());
        assert_eq!(READIES, 4);
    }

    #[test]
    fn four_picks_across_units_gear_and_runes_of_either_seat_are_readied() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GATE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let offered = fixtures::labels(&ctx);
        for card in [
            fixtures::VI,
            fixtures::SPRITE,
            fixtures::THEIR_UNIT,
            SPENT_GEAR,
            THEIR_SPENT_UNIT,
            fixtures::RUNE_A,
            41,
            BODY_RUNE,
            THEIR_BODY,
        ] {
            assert!(
                offered.contains(&format!("{{card {card}}}")),
                "{card} is a unit, gear or rune on the board: {offered:?}"
            );
        }
        for wrong in [
            fixtures::GROUNDS,
            fixtures::LEGEND_CARD,
            fixtures::HAND_UNIT,
            fixtures::CHAMPION_CARD,
        ] {
            assert!(
                !offered.contains(&format!("{{card {wrong}}}")),
                "{wrong} is no unit, gear or rune on the board"
            );
        }
        for pick in [fixtures::VI, SPENT_GEAR, fixtures::RUNE_A, THEIR_SPENT_UNIT] {
            fixtures::choose(&mut ctx, 0, &format!("{{card {pick}}}")).unwrap();
        }
        assert!(
            ctx.blob.prompt.is_some(),
            "four is the most: the group waits for done"
        );
        assert_eq!(fixtures::labels(&ctx), ["done", "cancel"]);
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none());
        let ready_runes = ctx.ready_runes_of(0).len();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Card(SPENT_GEAR),
                TargetRef::Card(fixtures::RUNE_A),
                TargetRef::Card(THEIR_SPENT_UNIT)
            ]
        );
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for card in [fixtures::VI, SPENT_GEAR, fixtures::RUNE_A, THEIR_SPENT_UNIT] {
            assert!(!ctx.card(card).unwrap().exhausted, "{card} is readied");
            assert!(ctx.effects.contains(&Effect::ready(card)));
            assert!(ctx
                .blob
                .log
                .contains(&format!("{{card {card}}} is readied")));
        }
        assert_eq!(
            readied(&ctx),
            [fixtures::VI, SPENT_GEAR, fixtures::RUNE_A, THEIR_SPENT_UNIT],
            "in pick order"
        );
        assert!(ctx.blob.log.contains(&"{seat 0} readies 4".to_string()));
        assert!(
            ctx.card(THEIR_BODY).unwrap().exhausted,
            "the unpicked rune stays spent"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready_runes + 1,
            "the readied rune is back in the pool"
        );
        assert_eq!(ctx.card(GATE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_already_ready_pick_does_nothing_skipping_readies_none_and_a_pick_that_left_is_passed_over(
    ) {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GATE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(readied(&ctx), [fixtures::VI], "Jinx was never exhausted");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} is already ready",
            fixtures::THEIR_UNIT
        )));
        assert!(ctx.blob.log.contains(&"{seat 0} readies 1".to_string()));
        drop(ctx);
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GATE).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain[0].targets.is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(readied(&ctx).is_empty());
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&"{seat 0} readies 0".to_string()));
        drop(ctx);
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GATE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SPENT_GEAR}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::HAND, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(readied(&ctx), [SPENT_GEAR], "Vi left the board");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn battlefields_legends_hand_cards_a_fifth_pick_and_the_other_seats_turn_are_refused() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_GATE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, GATE).unwrap();
        for wrong in [
            fixtures::GROUNDS,
            fixtures::LEGEND_CARD,
            fixtures::HAND_UNIT,
            fixtures::HAND_GEAR,
            fixtures::CHAMPION_CARD,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is no unit, gear or rune on the board"
            );
        }
        assert_eq!(
            play_engine::choose_targets(
                &mut ctx,
                1,
                0,
                &[fixtures::VI, SPENT_GEAR, fixtures::RUNE_A, 41, BODY_RUNE]
            ),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "up to four"
        );
        assert!(fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::VI)).is_err());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(GATE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.blob.is_neutral_open());
    }
}
