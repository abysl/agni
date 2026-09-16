use super::prelude::{done, draw_revealed, forget_revealing, on_move, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::FLAG_REVEALING;

pub const REVEALED: u8 = 1;

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
    let Some(card) = revealing(ctx, seat) else {
        return done();
    };
    forget_revealing(ctx, card);
    if ctx.is_gear(card) {
        draw_revealed(ctx, seat, card);
    } else {
        ctx.recycle_to_bottom(card);
        ctx.narrate(format!("{{seat {seat}}} recycles {{card {card}}}"));
    }
    done()
}

fn appraise(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
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

pub static CARD: Card = unit("Apprentice Smith", &[], &[on_move(&[], appraise)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, Where, Who, KIND_GEAR, KIND_UNIT};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, march, settle};
    use crate::state::{ItemKind, ItemStatus};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::Face;

    const SMITH: u32 = 90;
    const TOP_OF_DECK: u32 = 23;
    const NEXT: u32 = 22;

    fn forge(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut smith = fixtures::unit(SMITH, zone, 0, "Apprentice Smith", 2);
        smith.domain = vec!["Calm".into()];
        smith.energy = Some(2);
        fixture.table.cards.push(smith);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SMITH).unwrap(), &CARD));
        fixture
    }

    fn march(fixture: &mut Fixture, from: Location, to: Location) {
        let zone = to.battlefield().unwrap_or(fixtures::BASE);
        let action = fixtures::move_action(SMITH, zone, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, SMITH, from, to);
        settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SMITH
        ));
        pass_until_parked(&mut ctx);
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [TOP_OF_DECK]);
        assert_eq!(
            ctx.card(TOP_OF_DECK).unwrap().zone,
            Some(fixtures::CHAIN),
            "the revealed card sits in the public zone for one entry"
        );
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

    #[test]
    fn the_script_is_a_unit_whose_only_ability_triggers_on_any_move_of_itself() {
        assert!(std::ptr::eq(script_of("Apprentice Smith").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
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
    }

    #[test]
    fn a_gear_on_top_is_drawn_once_the_host_reveals_it() {
        let mut fixture = forge(fixtures::BASE);
        let hand = fixture.ctx().hand_of(0).len();
        march(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        arrives(
            &mut fixture,
            Face::named("Boots of Swiftness").with_kind(KIND_GEAR),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty(), "the trigger finished");
        assert_eq!(ctx.card(TOP_OF_DECK).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(TOP_OF_DECK).unwrap().seat, 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.blob.seat(0).draws, 1, "drawn, not merely put into hand");
        assert!(!ctx.has_flag(TOP_OF_DECK, FLAG_REVEALING));
        assert!(ctx.state_of(TOP_OF_DECK).is_none(), "no state row lingers");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} draws {{card {TOP_OF_DECK}}}")));
        assert_eq!(deck_of(&ctx, 0), [20, 21, NEXT], "bottom to top");
    }

    #[test]
    fn anything_else_is_recycled_and_the_walk_home_reveals_too() {
        let mut fixture = forge(fixtures::BF1);
        let hand = fixture.ctx().hand_of(0).len();
        march(
            &mut fixture,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        arrives(&mut fixture, Face::named("Jinx").with_kind(KIND_UNIT));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(TOP_OF_DECK).unwrap().zone,
            Some(fixtures::MAIN_DECK)
        );
        assert_eq!(
            deck_of(&ctx, 0),
            [TOP_OF_DECK, 20, 21, NEXT],
            "recycled under the deck"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.blob.seat(0).draws, 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} recycles {{card {TOP_OF_DECK}}}")));
    }

    #[test]
    fn an_empty_deck_reveals_nothing_and_a_recall_is_not_a_move() {
        let mut fixture = forge(fixtures::BASE);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        fixture.resolve();
        let action = fixtures::move_action(SMITH, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            SMITH,
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
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = forge(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.recall(SMITH, true);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(SMITH), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "434.1 · a recall is not a move");
    }
}
