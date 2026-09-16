use super::prelude::{
    a_card, card_target, charm_destination, done, move_unit, play, spell, CHARM_DESTINATION,
};
use super::{Card, Cost, Domain, Filter, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT_LIMIT: u8 = 3;
pub const FLOW: Cost = Cost {
    energy: 4,
    power: &[Power::Domain(Domain::Chaos)],
};
pub const SMALL_MOVABLE_UNIT: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Movable,
    Filter::MightAtMost(MIGHT_LIMIT),
]);

const UNIT: usize = 0;
const DESTINATION: usize = 1;

fn step(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    if let Some(to) = charm_destination(ctx, item, unit, DESTINATION) {
        move_unit(ctx, item, unit, to);
    }
    done()
}

pub static CARD: Card = spell(
    "Twilight Step",
    &[Keyword::Flow(FLOW)],
    &[play(
        &[
            a_card(SMALL_MOVABLE_UNIT, "a unit with 3 Might or less to move"),
            CHARM_DESTINATION,
        ],
        step,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{might_this_turn, Location};
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{Leave, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const STEP: u32 = 90;
    const THEIR_STEP: u32 = 91;
    const BRUTE: u32 = 92;
    const CHAOS_RUNE: u32 = 100;
    const SECOND_CHAOS: u32 = 101;

    fn step_card(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Twilight Step", 2, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn dusk(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(step_card(STEP, zone, 0));
        fixture
            .table
            .cards
            .push(step_card(THEIR_STEP, fixtures::HAND, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        for rune in [CHAOS_RUNE, SECOND_CHAOS] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
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

    #[test]
    fn the_script_is_a_flow_sorcery_over_a_small_unit_and_where_it_goes() {
        assert!(std::ptr::eq(script_of("Twilight Step").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[UNIT].filter, SMALL_MOVABLE_UNIT);
        assert_eq!(ability.targets[UNIT].kind, TargetKind::Card);
        assert_eq!(ability.targets[DESTINATION], CHARM_DESTINATION);
        assert_eq!(MIGHT_LIMIT, 3);
    }

    #[test]
    fn a_unit_of_three_might_or_less_is_moved_where_its_controller_chooses() {
        let mut fixture = dusk(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STEP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "Vi, the Sprite and Jinx are 3 or less; the 4-Might Brute is not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "both battlefields; its own base is where it stands"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Zone(fixtures::BF1)
            ]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::THEIR_UNIT,
            from: Some(Location::Base(1)),
            to: Location::Battlefield(fixtures::BF1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} moves to {zone 9}".to_string()));
        assert_eq!(ctx.card(STEP).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_pump_in_response_above_three_leaves_the_unit_where_it_is() {
        let mut fixture = dusk(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STEP).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, fixtures::VI, 1, None);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "355.11.b · the target is judged again as the spell resolves"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert_eq!(ctx.card(STEP).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn from_the_trash_the_flow_play_moves_the_unit_and_is_banished() {
        let mut fixture = dusk(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            STEP,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.banished_of(0), [STEP]);
        assert!(ctx.trash_of(0).is_empty());
    }

    #[test]
    fn a_big_unit_is_refused_and_the_other_seat_waits_for_its_turn() {
        let mut fixture = dusk(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STEP)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, STEP).unwrap();
        for wrong in [BRUTE, fixtures::GROUNDS, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is too big or not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[u32::from(fixtures::BASE)]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "its own base is where it already stands"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(STEP).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
