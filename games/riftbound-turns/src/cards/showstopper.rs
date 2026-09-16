use super::prelude::{
    a_card, buff, card_target, charm_destination, done, move_unit, play, spell, target,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MOVABLE_FRIENDLY_UNIT_IN_BASE: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::InBase,
    Filter::Movable,
]);

pub const A_BATTLEFIELD_FOR_IT: TargetSpec = target(
    Filter::And(&[Filter::AtBattlefield, Filter::DifferentLocationFrom(0)]),
    1,
    1,
    TargetKind::Zone,
    "the battlefield it moves to",
);

const UNIT: usize = 0;
const DESTINATION: usize = 1;

fn show(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    if buff(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is buffed"));
    }
    if let Some(to) = charm_destination(ctx, item, unit, DESTINATION) {
        move_unit(ctx, item, unit, to);
    }
    done()
}

pub static CARD: Card = spell(
    "Showstopper",
    &[],
    &[play(
        &[
            a_card(
                MOVABLE_FRIENDLY_UNIT_IN_BASE,
                "a friendly unit in your base",
            ),
            A_BATTLEFIELD_FOR_IT,
        ],
        show,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Keyword, Trigger};
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::ctx::{EntryMove, Event, Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{PromptWhy, ShowdownStage, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const SHOW: u32 = 90;
    const THEIR_SHOW: u32 = 91;
    const SPARE: u32 = 92;
    const BODY_RUNE: u32 = 100;

    fn show_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Showstopper", 1, 1);
        card.domain = vec!["Body".into(), "Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(show_card(SHOW, 0));
        fixture.table.cards.push(show_card(THEIR_SHOW, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(SPARE, fixtures::BF1, 0, "Unsung Hero", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
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
        settle(ctx).unwrap();
    }

    fn buffed(ctx: &Ctx, unit: u32) -> i32 {
        ctx.table
            .counter(Target::Card(unit), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_plain_spell_over_a_friendly_unit_in_base_and_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Showstopper").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Showstopper");
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[UNIT].filter, MOVABLE_FRIENDLY_UNIT_IN_BASE);
        assert_eq!(ability.targets[DESTINATION], A_BATTLEFIELD_FOR_IT);
        assert_eq!(A_BATTLEFIELD_FOR_IT.kind, TargetKind::Zone);
    }

    #[test]
    fn the_unit_in_base_gets_a_buff_then_moves_to_the_chosen_battlefield() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHOW).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "only the friendly unit in the base, not the one already at a battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "every battlefield, never the base it stands in"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Zone(fixtures::BF1)
            ]
        );
        assert_eq!(buffed(&ctx, fixtures::VI), 0, "nothing before it resolves");
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(buffed(&ctx, fixtures::VI), 1);
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "a buff is +1 Might on the printed 3"
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::VI,
            from: Some(Location::Base(0)),
            to: Location::Battlefield(fixtures::BF1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert!(
            !ctx.card(fixtures::VI).unwrap().exhausted,
            "an effect move does not exhaust"
        );
        let buff_index = ctx
            .effects
            .iter()
            .position(|effect| {
                matches!(effect, Effect::Counter { target: Target::Card(card), counter, .. }
                    if *card == fixtures::VI && *counter == COUNTER_BUFFED)
            })
            .expect("the buff was stamped");
        let move_index = ctx
            .effects
            .iter()
            .position(|effect| {
                matches!(effect, Effect::Move { card, zone, .. }
                    if *card == fixtures::VI && *zone == fixtures::BF1)
            })
            .expect("the unit moved");
        assert!(buff_index < move_index, "buff first, then the move");
        assert!(ctx.blob.log.contains(&"{card 50} is buffed".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} moves to {zone 9}".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert!(
            ctx.blob.staged.is_empty() && ctx.blob.showdown.is_none(),
            "seat 0 already holds this one"
        );
        assert_eq!(ctx.card(SHOW).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn moving_onto_the_enemys_battlefield_contests_it_and_an_already_buffed_unit_keeps_one_buff() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(fixtures::VI));
        fixtures::play_from_hand(&mut ctx, 0, SHOW).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(buffed(&ctx, fixtures::VI), 1, "a buff does not stack");
        assert!(!ctx.blob.log.contains(&"{card 50} is buffed".to_string()));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.blob.contester(fixtures::BF2),
            Some(0),
            "428 · the destination is contested"
        );
        let showdown = ctx.blob.showdown.as_ref().expect("a combat opened");
        assert_eq!(showdown.zone, fixtures::BF2);
        assert!(showdown.combat);
        assert!(matches!(showdown.stage, ShowdownStage::Open));
        assert!(ctx.is_attacker(fixtures::VI));
        assert!(ctx.is_defender(fixtures::SPRITE));
    }

    #[test]
    fn a_unit_that_left_the_base_before_resolution_is_left_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHOW).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::BF2, 0), 0)
            .unwrap();
        both_pass(&mut ctx);
        assert_eq!(buffed(&ctx, fixtures::VI), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(ctx.card(SHOW).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn units_at_battlefields_enemies_and_the_base_are_refused_and_the_spell_is_the_turn_players() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SHOW)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, SHOW).unwrap();
        for wrong in [SPARE, fixtures::THEIR_UNIT, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit in the base"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[u32::from(fixtures::BASE)]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the base is no battlefield"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(SHOW).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(buffed(&ctx, fixtures::VI), 0);
        assert!(ctx.blob.is_neutral_open());
        drop(ctx);

        let mut locked = armed();
        let mut ctx = locked.ctx();
        ctx.lock_move(fixtures::VI);
        fixtures::play_from_hand(&mut ctx, 0, SHOW).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "a move-locked unit leaves its owner's move effect"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.is_neutral_open());
    }
}
