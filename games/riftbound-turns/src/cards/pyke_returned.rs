use super::prelude::{
    at_battlefield, done, on_enemy_unit_dies, once_each_turn, spawn_gold, unit, when,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::{Ctx, Event};

const GOLD_ARRIVES_READY: bool = false;

fn at_a_battlefield(ctx: &Ctx, _: &Event, source: super::Source) -> bool {
    at_battlefield(ctx, source.card)
}

fn toll(ctx: &mut Ctx, item: &Item, _stage: Stage) -> Flow {
    spawn_gold(ctx, item.controller, GOLD_ARRIVES_READY);
    done()
}

pub static CARD: Card = unit(
    "Pyke - Returned",
    &[Keyword::Hidden, Keyword::Backline],
    &[once_each_turn(when(
        on_enemy_unit_dies(&[], toll),
        at_a_battlefield,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who, TOKEN_GOLD};
    use crate::engine::ctx::{Cause, Killed, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cleanup, combat, hide, phases, priority, settle};
    use crate::state::FLAG_ONCE_USED;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const PYKE: u32 = 90;
    const PREY: u32 = 91;
    const MORE_PREY: u32 = 92;
    const HOURGLASS: u32 = 93;
    const ALLY: u32 = 94;

    fn pyke(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Pyke - Returned", 3);
        card.domain = vec!["Chaos".into()];
        card.energy = Some(3);
        card
    }

    fn prey(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Jinx", 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn docked(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(pyke(PYKE, zone, 0));
        fixture.table.cards.push(prey(PREY, fixtures::BF1, 1));
        fixture.table.cards.push(prey(MORE_PREY, fixtures::BF1, 1));
        fixture.resolve();
        fixture
    }

    fn resolve_all(ctx: &mut Ctx) {
        for _ in 0..16 {
            let Some(seat) = priority::holder(ctx) else {
                break;
            };
            priority::pass(ctx, seat).unwrap();
        }
    }

    fn dies(ctx: &mut Ctx, card: u32) -> Killed {
        let killed = ctx.kill(card, Cause::Cleanup { last_item: None });
        settle(ctx).unwrap();
        resolve_all(ctx);
        killed
    }

    fn gold_ids(ctx: &Ctx) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == 0)
            .map(|card| card.id)
            .collect()
    }

    fn golds(ctx: &Ctx) -> usize {
        gold_ids(ctx).len()
    }

    #[test]
    fn pyke_is_hidden_and_backline_and_tolls_once_a_turn_from_a_battlefield() {
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Backline));
        assert_eq!(CARD.abilities[0].trigger, Trigger::UnitDies(Who::Enemy));
        assert_eq!(CARD.abilities[0].once, crate::cards::Once::PerTurn);

        let mut fixture = docked(fixtures::BF2);
        let mut ctx = fixture.ctx();
        dies(&mut ctx, PREY);
        assert_eq!(golds(&ctx), 1, "a battlefield toll");
        assert!(ctx.has_flag(PYKE, FLAG_ONCE_USED));
        dies(&mut ctx, MORE_PREY);
        assert_eq!(golds(&ctx), 1, "once each turn");
        drop(ctx);

        let mut home = docked(fixtures::BASE);
        let mut ctx = home.ctx();
        dies(&mut ctx, PREY);
        assert_eq!(golds(&ctx), 0, "the toll is collected at a battlefield");
    }

    #[test]
    fn the_toll_is_a_gold_gear_token_played_exhausted_into_its_owners_base() {
        let mut fixture = docked(fixtures::BF2);
        let mut ctx = fixture.ctx();
        dies(&mut ctx, PREY);
        let paid = gold_ids(&ctx);
        assert_eq!(paid.len(), 1);
        let gold = paid[0];
        assert!(ctx.is_token(gold), "a token, not a card from anywhere");
        assert!(ctx.is_gear(gold));
        assert!(!ctx.is_unit(gold));
        assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        assert!(
            ctx.card(gold).unwrap().exhausted,
            "played exhausted, so its rainbow waits for a later turn"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} gains a Gold"));
    }

    #[test]
    fn a_friendly_death_is_no_toll_and_leaves_the_once_mark_unspent() {
        let mut fixture = docked(fixtures::BF2);
        let mut ctx = fixture.ctx();
        dies(&mut ctx, fixtures::VI);
        assert_eq!(golds(&ctx), 0, "an enemy unit, never a friendly one");
        assert!(
            !ctx.has_flag(PYKE, FLAG_ONCE_USED),
            "a trigger that never matched spends nothing"
        );
        dies(&mut ctx, PREY);
        assert_eq!(golds(&ctx), 1);
    }

    #[test]
    fn a_death_a_replacement_turned_aside_pays_no_toll() {
        let mut fixture = docked(fixtures::BF2);
        fixture.table.cards.push(fixtures::gear(
            HOURGLASS,
            fixtures::BASE,
            1,
            "Zhonya's Hourglass",
            2,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.kill(PREY, Cause::Cleanup { last_item: None }),
            Killed::Replaced,
            "the enemy gear kills itself and recalls the unit instead"
        );
        settle(&mut ctx).unwrap();
        resolve_all(&mut ctx);
        assert_eq!(
            golds(&ctx),
            0,
            "no death happened, so Pyke counts nothing for it"
        );
        assert!(
            !ctx.has_flag(PYKE, FLAG_ONCE_USED),
            "and the toll is still owed this turn"
        );
        assert_eq!(
            ctx.card(HOURGLASS).unwrap().zone,
            Some(fixtures::TRASH),
            "the gear itself died, but a gear is not a unit"
        );
        dies(&mut ctx, MORE_PREY);
        assert_eq!(golds(&ctx), 1, "the next real death is paid for");
    }

    #[test]
    fn the_toll_comes_back_the_next_turn() {
        let mut fixture = docked(fixtures::BF2);
        let mut ctx = fixture.ctx();
        dies(&mut ctx, PREY);
        assert_eq!(golds(&ctx), 1);
        phases::end_turn(&mut ctx).unwrap();
        assert!(
            !ctx.has_flag(PYKE, FLAG_ONCE_USED),
            "the once-each-turn mark expires with the turn"
        );
        dies(&mut ctx, MORE_PREY);
        assert_eq!(golds(&ctx), 2, "a fresh turn, a fresh toll");
    }

    #[test]
    fn backline_keeps_pyke_out_of_the_damage_order_until_nothing_else_remains() {
        let mut fixture = docked(fixtures::BF1);
        fixture.table.cards.push(prey(ALLY, fixtures::BF1, 0));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(PYKE, Keyword::Backline));
        assert!(!ctx.has_keyword(PYKE, Keyword::Tank));
        assert_eq!(
            combat::ordered(&ctx, &[ALLY, PYKE], false),
            [ALLY],
            "he is assigned combat damage last"
        );
        assert_eq!(
            combat::ordered(&ctx, &[PYKE], false),
            [PYKE],
            "and takes it once he is the only one left"
        );
    }

    #[test]
    fn a_facedown_pyke_collects_no_toll() {
        let mut fixture = docked(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        hide::hide(&mut ctx, 0, PYKE, fixtures::BF1).unwrap();
        assert!(hide::is_facedown(&ctx, PYKE));
        dies(&mut ctx, PREY);
        assert_eq!(
            golds(&ctx),
            0,
            "a facedown card is no unit at a battlefield"
        );
        assert!(!ctx.has_flag(PYKE, FLAG_ONCE_USED));
    }

    #[test]
    fn hiding_pyke_is_refused_off_a_held_battlefield_and_where_one_card_is_already_hidden() {
        let mut fixture = docked(fixtures::BF2);
        fixture.table.card_mut(PYKE).unwrap().zone = Some(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            hide::legal(&ctx, 0, PYKE, fixtures::BF2),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "seat 1 holds that battlefield"
        );
        ctx.blob.set_holder(fixtures::BF1, Some(0));
        hide::hide(&mut ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        assert_eq!(
            hide::legal(&ctx, 0, PYKE, fixtures::BF1),
            Err(Refusal::Illegal(Reason::OneFacedown)),
            "106.4.b · one facedown card per battlefield"
        );
        assert_eq!(
            hide::play_legal(&ctx, 0, fixtures::HAND_HIDDEN),
            Err(Refusal::Illegal(Reason::HiddenThisTurn)),
            "737.1.c · a card hidden this turn reacts from the next turn on"
        );
    }

    #[test]
    fn pyke_can_be_hidden_at_a_battlefield_his_seat_holds() {
        let mut fixture = docked(fixtures::BF2);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(PYKE).unwrap().zone = Some(fixtures::HAND);
        fixture.table.card_mut(PYKE).unwrap().seat = 0;
        let mut ctx = fixture.ctx();
        assert_eq!(hide::legal(&ctx, 0, PYKE, fixtures::BF1), Ok(()));
        hide::hide(&mut ctx, 0, PYKE, fixtures::BF1).unwrap();
        assert_eq!(hide::zone_of(&ctx, PYKE), Some(fixtures::BF1));
        cleanup::run(&mut ctx, None);
        assert_eq!(
            hide::zone_of(&ctx, PYKE),
            Some(fixtures::BF1),
            "he stays while his seat holds the battlefield"
        );
    }
}
