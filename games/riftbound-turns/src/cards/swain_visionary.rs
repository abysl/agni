use super::prelude::{done, friendly_gear, friendly_units, on_conquer_me, score_point, unit, when};
use super::{Card, Event, Flow, Item, Keyword, Source, Stage, KIND_GEAR, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::state::FLAG_ENTERED_THIS_TURN;

fn entered_this_turn(ctx: &Ctx, card: u32) -> bool {
    ctx.has_flag(card, FLAG_ENTERED_THIS_TURN)
        && ctx
            .state_of(card)
            .is_some_and(|row| row.entered == ctx.turn())
}

fn played_in_this_request(ctx: &Ctx, seat: u8, wanted: &str) -> bool {
    ctx.events.iter().any(|event| {
        matches!(
            event,
            Event::Played { card, controller, kind, .. }
                if *controller == seat && kind == wanted && !ctx.is_token(*card)
        )
    })
}

pub fn non_token_unit_played_this_turn(ctx: &Ctx, seat: u8) -> bool {
    friendly_units(ctx, seat)
        .into_iter()
        .any(|unit| !ctx.is_token(unit) && entered_this_turn(ctx, unit))
        || played_in_this_request(ctx, seat, KIND_UNIT)
}

pub fn non_token_gear_played_this_turn(ctx: &Ctx, seat: u8) -> bool {
    friendly_gear(ctx, seat)
        .into_iter()
        .any(|gear| !ctx.is_token(gear) && entered_this_turn(ctx, gear))
        || played_in_this_request(ctx, seat, KIND_GEAR)
}

pub fn spell_played_this_turn(ctx: &Ctx, seat: u8) -> bool {
    ctx.blob.seat(seat).spells_played > 0
}

pub fn played_a_unit_a_gear_and_a_spell_this_turn(ctx: &Ctx, seat: u8) -> bool {
    non_token_unit_played_this_turn(ctx, seat)
        && non_token_gear_played_this_turn(ctx, seat)
        && spell_played_this_turn(ctx, seat)
}

fn the_three_kinds_played(ctx: &Ctx, _: &Event, source: Source) -> bool {
    played_a_unit_a_gear_and_a_spell_this_turn(ctx, ctx.controller(source.card))
}

fn foresee(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

pub static CARD: Card = unit(
    "Swain, Visionary",
    &[Keyword::Vision],
    &[when(on_conquer_me(&[], foresee), the_three_kinds_played)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, expiry, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const SWAIN: u32 = 90;
    const RAVEN: u32 = 91;
    const LANTERN: u32 = 92;
    const SOLDIER: u32 = 93;

    fn swain(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(SWAIN, zone, seat, "Swain, Visionary", 6)
        }
    }

    fn war_room() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(swain(fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAVEN, fixtures::BASE, 0, "Raven", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(LANTERN, fixtures::BASE, 0, "Lantern", 1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SWAIN).unwrap(), &CARD));
        fixture
    }

    fn mark_played(ctx: &mut Ctx, card: u32) {
        let turn = ctx.turn();
        let row = ctx.state_mut(card);
        row.entered = turn;
        row.set(FLAG_ENTERED_THIS_TURN, true);
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn his_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == SWAIN))
            .count()
    }

    #[test]
    fn the_script_prints_vision_and_one_conquer_trigger_conditioned_on_the_three_kinds() {
        assert!(std::ptr::eq(script_of("Swain, Visionary").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Vision]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::Me));
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
    }

    #[test]
    fn the_readers_want_a_non_token_unit_a_non_token_gear_and_a_spell_of_yours_this_turn() {
        let mut fixture = war_room();
        let mut ctx = fixture.ctx();
        assert!(!non_token_unit_played_this_turn(&ctx, 0));
        assert!(!non_token_gear_played_this_turn(&ctx, 0));
        assert!(!spell_played_this_turn(&ctx, 0));
        assert!(!played_a_unit_a_gear_and_a_spell_this_turn(&ctx, 0));
        mark_played(&mut ctx, RAVEN);
        assert!(non_token_unit_played_this_turn(&ctx, 0));
        assert!(
            !non_token_unit_played_this_turn(&ctx, 1),
            "yours, not theirs"
        );
        mark_played(&mut ctx, LANTERN);
        assert!(non_token_gear_played_this_turn(&ctx, 0));
        assert!(
            !played_a_unit_a_gear_and_a_spell_this_turn(&ctx, 0),
            "no spell yet"
        );
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.seat(0).spells_played, 1);
        assert!(spell_played_this_turn(&ctx, 0));
        assert!(played_a_unit_a_gear_and_a_spell_this_turn(&ctx, 0));
        expiry::at_expiration(&mut ctx);
        assert!(
            !played_a_unit_a_gear_and_a_spell_this_turn(&ctx, 0),
            "the turn's memory expires with it"
        );
    }

    #[test]
    fn a_token_unit_does_not_count_but_a_play_in_this_request_does() {
        let mut fixture = war_room();
        let mut ctx = fixture.ctx();
        let soldier = ctx
            .spawn(
                0,
                crate::engine::ctx::Token::SandSoldier,
                Location::Base(0),
                false,
            )
            .unwrap();
        mark_played(&mut ctx, soldier);
        assert!(ctx.is_token(soldier));
        assert!(
            !non_token_unit_played_this_turn(&ctx, 0),
            "a token play is a Played event and an entered flag, and neither counts"
        );
        ctx.raise(Event::Played {
            card: SOLDIER,
            controller: 0,
            kind: KIND_UNIT.into(),
            origin: Origin::Hand,
            paid_additional: false,
        });
        assert!(
            non_token_unit_played_this_turn(&ctx, 0),
            "a unit played and gone again this request is remembered by its event"
        );
    }

    #[test]
    fn conquering_after_the_three_kinds_scores_a_second_point_when_the_trigger_resolves() {
        let mut fixture = war_room();
        fixture.blob.seats[0].spells_played = 1;
        let mut ctx = fixture.ctx();
        mark_played(&mut ctx, RAVEN);
        mark_played(&mut ctx, LANTERN);
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 1, "the conquer's own point");
        assert_eq!(his_items(&ctx), 1, "the trigger waits on the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 2, "an unrestricted point on top");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} scores 1 point".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn missing_any_one_kind_the_conquer_scores_its_own_point_only() {
        for missing in ["unit", "gear", "spell"] {
            let mut fixture = war_room();
            if missing != "spell" {
                fixture.blob.seats[0].spells_played = 1;
            }
            let mut ctx = fixture.ctx();
            if missing != "unit" {
                mark_played(&mut ctx, RAVEN);
            }
            if missing != "gear" {
                mark_played(&mut ctx, LANTERN);
            }
            assert!(!played_a_unit_a_gear_and_a_spell_this_turn(&ctx, 0));
            conquer(&mut ctx);
            assert_eq!(
                his_items(&ctx),
                0,
                "383.2.a.1 · the three kinds are the condition · missing the {missing}"
            );
            assert_eq!(ctx.points(0), 1);
            assert!(ctx.fault.is_none());
        }
    }

    #[test]
    fn a_hold_is_not_a_conquer() {
        let mut fixture = war_room();
        fixture.blob.seats[0].spells_played = 1;
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        mark_played(&mut ctx, RAVEN);
        mark_played(&mut ctx, LANTERN);
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(
            his_items(&ctx),
            0,
            "383.4.d · a hold is its own trigger family"
        );
        assert_eq!(ctx.points(0), 1);
    }

    #[test]
    #[ignore = "engine gap · per-turn counters: a non-token unit or gear played in an earlier request this turn and gone from the board since is forgotten (the entered flag leaves with the card and ctx.events is one request deep); the reader wants a per-seat kinds-played record on SeatState reset at Expiration"]
    fn a_unit_played_and_killed_earlier_this_turn_still_counts() {
        let mut fixture = war_room();
        fixture.blob.seats[0].spells_played = 1;
        let mut ctx = fixture.ctx();
        mark_played(&mut ctx, RAVEN);
        mark_played(&mut ctx, LANTERN);
        ctx.kill(RAVEN, crate::engine::ctx::Cause::Rule);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let mut ctx = fixture.ctx();
        assert!(!ctx.on_board(RAVEN));
        assert!(non_token_unit_played_this_turn(&ctx, 0));
        conquer(&mut ctx);
        assert_eq!(his_items(&ctx), 1);
    }

    #[test]
    fn playing_him_looks_at_the_top_card_and_may_recycle_it() {
        let mut fixture = war_room();
        fixture.table.card_mut(SWAIN).unwrap().zone = Some(fixtures::HAND);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        for rune in [46, 47, 48] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SWAIN).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })) {
            fixtures::choose(&mut ctx, 0, "your base").unwrap();
        }
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(SWAIN));
        assert!(
            ctx.blob.prompt.is_some(),
            "the Vision look asks whether to recycle the top card"
        );
    }
}
