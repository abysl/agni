use super::prelude::{banish_by, done, trigger_subject, unit};
use super::the_zero_drive::banish_me_as_the_cost_until_self_cost_banish_self_pays_it_at_finalization;
use super::{Card, Flow, Item, Source, Stage, KIND_GEAR};
use crate::engine::ctx::{Ctx, Event};

pub fn an_opponent_played_a_gear_until_trigger_opponent_plays_gear_matches_it(
    ctx: &Ctx,
    event: &Event,
    source: Source,
) -> bool {
    match event {
        Event::Played {
            controller, kind, ..
        } => *controller != ctx.controller(source.card) && kind == KIND_GEAR,
        _ => false,
    }
}

pub fn confiscate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(gear) = trigger_subject(item) else {
        ctx.narrate(format!("{{card {me}}} has no gear to banish"));
        return done();
    };
    if !banish_me_as_the_cost_until_self_cost_banish_self_pays_it_at_finalization(ctx, me) {
        ctx.narrate(format!(
            "{{card {me}}} is no longer on the board · the cost is unpaid and {{card {gear}}} stays"
        ));
        return done();
    }
    if !ctx.is_gear(gear) || !ctx.on_board(gear) {
        ctx.narrate(format!(
            "{{card {gear}}} is no longer a gear on the board · nothing more is banished"
        ));
        return done();
    }
    if banish_by(ctx, gear, item.controller) {
        ctx.narrate(format!("{{card {gear}}} is banished with {{card {me}}}"));
    }
    done()
}

pub static CARD: Card = unit("Ravenbloom Prefect", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ItemKind, Origin, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const PREFECT: u32 = 90;
    const THEIR_GEAR: u32 = 91;
    const MY_GEAR: u32 = 92;

    fn prefect(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(PREFECT, zone, 0, "Ravenbloom Prefect", 3)
        }
    }

    fn precinct() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(prefect(fixtures::BASE));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Trinket", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 2));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PREFECT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn source() -> Source {
        Source {
            card: PREFECT,
            ability: 0,
        }
    }

    fn played(card: u32, controller: u8, kind: &str) -> Event {
        Event::Played {
            card,
            controller,
            kind: kind.into(),
            origin: Origin::Hand,
            paid_additional: false,
        }
    }

    fn trigger(subject: Option<u32>) -> Item {
        let mut item = Item::new(
            9,
            ItemKind::Trigger {
                source: PREFECT,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.subject = subject.map(TargetRef::Card);
        item
    }

    #[test]
    fn the_stub_is_the_pool_name_with_no_keywords_and_the_gear_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(
            script_of("Ravenbloom Prefect").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
    }

    #[test]
    fn the_condition_reads_an_opponent_playing_a_gear_and_nothing_else() {
        let mut fixture = precinct();
        let ctx = fixture.ctx();
        let seam = an_opponent_played_a_gear_until_trigger_opponent_plays_gear_matches_it;
        assert!(seam(&ctx, &played(THEIR_GEAR, 1, KIND_GEAR), source()));
        assert!(
            !seam(&ctx, &played(MY_GEAR, 0, KIND_GEAR), source()),
            "your own gear is not an opponent's"
        );
        assert!(
            !seam(&ctx, &played(fixtures::THEIR_UNIT, 1, "Unit"), source()),
            "a unit is not a gear"
        );
        assert!(!seam(&ctx, &Event::Attacks { card: THEIR_GEAR }, source()));
    }

    #[test]
    fn the_effect_banishes_him_as_the_cost_and_then_the_played_gear() {
        let mut fixture = precinct();
        let mut ctx = fixture.ctx();
        assert_eq!(
            confiscate(&mut ctx, &trigger(Some(THEIR_GEAR)), Stage(0)),
            Flow::Done
        );
        assert!(ctx.in_banishment(PREFECT));
        assert!(ctx.in_banishment(THEIR_GEAR));
        assert!(!ctx.on_board(THEIR_GEAR));
        assert!(ctx.on_board(MY_GEAR), "only the played gear");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {THEIR_GEAR}}} is banished with {{card {PREFECT}}}"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_prefect_off_the_board_cannot_pay_and_a_gear_already_gone_is_not_banished_twice() {
        let mut fixture = precinct();
        let mut ctx = fixture.ctx();
        ctx.trash(PREFECT);
        assert_eq!(
            confiscate(&mut ctx, &trigger(Some(THEIR_GEAR)), Stage(0)),
            Flow::Done
        );
        assert!(ctx.in_trash(PREFECT), "not banished from the trash");
        assert!(
            ctx.on_board(THEIR_GEAR),
            "the cost is unpaid, the gear stays"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {PREFECT}}} is no longer on the board · the cost is unpaid and {{card {THEIR_GEAR}}} stays"
        )));
        drop(ctx);
        let mut fixture = precinct();
        let mut ctx = fixture.ctx();
        ctx.trash(THEIR_GEAR);
        assert_eq!(
            confiscate(&mut ctx, &trigger(Some(THEIR_GEAR)), Stage(0)),
            Flow::Done
        );
        assert!(
            ctx.in_banishment(PREFECT),
            "383.3.b · the cost is paid even when the gear has left"
        );
        assert!(
            ctx.in_trash(THEIR_GEAR),
            "a gear in the trash is not banished"
        );
        assert_eq!(confiscate(&mut ctx, &trigger(None), Stage(0)), Flow::Done);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PREFECT}}} has no gear to banish")));
    }

    #[test]
    #[ignore = "engine gap · missing trigger subject and SelfCost::BanishSelf (the Zero Drive row): Trigger lacks OpponentPlaysGear and no SelfCost banishes the source at finalization; with them the script is optional(paying_with(triggered(OpponentPlaysGear, &[], confiscate), SelfCost::BanishSelf)) and the opponent's gear play offers the banish"]
    fn an_opponents_gear_play_offers_to_banish_him_and_the_gear_through_the_engine() {
        let mut fixture = precinct();
        fixture.table.card_mut(THEIR_GEAR).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.raise(played(THEIR_GEAR, 1, KIND_GEAR));
        crate::engine::settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::OptionalCost { .. })
        ));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.in_banishment(PREFECT), "paid at finalization");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.in_banishment(THEIR_GEAR));
    }
}
