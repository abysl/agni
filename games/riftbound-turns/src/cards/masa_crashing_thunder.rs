use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{a_card, card_target, done, play, stun, unit, when, with_additional};
use super::{Card, Cost, Domain, Filter, Flow, Item, Power, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ADDITIONAL: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Order)],
};

pub const ENEMY_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::AtBattlefield]);

pub const STRUCK: TargetSpec = a_card(
    ENEMY_UNIT_AT_A_BATTLEFIELD,
    "an enemy unit at a battlefield to stun",
);

fn thunderclap(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !stun(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is already stunned"));
    }
    done()
}

pub static CARD: Card = with_additional(
    unit(
        "Masa, Crashing Thunder",
        &[],
        &[when(play(&[STRUCK], thunderclap), paid_additional_on_entry)],
    ),
    ADDITIONAL,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MASA: u32 = 90;
    const THEIR_BRUTE: u32 = 91;
    const ORDER_RUNE: u32 = 46;

    fn masa(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(0),
            domain: vec!["Order".into()],
            ..fixtures::unit(MASA, zone, 0, "Masa, Crashing Thunder", 4)
        }
    }

    fn storm(order_rune: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(masa(fixtures::HAND));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        if order_rune {
            fixture
                .table
                .cards
                .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
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

    #[test]
    fn the_script_prints_an_optional_order_cost_and_a_stun_gated_on_paying_it() {
        assert!(std::ptr::eq(
            script_of("Masa, Crashing Thunder").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Masa, Crashing Thunder");
        assert_eq!(CARD.additional, Some(ADDITIONAL));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional, "the may is the cost, not the stun");
        assert!(ability.condition.is_some());
        assert_eq!(ability.targets, &[STRUCK]);
        assert_eq!((STRUCK.min, STRUCK.max), (1, 1));
        assert_eq!(STRUCK.kind, TargetKind::Card);
        assert_eq!(STRUCK.filter, ENEMY_UNIT_AT_A_BATTLEFIELD);
    }

    #[test]
    fn the_additional_cost_adds_an_order_need_only_when_the_slot_says_paid() {
        let mut fixture = storm(true);
        let ctx = fixture.ctx();
        let mut held = ChainItem::new(1, ItemKind::Permanent { card: MASA }, 0, Origin::Hand);
        let plain = cost::of_item(&ctx, &held, None);
        assert_eq!(plain.energy, 2);
        assert!(plain.power.is_empty());
        held.set_slot(SLOT_ADDITIONAL, 1);
        let paid = cost::of_item(&ctx, &held, None);
        assert_eq!(paid.energy, 2);
        assert_eq!(paid.power, [Need::Domain(Domain::Order)]);
        assert!(held.paid_additional());
        assert!(crate::engine::pay::affordable(&ctx, 0, &paid));
    }

    #[test]
    fn paying_the_order_rune_asks_for_an_enemy_unit_at_a_battlefield_and_stuns_it() {
        let mut fixture = storm(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MASA).unwrap();
        assert!(additional_confirm(&ctx), "{:?}", ctx.blob.why);
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(MASA), Some(Location::Base(0)));
        assert_eq!(
            ctx.card(ORDER_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Order rune is recycled for the additional cost"
        );
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {THEIR_BRUTE}}}"),
            ],
            "enemy units at battlefields · not Jinx in their base, not Vi"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy in its base is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is refused"
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
            ItemKind::Trigger { source, index: 0 } if source == MASA
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_BRUTE)]);
        assert!(!ctx.is_stunned(THEIR_BRUTE), "the stun waits");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(THEIR_BRUTE));
        assert!(!ctx.deals_combat_damage(THEIR_BRUTE));
        assert!(!ctx.is_stunned(fixtures::SPRITE), "only the pick");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {THEIR_BRUTE}}} is stunned")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_rune_plays_him_for_two_and_asks_for_no_target() {
        let mut fixture = storm(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MASA).unwrap();
        assert!(additional_confirm(&ctx));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.location(MASA), Some(Location::Base(0)));
        assert_eq!(
            ctx.card(ORDER_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL),
            "no additional cost, no Order rune recycled"
        );
        assert!(ctx.blob.prompt.is_none(), "unpaid, no target prompt");
        assert!(ctx.blob.chain.is_empty(), "unpaid, the trigger never fires");
        assert!(!ctx.is_stunned(THEIR_BRUTE));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_an_order_rune_the_confirm_is_skipped_and_the_plain_play_goes_through() {
        let mut fixture = storm(false);
        let mut ctx = fixture.ctx();
        let mut paid = ChainItem::new(1, ItemKind::Permanent { card: MASA }, 0, Origin::Hand);
        paid.set_slot(SLOT_ADDITIONAL, 1);
        assert!(!crate::engine::pay::affordable(
            &ctx,
            0,
            &cost::of_item(&ctx, &paid, None)
        ));
        fixtures::play_from_hand(&mut ctx, 0, MASA).unwrap();
        assert!(!additional_confirm(&ctx), "{:?}", ctx.blob.why);
        assert_eq!(ctx.location(MASA), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_stunned(THEIR_BRUTE));
    }
}
