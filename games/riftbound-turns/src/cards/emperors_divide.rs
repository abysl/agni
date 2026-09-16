use super::prelude::{card_targets, done, move_unit, play, spell, target, Location, Moved};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ANY_NUMBER: u8 = u8::MAX;

pub const FRIENDLY_UNITS_AT_ONE_BATTLEFIELD: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::Friendly,
        Filter::AtBattlefield,
        Filter::MovableToBase,
        Filter::SameLocationAsPicks,
    ]),
    0,
    ANY_NUMBER,
    TargetKind::Card,
    "any number of friendly units at one battlefield to move to their base",
);

pub fn send_home(ctx: &mut Ctx, item: &Item, unit: u32) -> bool {
    let home = Location::Base(ctx.controller(unit));
    move_unit(ctx, item, unit, home) == Some(Moved::Moved)
}

fn divide(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let units = card_targets(ctx, item);
    if units.is_empty() {
        ctx.narrate(format!("{{card {}}} moves no unit", item.kind.source()));
        return done();
    }
    for unit in units {
        send_home(ctx, item, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Emperor's Divide",
    &[Keyword::Hidden, Keyword::Action],
    &[play(&[FRIENDLY_UNITS_AT_ONE_BATTLEFIELD], divide)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const DIVIDE: u32 = 90;
    const ALLY: u32 = 91;
    const FAR_ALLY: u32 = 92;
    const FAR_FIELD: u32 = 93;

    fn court() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Calm".into()],
            ..fixtures::spell(DIVIDE, fixtures::HAND, 0, "Emperor's Divide", 2, 0)
        });
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(FAR_ALLY, fixtures::BF3, 0, "Far Ally", 2));
        fixture.table.cards.push(fixtures::card(
            FAR_FIELD,
            fixtures::BF3,
            0,
            "Far Field",
            "Battlefield",
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_holder(fixtures::BF3, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DIVIDE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn moves(ctx: &Ctx) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Moved { card, .. } => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_hidden_action_over_any_number_of_friendly_units_at_one_battlefield() {
        assert!(std::ptr::eq(script_of("Emperor's Divide").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, [FRIENDLY_UNITS_AT_ONE_BATTLEFIELD]);
        assert_eq!(
            (ability.targets[0].min, ability.targets[0].max),
            (0, ANY_NUMBER)
        );
    }

    #[test]
    fn the_chosen_units_at_one_battlefield_move_to_their_base_and_the_holder_is_lost() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DIVIDE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 50}",
                "{card 91}",
                "{card 92}",
                "done",
                "skip",
                "cancel"
            ],
            "friendly units at battlefields · the Sprite and Jinx are not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 91}", "done", "cancel"],
            "the second pick must share Vi's battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(ALLY)]
        );
        assert!(moves(&ctx).is_empty(), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(ctx.location(ALLY), Some(Location::Base(0)));
        assert_eq!(
            ctx.location(FAR_ALLY),
            Some(Location::Battlefield(fixtures::BF3)),
            "the unit elsewhere stays"
        );
        assert_eq!(
            moves(&ctx),
            [fixtures::VI, ALLY],
            "456 · an effect move is a move"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, cause: MoveCause::Effect, .. } if *card == fixtures::VI
        )));
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "190.4.c · no unit left there in an open turn"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF3), Some(0));
        assert_eq!(ctx.card(DIVIDE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn choosing_nothing_is_allowed_and_moves_no_unit() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DIVIDE).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, []);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(moves(&ctx).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} moves no unit".to_string()));
        assert_eq!(ctx.card(DIVIDE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_unit_that_left_its_battlefield_before_resolution_is_skipped() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DIVIDE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        ctx.bounce(ALLY);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.in_hand(ALLY));
        assert_eq!(moves(&ctx), [fixtures::VI]);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn enemy_units_and_units_at_another_battlefield_are_refused() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DIVIDE).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI, FAR_ALLY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "one battlefield only"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(DIVIDE).unwrap().zone, Some(fixtures::HAND));
    }
}
