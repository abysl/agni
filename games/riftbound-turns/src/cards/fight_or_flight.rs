use super::prelude::{a_card, card_target, done, move_unit, play, spell, Location};
use super::{Card, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const UNIT_AT_A_BATTLEFIELD_THAT_MAY_GO_HOME: Filter =
    Filter::And(&[Filter::Unit, Filter::MovableToBase, Filter::Movable]);

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        let home = Location::Base(ctx.controller(unit));
        move_unit(ctx, item, unit, home);
    }
    done()
}

pub static CARD: Card = spell(
    "Fight or Flight",
    &[Keyword::Hidden, Keyword::Action],
    &[play(
        &[a_card(
            UNIT_AT_A_BATTLEFIELD_THAT_MAY_GO_HOME,
            "a unit at a battlefield",
        )],
        resolve,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{battlefield, with_statics};
    use crate::cards::{Static, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const FLIGHT: u32 = 90;
    const THEIR_FLIGHT: u32 = 91;
    const SPARE: u32 = 55;

    static NO_RETREAT: Card = with_statics(
        battlefield("Rockfall Path", &[], &[]),
        &[Static::NoMoveToBase],
    );

    fn flight(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Fight or Flight", 2, 0);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(flight(FLIGHT, 0));
        fixture.table.cards.push(flight(THEIR_FLIGHT, 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
            .table
            .cards
            .push(fixtures::unit(SPARE, fixtures::BF1, 0, "Unsung Hero", 2));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
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

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_hidden_action_over_one_unit_at_a_battlefield_that_may_retreat() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Fight or Flight").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Fight or Flight");
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(
            ability.targets[0].filter,
            UNIT_AT_A_BATTLEFIELD_THAT_MAY_GO_HOME
        );
    }

    #[test]
    fn an_enemy_unit_at_a_battlefield_is_moved_to_its_own_base() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLIGHT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 55}", "cancel"],
            "either side's units at battlefields, never a unit already in a base"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1)),
            "nothing moves before it resolves"
        );
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::THEIR_UNIT,
            zone: fixtures::BASE,
            seat: 1,
            index: TOP
        }));
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::THEIR_UNIT,
            from: Some(Location::Battlefield(fixtures::BF1)),
            to: Location::Base(1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert!(
            !ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "an effect move does not exhaust"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} moves to their base".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "seat 0 keeps the battlefield its unit still stands on"
        );
        assert_eq!(ctx.card(FLIGHT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn your_own_unit_retreats_home_and_the_battlefield_it_held_alone_falls_open() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLIGHT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 55}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(ctx.location(SPARE), Some(Location::Base(0)));
        assert_eq!(ctx.blob.holder(fixtures::BF1), None);
        assert!(ctx.blob.staged.is_empty(), "a retreat contests nothing");
    }

    #[test]
    fn played_from_facedown_it_is_free_and_only_reaches_the_hiding_battlefield() {
        let mut fixture = armed();
        fixture.table.card_mut(FLIGHT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(FLIGHT).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(FLIGHT, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            FLIGHT,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "{card 55}", "cancel"],
            "737.1.d · the Sprite at the other battlefield is out of reach"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Annotate { key, .. } if key == "exhausted"
            )),
            "no energy is paid from facedown"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert_eq!(ctx.card(FLIGHT).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_unit_that_already_went_home_is_left_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLIGHT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let base = ctx.zones.base.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::THEIR_UNIT, base, 1), 1)
            .unwrap();
        both_pass(&mut ctx);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert_eq!(ctx.card(FLIGHT).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn units_in_a_base_or_where_retreat_is_barred_are_refused_and_the_action_waits_for_your_turn() {
        let mut fixture = armed();
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::ROCKFALL, &NO_RETREAT);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_FLIGHT)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, FLIGHT).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "{card 55}", "cancel"],
            "the Sprite stands where no unit may move to base"
        );
        for wrong in [fixtures::VI, fixtures::SPRITE, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} cannot be sent home"
            );
        }
        ctx.lock_move(SPARE);
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "cancel"],
            "a move-locked friendly unit leaves its owner's move effect"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(FLIGHT).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
