use super::prelude::{done, draw, empower, on_move, unit, when, when_empowered};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 3,
    power: &[],
};
pub const DRAWS: usize = 1;

fn inform(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = unit(
    "Covert Informant",
    &[Keyword::Empower(EMPOWER)],
    &[empower(EMPOWER), when(on_move(&[], inform), when_empowered)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger, Where, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, march, priority, settle};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const INFORMANT: u32 = 90;

    fn informant(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(INFORMANT, zone, seat, "Covert Informant", 4)
        }
    }

    fn safehouse(zone: u16, ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(informant(zone, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        let mut count = 0;
        for card in fixture.table.cards.iter_mut() {
            if card.is_kind("Rune") && card.owner == 0 {
                card.exhausted = count >= ready;
                count += 1;
            }
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(INFORMANT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn his_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == INFORMANT)
            .collect()
    }

    fn move_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 1 } if source == INFORMANT))
            .count()
    }

    fn walk(ctx: &mut Ctx, from: Location, to: Location) {
        march::standard_move(ctx, 0, INFORMANT, from, to);
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_prints_empower_and_a_move_trigger_gated_on_empowered() {
        assert!(std::ptr::eq(script_of("Covert Informant").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 3);
        assert!(EMPOWER.power.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(empower.usable.is_some());
        let moved = &CARD.abilities[1];
        assert_eq!(
            moved.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(moved.condition.is_some(), "[Empowered] gates the trigger");
        assert!(moved.targets.is_empty());
        assert!(!moved.optional);
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn empower_pays_three_and_afterwards_every_move_draws_one() {
        let mut fixture = safehouse(fixtures::BASE, 3);
        let action = fixtures::move_action(INFORMANT, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {INFORMANT}}}: empower (3 energy)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, INFORMANT, 0).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        assert!(
            !ctx.card(INFORMANT).unwrap().exhausted,
            "827.1 · Empower never exhausts him"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(INFORMANT));
        assert!(his_offers(&ctx).is_empty(), "377.2.b · the offer is gone");
        assert_eq!(
            activate::activate(&mut ctx, 0, INFORMANT, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        let hand = ctx.hand_of(0).len();
        walk(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(move_items(&ctx), 1, "the move trigger waits on the chain");
        assert_eq!(ctx.hand_of(0).len(), hand);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        march::effect_move(
            &mut ctx,
            &fixtures::effect_of(0),
            INFORMANT,
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(INFORMANT), Some(Location::Base(0)));
        assert_eq!(move_items(&ctx), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + 2 * DRAWS,
            "an effect's move home draws too · no once-a-turn on the card"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn unempowered_he_moves_without_drawing_and_two_ready_runes_cannot_empower_him() {
        let mut fixture = safehouse(fixtures::BASE, 2);
        let action = fixtures::move_action(INFORMANT, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].enabled, "greyed, not hidden");
        assert_eq!(
            activate::activate(&mut ctx, 0, INFORMANT, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 2
            })
        );
        assert!(!ctx.is_empowered(INFORMANT));
        let hand = ctx.hand_of(0).len();
        walk(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(
            ctx.location(INFORMANT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "727.1.c.1 · [Empowered] text is silent until he is empowered"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.fault.is_none());
    }
}
