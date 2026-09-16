use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{buff, done, play, unit, when, with_additional, ONE_ENERGY};
use super::{Card, Cost, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const ADDITIONAL: Cost = ONE_ENERGY;

fn swell(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    }
    done()
}

pub static CARD: Card = with_additional(
    unit(
        "Sea Monkey",
        &[],
        &[when(play(&[], swell), paid_additional_on_entry)],
    ),
    ADDITIONAL,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::cost;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, SLOT_ADDITIONAL};
    use agni_plugin_sdk::table::CardInfo;

    const MONKEY: u32 = 90;

    fn monkey(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Body".into()],
            ..fixtures::unit(MONKEY, zone, 0, "Sea Monkey", 2)
        }
    }

    fn reef(ready_runes: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(monkey(fixtures::HAND));
        for rune in [41, 42, 43].into_iter().skip(ready_runes) {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
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

    #[test]
    fn the_script_prints_an_optional_one_energy_cost_and_a_self_buff_gated_on_paying_it() {
        assert!(std::ptr::eq(script_of("Sea Monkey").unwrap(), &CARD));
        assert_eq!(CARD.name, "Sea Monkey");
        assert_eq!(CARD.additional, Some(ADDITIONAL));
        assert_eq!(ADDITIONAL.energy, 1);
        assert!(ADDITIONAL.power.is_empty());
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional, "the may is the cost, not the buff");
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_some());
    }

    #[test]
    fn the_additional_cost_adds_one_energy_only_when_the_slot_says_paid() {
        let mut fixture = reef(3);
        let ctx = fixture.ctx();
        let mut held = ChainItem::new(1, ItemKind::Permanent { card: MONKEY }, 0, Origin::Hand);
        assert_eq!(cost::of_item(&ctx, &held, None).energy, 2);
        held.set_slot(SLOT_ADDITIONAL, 1);
        let paid = cost::of_item(&ctx, &held, None);
        assert_eq!(paid.energy, 3);
        assert!(paid.power.is_empty());
        assert!(held.paid_additional());
    }

    #[test]
    fn paying_the_third_energy_buffs_it_when_the_trigger_resolves() {
        let mut fixture = reef(3);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONKEY).unwrap();
        assert!(
            additional_confirm(&ctx),
            "355.1.a · the additional cost is asked as you play: {:?}",
            ctx.blob.why
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(MONKEY), Some(Location::Base(0)));
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "three runes exhausted");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MONKEY
        ));
        assert!(!ctx.is_buffed(MONKEY), "the buff waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(MONKEY));
        assert_eq!(ctx.current_might(MONKEY), 3);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {MONKEY}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_energy_plays_it_for_two_and_it_stays_unbuffed() {
        let mut fixture = reef(3);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONKEY).unwrap();
        assert!(additional_confirm(&ctx));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.location(MONKEY), Some(Location::Base(0)));
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy only");
        assert!(ctx.blob.chain.is_empty(), "unpaid, the trigger never fires");
        assert!(!ctx.is_buffed(MONKEY));
        assert_eq!(ctx.current_might(MONKEY), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_only_two_ready_runes_the_confirm_is_skipped_and_it_plays_plain() {
        let mut fixture = reef(2);
        let mut ctx = fixture.ctx();
        let mut paid = ChainItem::new(1, ItemKind::Permanent { card: MONKEY }, 0, Origin::Hand);
        paid.set_slot(SLOT_ADDITIONAL, 1);
        assert!(!crate::engine::pay::affordable(
            &ctx,
            0,
            &cost::of_item(&ctx, &paid, None)
        ));
        fixtures::play_from_hand(&mut ctx, 0, MONKEY).unwrap();
        assert!(!additional_confirm(&ctx), "{:?}", ctx.blob.why);
        assert_eq!(ctx.location(MONKEY), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(MONKEY));
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
    }

    #[test]
    fn a_monkey_bounced_in_response_has_nothing_to_buff() {
        let mut fixture = reef(3);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONKEY).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.bounce(MONKEY);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(MONKEY));
        assert!(!ctx.is_buffed(MONKEY));
        assert!(!ctx
            .blob
            .log
            .contains(&format!("{{card {MONKEY}}} is buffed")));
    }
}
