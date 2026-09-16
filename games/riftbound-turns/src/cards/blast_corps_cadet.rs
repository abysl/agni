use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{
    a_unit_at_a_battlefield, card_target, deal, done, play, unit, when, with_additional,
};
use super::{Card, Cost, Domain, Flow, Item, Power, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ADDITIONAL: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Fury)],
};
pub const DAMAGE: u8 = 2;

pub const BLAST_TARGET: TargetSpec =
    a_unit_at_a_battlefield("a unit at a battlefield to deal 2 to");

fn blast(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        deal(ctx, item, unit, DAMAGE);
    }
    done()
}

pub static CARD: Card = with_additional(
    unit(
        "Blast Corps Cadet",
        &[],
        &[when(play(&[BLAST_TARGET], blast), paid_additional_on_entry)],
    ),
    ADDITIONAL,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::{Cause, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const CADET: u32 = 90;
    const THEIR_BRUTE: u32 = 91;

    fn cadet(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            ..fixtures::unit(CADET, zone, 0, "Blast Corps Cadet", 2)
        }
    }

    fn range(fury_runes: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(cadet(fixtures::HAND));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 3));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        if !fury_runes {
            for rune in [40, 41, 43] {
                let held = fixture.table.card_mut(rune).unwrap();
                held.domain = vec!["Calm".into()];
                held.name = "Calm Rune".into();
            }
        }
        fixture.resolve();
        fixture
    }

    fn additional_confirm(ctx: &Ctx) -> bool {
        matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL
        )
    }

    fn pending(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the play trigger is pending")
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_an_optional_energy_and_fury_cost_and_a_targeted_trigger_gated_on_paying_it(
    ) {
        assert!(std::ptr::eq(script_of("Blast Corps Cadet").unwrap(), &CARD));
        assert_eq!(CARD.name, "Blast Corps Cadet");
        assert_eq!(CARD.additional, Some(ADDITIONAL));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional, "the may is the cost, not the blast");
        assert!(ability.condition.is_some());
        assert_eq!(ability.targets, &[BLAST_TARGET]);
        assert_eq!((BLAST_TARGET.min, BLAST_TARGET.max), (1, 1));
        assert_eq!(BLAST_TARGET.kind, TargetKind::Card);
        assert_eq!(BLAST_TARGET.filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(DAMAGE, 2);
    }

    #[test]
    fn the_additional_cost_adds_one_energy_and_a_fury_need_only_when_the_slot_says_paid() {
        let mut fixture = range(true);
        let ctx = fixture.ctx();
        let mut held = ChainItem::new(1, ItemKind::Permanent { card: CADET }, 0, Origin::Hand);
        let plain = cost::of_item(&ctx, &held, None);
        assert_eq!(plain.energy, 2);
        assert!(plain.power.is_empty());
        held.set_slot(SLOT_ADDITIONAL, 1);
        let paid = cost::of_item(&ctx, &held, None);
        assert_eq!(paid.energy, 3);
        assert_eq!(paid.power, [Need::Domain(Domain::Fury)]);
        assert!(held.paid_additional());
    }

    #[test]
    fn paying_it_asks_for_a_unit_at_a_battlefield_and_deals_two_when_the_trigger_resolves() {
        let mut fixture = range(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CADET).unwrap();
        assert!(
            additional_confirm(&ctx),
            "355.1.a · the additional cost is asked as you play: {:?}",
            ctx.blob.why
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(CADET), Some(Location::Base(0)));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "three energy and a Fury rune recycled"
        );
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {THEIR_BRUTE}}}")
            ],
            "units at battlefields of either side, none in a base"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in its base is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CADET
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_BRUTE)]);
        let chain_item = ctx.blob.chain[0].id;
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 0, "the damage waits");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 2);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: THEIR_BRUTE,
            n: DAMAGE,
            source: Cause::Item(chain_item)
        }));
        assert!(ctx.on_board(THEIR_BRUTE), "3 Might survives 2");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_damage_kills_a_two_might_unit() {
        let mut fixture = range(true);
        fixture.table.card_mut(THEIR_BRUTE).unwrap().might = Some(2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CADET).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_trash(THEIR_BRUTE));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == THEIR_BRUTE
        )));
    }

    #[test]
    fn declining_the_cost_plays_him_for_two_and_asks_for_no_target() {
        let mut fixture = range(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CADET).unwrap();
        assert!(additional_confirm(&ctx));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.location(CADET), Some(Location::Base(0)));
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy only");
        assert!(ctx.blob.prompt.is_none(), "unpaid, no target prompt");
        assert!(ctx.blob.chain.is_empty(), "unpaid, the trigger never fires");
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 0);
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_fury_rune_the_confirm_is_skipped_and_the_plain_play_goes_through() {
        let mut fixture = range(false);
        let mut ctx = fixture.ctx();
        let mut paid = ChainItem::new(1, ItemKind::Permanent { card: CADET }, 0, Origin::Hand);
        paid.set_slot(SLOT_ADDITIONAL, 1);
        assert!(!crate::engine::pay::affordable(
            &ctx,
            0,
            &cost::of_item(&ctx, &paid, None)
        ));
        fixtures::play_from_hand(&mut ctx, 0, CADET).unwrap();
        assert!(!additional_confirm(&ctx), "{:?}", ctx.blob.why);
        assert_eq!(ctx.location(CADET), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 0);
    }

    #[test]
    fn paid_with_no_unit_at_any_battlefield_the_trigger_fizzles_and_the_cost_stays_paid() {
        let mut fixture = range(true);
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, THEIR_BRUTE].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CADET).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "402.4 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {CADET}}} trigger fizzles · no legal target"
        )));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "357 · a paid cost is not refunded"
        );
        assert_eq!(ctx.location(CADET), Some(Location::Base(0)));
    }
}
