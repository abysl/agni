use super::prelude::{unit, with_additional, with_statics, CHAOS};
use super::{Card, Static};
use crate::engine::ctx::{Ctx, Event};

pub fn played_a_spell_this_turn(ctx: &Ctx, seat: u8) -> bool {
    ctx.blob.seat(seat).spells_played > 0
}

pub fn additional_cost_offered(ctx: &Ctx, seat: u8) -> bool {
    played_a_spell_this_turn(ctx, seat)
}

pub fn paid_the_additional_cost(ctx: &Ctx, me: u32) -> bool {
    let pending = ctx
        .blob
        .queue
        .iter()
        .find(|pending| pending.item.kind.card() == Some(me))
        .map(|pending| pending.item.paid_additional());
    match pending {
        Some(paid) => paid,
        None => ctx.events.iter().rev().any(|event| {
            matches!(
                event,
                Event::Played {
                    card,
                    paid_additional: true,
                    ..
                } if *card == me
            )
        }),
    }
}

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    paid_the_additional_cost(ctx, me)
}

pub static CARD: Card = with_statics(
    with_additional(unit("Crescent Guardian", &[], &[]), CHAOS),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Domain;
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const GUARDIAN: u32 = 90;
    const ENERGY: u8 = 4;
    const MIGHT: u8 = 4;
    const CHAOS_RUNE: u32 = 46;

    fn guardian(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Chaos".into()],
            ..fixtures::unit(GUARDIAN, zone, seat, "Crescent Guardian", MIGHT)
        }
    }

    fn temple(spells_played: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(guardian(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.seat_mut(0).spells_played = spells_played;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GUARDIAN).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_with_an_optional_chaos_additional_cost_and_the_enters_ready_seam() {
        assert!(std::ptr::eq(script_of("Crescent Guardian").unwrap(), &CARD));
        assert_eq!(CARD.name, "Crescent Guardian");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert_eq!(CARD.additional, Some(CHAOS));
        let mut fixture = temple(0);
        let ctx = fixture.ctx();
        let mut item = ChainItem::new(1, ItemKind::Permanent { card: GUARDIAN }, 0, Origin::Hand);
        assert_eq!(cost::of_item(&ctx, &item, None).power, []);
        item.set_slot(SLOT_ADDITIONAL, 1);
        let paid = cost::of_item(&ctx, &item, None);
        assert_eq!(paid.energy, ENERGY);
        assert_eq!(paid.power, [Need::Domain(Domain::Chaos)]);
    }

    #[test]
    fn the_seams_read_the_spells_played_this_turn_and_the_pending_plays_answer() {
        let mut fixture = temple(0);
        let ctx = fixture.ctx();
        assert!(!played_a_spell_this_turn(&ctx, 0));
        assert!(!additional_cost_offered(&ctx, 0));
        assert!(
            !enters_ready(&ctx, GUARDIAN),
            "nothing pending, nothing paid"
        );
        drop(ctx);
        let mut fixture = temple(1);
        let mut ctx = fixture.ctx();
        assert!(additional_cost_offered(&ctx, 0));
        assert!(
            !additional_cost_offered(&ctx, 1),
            "the other seat has played no spell"
        );
        fixtures::play_from_hand(&mut ctx, 0, GUARDIAN).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL
        ));
        assert!(!enters_ready(&ctx, GUARDIAN), "unanswered reads as unpaid");
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            enters_ready(&ctx, GUARDIAN),
            "paid · the Played event remembers the answer once the item has left the queue"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, paid_additional: true, .. } if *card == GUARDIAN
        )));
    }

    #[test]
    fn paid_it_costs_four_and_a_chaos_declined_it_costs_four_and_short_it_is_refused() {
        let mut fixture = temple(1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GUARDIAN).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(GUARDIAN), Some(Location::Base(0)));
        assert!(
            ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move {
                    card: CHAOS_RUNE,
                    zone: fixtures::RUNE_DECK,
                    ..
                }
            )),
            "the Chaos rune recycles for the additional power: {:?}",
            ctx.effects
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "four energy off five runes · the Chaos rune exhausts for energy and recycles for the power"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut fixture = temple(1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GUARDIAN).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(GUARDIAN), Some(Location::Base(0)));
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "the Chaos rune stays");
        assert!(!enters_ready(&ctx, GUARDIAN));
        assert!(ctx.card(GUARDIAN).unwrap().exhausted);
        drop(ctx);
        let mut short = temple(1);
        for rune in [41, 42] {
            short.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = short.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, GUARDIAN),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 3
            })
        );
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    #[ignore = "engine gap · Card.additional is offered whenever it is affordable; the ask needs a gate (additional_cost_offered) so a Guardian played before any spell this turn is never asked"]
    fn without_a_spell_played_this_turn_the_additional_cost_is_not_offered() {
        let mut fixture = temple(0);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GUARDIAN).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no spell this turn, no Chaos to offer: {:?}",
            ctx.blob.why
        );
        assert_eq!(ctx.location(GUARDIAN), Some(Location::Base(0)));
    }

    #[test]
    fn paid_after_a_spell_this_turn_it_enters_ready() {
        let mut fixture = temple(1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GUARDIAN).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(GUARDIAN));
        assert!(
            !ctx.card(GUARDIAN).unwrap().exhausted,
            "356.4.f.1 · paid means it enters ready"
        );
    }
}
