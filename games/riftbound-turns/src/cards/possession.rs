use super::prelude::{a_card, card_target, done, play, recall, spell};
use super::{Card, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const ENEMY_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::AtBattlefield]);

fn possess(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if ctx.set_controller(unit, item.controller, unit) {
        recall(ctx, unit, false);
        ctx.narrate(format!("{{card {unit}}} is recalled"));
    }
    done()
}

pub static CARD: Card = spell(
    "Possession",
    &[Keyword::Action],
    &[play(
        &[a_card(
            ENEMY_UNIT_AT_A_BATTLEFIELD,
            "an enemy unit at a battlefield",
        )],
        possess,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{Cause, EntryMove, Event, Killed, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{cleanup, phases, play as play_engine, priority, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const POSSESSION: u32 = 90;
    const THEIR_POSSESSION: u32 = 91;
    const CHAOS: [u32; 5] = [100, 101, 102, 103, 104];

    fn possession(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Possession", 8, 3);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(possession(POSSESSION, 0));
        fixture.table.cards.push(possession(THEIR_POSSESSION, 1));
        for rune in CHAOS {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let jinx = fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap();
        jinx.zone = Some(fixtures::BF1);
        jinx.exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(1));
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
    fn the_script_is_an_action_over_one_enemy_unit_at_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Possession").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Possession");
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, ENEMY_UNIT_AT_A_BATTLEFIELD);
    }

    #[test]
    fn the_caster_takes_control_and_the_unit_is_recalled_to_the_casters_base_as_it_stands() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, POSSESSION).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "the enemy units at battlefields only"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            ctx.controller(fixtures::THEIR_UNIT),
            1,
            "nothing before it resolves"
        );
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 0);
        assert_eq!(
            ctx.owner(fixtures::THEIR_UNIT),
            1,
            "ownership never changes"
        );
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(0)));
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::THEIR_UNIT,
            zone: fixtures::BASE,
            seat: 0,
            index: TOP
        }));
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Moved { .. })),
            "456 · a recall is not a move"
        );
        assert!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "458 · a recall leaves its state alone"
        );
        assert_eq!(
            ctx.state_of(fixtures::THEIR_UNIT).unwrap().control_source,
            Some(fixtures::THEIR_UNIT),
            "the unit is its own control source: control lasts as long as it stays on the board"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} takes control of {card 81}".to_string()));
        assert!(ctx.blob.log.contains(&"{card 81} is recalled".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "seat 1 no longer stands there"
        );
        assert_eq!(ctx.card(POSSESSION).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn control_survives_cleanups_and_the_possessed_unit_readies_on_its_new_controllers_awaken() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, POSSESSION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        both_pass(&mut ctx);
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.controller(fixtures::THEIR_UNIT),
            0,
            "the spell in the trash is not what holds the control"
        );
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(0)));
        assert!(!ctx.blob.log.iter().any(|line| line.ends_with("control")));
        phases::start_turn(&mut ctx);
        assert!(
            !ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "it awakens with seat 0's permanents"
        );
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 0);
    }

    #[test]
    fn a_target_that_left_the_battlefield_before_resolution_is_left_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, POSSESSION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let base = ctx.zones.base.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::THEIR_UNIT, base, 1), 1)
            .unwrap();
        both_pass(&mut ctx);
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 1);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert!(ctx
            .state_of(fixtures::THEIR_UNIT)
            .is_none_or(|row| row.controlled_by.is_none()));
        assert_eq!(ctx.card(POSSESSION).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_possessed_unit_that_dies_goes_to_its_owners_trash() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, POSSESSION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(ctx.kill(fixtures::THEIR_UNIT, Cause::Rule), Killed::Yes);
        let jinx = ctx.card(fixtures::THEIR_UNIT).unwrap();
        assert_eq!(jinx.zone, Some(fixtures::TRASH));
        assert_eq!(
            jinx.seat, 1,
            "056.2 · its owner's trash, not its controller's"
        );
    }

    #[test]
    fn friendly_units_and_enemies_in_their_base_are_refused_and_the_action_waits_for_your_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_POSSESSION)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, POSSESSION).unwrap();
        for wrong in [fixtures::VI, fixtures::GROUNDS, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit at a battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(POSSESSION).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut home = armed();
        home.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BASE);
        home.resolve();
        let mut ctx = home.ctx();
        fixtures::play_from_hand(&mut ctx, 0, POSSESSION).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "cancel"],
            "an enemy unit in its base is out of reach"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.is_neutral_open());
    }
}
