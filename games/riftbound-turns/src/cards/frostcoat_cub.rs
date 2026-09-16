use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{
    a_unit, card_target, done, might_this_turn, play, unit, when, with_additional,
};
use super::{Card, Cost, Domain, Flow, Item, Power, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ADDITIONAL: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Mind)],
};
pub const MIGHT: i16 = -2;

pub const CHILLED: TargetSpec = a_unit("a unit to give -2 Might this turn");

fn chill(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
    }
    done()
}

pub static CARD: Card = with_additional(
    unit(
        "Frostcoat Cub",
        &[],
        &[when(play(&[CHILLED], chill), paid_additional_on_entry)],
    ),
    ADDITIONAL,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Filter, TargetKind, Trigger};
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const CUB: u32 = 90;
    const MIND_RUNE: u32 = 46;

    fn cub(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Mind".into()],
            ..fixtures::unit(CUB, zone, 0, "Frostcoat Cub", 3)
        }
    }

    fn tundra(mind_rune: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(cub(fixtures::HAND));
        if mind_rune {
            fixture
                .table
                .cards
                .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
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
    fn the_script_prints_an_optional_mind_cost_and_a_targeted_trigger_gated_on_paying_it() {
        assert!(std::ptr::eq(script_of("Frostcoat Cub").unwrap(), &CARD));
        assert_eq!(CARD.name, "Frostcoat Cub");
        assert_eq!(CARD.additional, Some(ADDITIONAL));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional, "the may is the cost, not the chill");
        assert!(ability.condition.is_some());
        assert_eq!(ability.targets, &[CHILLED]);
        assert_eq!((CHILLED.min, CHILLED.max), (1, 1));
        assert_eq!(CHILLED.kind, TargetKind::Card);
        assert_eq!(CHILLED.filter, Filter::Unit);
        assert_eq!(MIGHT, -2);
    }

    #[test]
    fn the_additional_cost_adds_a_mind_need_only_when_the_slot_says_paid() {
        let mut fixture = tundra(true);
        let ctx = fixture.ctx();
        let mut held = ChainItem::new(1, ItemKind::Permanent { card: CUB }, 0, Origin::Hand);
        let plain = cost::of_item(&ctx, &held, None);
        assert_eq!(plain.energy, 3);
        assert!(plain.power.is_empty());
        held.set_slot(SLOT_ADDITIONAL, 1);
        let paid = cost::of_item(&ctx, &held, None);
        assert_eq!(paid.energy, 3);
        assert_eq!(paid.power, [Need::Domain(Domain::Mind)]);
        assert!(held.paid_additional());
    }

    #[test]
    fn paying_the_mind_rune_asks_for_any_unit_and_it_loses_two_might_for_the_turn() {
        let mut fixture = tundra(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CUB).unwrap();
        assert!(additional_confirm(&ctx), "{:?}", ctx.blob.why);
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(CUB), Some(Location::Base(0)));
        assert_eq!(
            ctx.card(MIND_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Mind rune is recycled for the additional cost"
        );
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {CUB}}}"),
            ],
            "any unit, the cub included"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::LEGEND_CARD]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a legend is not a unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CUB
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3, "the chill waits");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::SPRITE), 1);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2, "only the pick");
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            3,
            "it ends with the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_rune_plays_the_cub_for_three_and_asks_for_no_target() {
        let mut fixture = tundra(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CUB).unwrap();
        assert!(additional_confirm(&ctx));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.location(CUB), Some(Location::Base(0)));
        assert_eq!(
            ctx.card(MIND_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL),
            "no additional cost, no Mind rune recycled"
        );
        assert!(ctx.blob.prompt.is_none(), "unpaid, no target prompt");
        assert!(ctx.blob.chain.is_empty(), "unpaid, the trigger never fires");
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_mind_rune_the_confirm_is_skipped_and_the_plain_play_goes_through() {
        let mut fixture = tundra(false);
        let mut ctx = fixture.ctx();
        let mut paid = ChainItem::new(1, ItemKind::Permanent { card: CUB }, 0, Origin::Hand);
        paid.set_slot(SLOT_ADDITIONAL, 1);
        assert!(!crate::engine::pay::affordable(
            &ctx,
            0,
            &cost::of_item(&ctx, &paid, None)
        ));
        fixtures::play_from_hand(&mut ctx, 0, CUB).unwrap();
        assert!(!additional_confirm(&ctx), "{:?}", ctx.blob.why);
        assert_eq!(ctx.location(CUB), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
    }
}
