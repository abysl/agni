use super::prelude::{done, draw_revealed, forget_revealing, might_this_turn, on_move, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::FLAG_REVEALING;

pub const REVEALED: u8 = 1;
pub const MIGHT: i16 = 2;

fn revealing(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .find(|card| {
            ctx.owner(*card) == seat
                && ctx
                    .card(*card)
                    .is_some_and(|held| held.zone == ctx.zones.chain)
        })
}

fn sort(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let me = item.kind.source();
    let Some(card) = revealing(ctx, seat) else {
        return done();
    };
    forget_revealing(ctx, card);
    if ctx.is_unit(card) {
        draw_revealed(ctx, seat, card);
        return done();
    }
    ctx.trash(card);
    ctx.narrate(format!(
        "{{seat {seat}}} puts {{card {card}}} into their trash"
    ));
    if ctx.on_board(me) {
        might_this_turn(ctx, item, me, MIGHT, None);
        ctx.narrate(format!("{{card {me}}} gets +{MIGHT} Might this turn"));
    }
    done()
}

fn protect(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == REVEALED {
        return sort(ctx, item);
    }
    let seat = item.controller;
    let Some(top) = ctx.reveal_top(seat) else {
        ctx.narrate(format!("{{seat {seat}}} has no card to reveal"));
        return done();
    };
    Flow::Ask(ctx.await_faces(item, &[top], REVEALED))
}

pub static CARD: Card = unit("Pakaa Protector", &[], &[on_move(&[], protect)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, Where, Who, KIND_GEAR, KIND_SPELL, KIND_UNIT};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, march, settle};
    use crate::state::{ItemKind, ItemStatus};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::Face;

    const PAKAA: u32 = 90;
    const TOP_OF_DECK: u32 = 23;
    const NEXT: u32 = 22;

    fn den(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut pakaa = fixtures::unit(PAKAA, zone, 0, "Pakaa Protector", 4);
        pakaa.domain = vec!["Calm".into()];
        pakaa.energy = Some(5);
        fixture.table.cards.push(pakaa);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PAKAA).unwrap(), &CARD));
        fixture
    }

    fn march(fixture: &mut Fixture, from: Location, to: Location) {
        let zone = to.battlefield().unwrap_or(fixtures::BASE);
        let action = fixtures::move_action(PAKAA, zone, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, PAKAA, from, to);
        settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PAKAA
        ));
        pass_until_parked(&mut ctx);
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [TOP_OF_DECK]);
        assert_eq!(ctx.card(TOP_OF_DECK).unwrap().zone, Some(fixtures::CHAIN));
        assert!(ctx.has_flag(TOP_OF_DECK, FLAG_REVEALING));
        assert!(ctx.blob.prompt.is_none(), "the host's reveal is awaited");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals the top card of their deck".to_string()));
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, face: Face) {
        let action = Action::Reveal {
            card: TOP_OF_DECK,
            face,
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, TOP_OF_DECK).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn trash_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::TRASH, seat)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_script_is_a_unit_whose_only_ability_triggers_on_any_move_of_itself() {
        assert!(std::ptr::eq(script_of("Pakaa Protector").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(MIGHT, 2);
    }

    #[test]
    fn a_unit_on_top_is_drawn_once_the_host_reveals_it_and_he_gains_nothing() {
        let mut fixture = den(fixtures::BASE);
        let hand = fixture.ctx().hand_of(0).len();
        march(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        arrives(&mut fixture, Face::named("Jinx").with_kind(KIND_UNIT));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty(), "the trigger finished");
        assert_eq!(ctx.card(TOP_OF_DECK).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.blob.seat(0).draws, 1, "drawn, not merely put into hand");
        assert!(!ctx.has_flag(TOP_OF_DECK, FLAG_REVEALING));
        assert!(trash_of(&ctx, 0).is_empty());
        assert_eq!(ctx.current_might(PAKAA), 4, "no Might for a drawn unit");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} draws {{card {TOP_OF_DECK}}}")));
        assert_eq!(deck_of(&ctx, 0), [20, 21, NEXT]);
    }

    #[test]
    fn a_spell_goes_to_the_trash_and_he_gets_two_might_this_turn() {
        let mut fixture = den(fixtures::BASE);
        let hand = fixture.ctx().hand_of(0).len();
        march(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        arrives(&mut fixture, Face::named("Spark").with_kind(KIND_SPELL));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(TOP_OF_DECK).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(trash_of(&ctx, 0), [TOP_OF_DECK]);
        assert_eq!(deck_of(&ctx, 0), [20, 21, NEXT], "not recycled");
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.blob.seat(0).draws, 0);
        assert_eq!(ctx.current_might(PAKAA), 4 + i32::from(MIGHT));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} puts {{card {TOP_OF_DECK}}} into their trash"
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PAKAA}}} gets +2 Might this turn")));
        let until = crate::cards::prelude::this_turn(&ctx);
        let mut ctx = ctx;
        ctx.expire(until);
        assert_eq!(ctx.current_might(PAKAA), 4, "gone at the end of the turn");
    }

    #[test]
    fn a_gear_on_the_walk_home_is_trashed_too_and_an_empty_deck_reveals_nothing() {
        let mut fixture = den(fixtures::BF1);
        march(
            &mut fixture,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        arrives(
            &mut fixture,
            Face::named("Boots of Swiftness").with_kind(KIND_GEAR),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(trash_of(&ctx, 0), [TOP_OF_DECK], "a gear is not a unit");
        assert_eq!(ctx.current_might(PAKAA), 4 + i32::from(MIGHT));
        drop(ctx);

        let mut fixture = den(fixtures::BASE);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        fixture.resolve();
        let action = fixtures::move_action(PAKAA, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            PAKAA,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card to reveal".to_string()));
        assert_eq!(
            ctx.current_might(PAKAA),
            4,
            "nothing revealed, nothing gained"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_recall_is_not_a_move() {
        let mut fixture = den(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.recall(PAKAA, true);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(PAKAA), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "434.1 · a recall is not a move");
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23]);
    }
}
