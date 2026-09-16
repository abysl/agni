use super::prelude::{activated, battlefield, done, exhausting_self, gain_xp, named, with_statics};
use super::{Ability, Card, Cost, Flow, Grant, Item, Scope, Stage, Static, Timing};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const XP: u8 = 1;

fn gain_one_xp(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, XP);
    done()
}

pub const BECOME: Ability = named(
    exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], gain_one_xp)),
    "gain 1 XP",
);

pub static LENT: [Ability; 1] = [BECOME];

pub fn is_gardens(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn lends_to(ctx: &Ctx, gardens: u32, unit: u32) -> bool {
    is_gardens(ctx, gardens)
        && statics::in_play(ctx, gardens)
        && ctx.is_unit(unit)
        && ctx.on_board(unit)
        && ctx.location(gardens).is_some()
        && ctx.location(unit) == ctx.location(gardens)
}

pub fn units_granted(ctx: &Ctx, gardens: u32) -> Vec<u32> {
    let mut units: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .map(|held| held.id)
        .filter(|unit| lends_to(ctx, gardens, *unit))
        .collect();
    units.sort_unstable();
    units
}

pub fn lent_ability(ctx: &Ctx, gardens: u32, unit: u32) -> Option<&'static Ability> {
    lends_to(ctx, gardens, unit).then_some(&LENT[0])
}

pub static CARD: Card = with_statics(
    battlefield("Gardens of Becoming", &[], &[]),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: lends_to,
        grants: &[Grant::Ability(&LENT)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::unit;
    use crate::cards::{script_of, Resolved, SelfCost, Trigger, GRANTED};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{GameBlob, ItemKind};
    use crate::Refusal;

    const GARDENS: u32 = fixtures::GROUNDS;
    const JINX: u32 = 90;

    static LENT_VI: Card = unit("Vi", &[], &LENT);

    fn gardens() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(GARDENS).unwrap().name = "Gardens of Becoming".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(JINX, fixtures::BASE, 0, "Jinx", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GARDENS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn lent() -> Fixture {
        let mut fixture = gardens();
        fixture.scripts = fixture.scripts.clone().with_script(fixtures::VI, &LENT_VI);
        fixture
    }

    #[test]
    fn the_gardens_are_a_blank_battlefield_beside_the_ability_they_lend() {
        assert!(std::ptr::eq(
            script_of("Gardens of Becoming").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Ability(lent)],
                ..
            }] if std::ptr::eq(*lent, &LENT)
        ));
        assert!(CARD.replacement.is_none());
        assert_eq!(BECOME.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(BECOME.cost, Some(Cost::FREE));
        assert_eq!(BECOME.self_cost, SelfCost::Exhaust);
        assert!(BECOME.targets.is_empty());
        assert_eq!(BECOME.label, Some("gain 1 XP"));
        assert!(BECOME.condition.is_none());
        assert_eq!(XP, 1);
    }

    #[test]
    fn every_unit_here_of_either_side_is_lent_the_ability_and_nobody_elsewhere_is() {
        let mut fixture = gardens();
        let ctx = fixture.ctx();
        assert!(is_gardens(&ctx, GARDENS));
        assert!(!is_gardens(&ctx, fixtures::ROCKFALL));
        assert!(lends_to(&ctx, GARDENS, fixtures::VI));
        assert!(
            lends_to(&ctx, GARDENS, fixtures::THEIR_UNIT),
            "units here, whoever controls them or holds the gardens"
        );
        assert!(!lends_to(&ctx, GARDENS, JINX), "in base");
        assert!(
            !lends_to(&ctx, GARDENS, fixtures::SPRITE),
            "at the other battlefield"
        );
        assert!(!lends_to(&ctx, GARDENS, GARDENS), "units only");
        assert_eq!(
            units_granted(&ctx, GARDENS),
            [fixtures::VI, fixtures::THEIR_UNIT]
        );
        assert!(std::ptr::eq(
            lent_ability(&ctx, GARDENS, fixtures::VI).unwrap(),
            &LENT[0]
        ));
        assert!(lent_ability(&ctx, GARDENS, JINX).is_none());
        assert!(
            units_granted(&ctx, fixtures::ROCKFALL).is_empty(),
            "Rockfall Path lends nothing"
        );
    }

    #[test]
    fn a_unit_carrying_the_ability_exhausts_to_gain_its_controller_one_xp() {
        let mut fixture = lent();
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == fixtures::VI && offer.label.contains("gain 1 XP")));
        activate::activate(&mut ctx, 0, fixtures::VI, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "the ability costs nothing but the exhaust"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == fixtures::VI
        ));
        assert_eq!(ctx.xp(0), 0, "not before it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 1);
        assert_eq!(ctx.xp(1), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        assert_eq!(
            activate::activate(&mut ctx, 0, fixtures::VI, 0),
            Err(Refusal::Exhausted),
            "once per readying"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_and_an_exhausted_unit_are_refused() {
        let mut fixture = lent();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, fixtures::VI, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        ctx.exhaust(fixtures::VI);
        assert_eq!(
            activate::activate(&mut ctx, 0, fixtures::VI, 0),
            Err(Refusal::Exhausted)
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 0);
    }

    #[test]
    fn a_unit_here_lists_and_activates_the_lent_ability_and_loses_it_when_it_leaves() {
        let mut fixture = gardens();
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == fixtures::VI && offer.label.contains("gain 1 XP"))
            .expect("the lent ability on Vi's strip");
        activate::activate(&mut ctx, 0, fixtures::VI, offer.index).unwrap();
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Lent { holder, lender, index: GRANTED }
                if holder == fixtures::VI && lender == GARDENS
        ));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 1);
        ctx.recall(fixtures::VI, false);
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == fixtures::VI && offer.label.contains("gain 1 XP")));
    }

    #[test]
    fn a_pending_activation_resolves_after_the_unit_left_the_gardens() {
        let mut fixture = gardens();
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == fixtures::VI && offer.label.contains("gain 1 XP"))
            .unwrap();
        activate::activate(&mut ctx, 0, fixtures::VI, offer.index).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        ctx.kill(fixtures::VI, Cause::Rule);
        assert!(!ctx.on_board(fixtures::VI));
        assert!(
            activate::lent_at(&ctx, fixtures::VI, GRANTED).is_none(),
            "she carries nothing off the board"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        let saved_item = ctx.blob.chain[0].clone();
        let saved_table = ctx.table.clone();
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        fixture.table = saved_table;
        fixture.blob = GameBlob::decode(&saved_blob).unwrap();
        fixture.scripts = Resolved::of(&fixture.table);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.blob.chain[0], saved_item);
        assert!(activate::lent_at(&ctx, fixtures::VI, GRANTED).is_none());
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.xp(0),
            1,
            "727.1.c.3.a: the pending item proceeds as normal once the ability is gone"
        );
        assert!(ctx.fault.is_none());
    }
}
