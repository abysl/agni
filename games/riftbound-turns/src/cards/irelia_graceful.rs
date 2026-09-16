use super::prelude::{unit, with_statics, ONE_ENERGY, RAINBOW};
use super::{Card, Cost, Static};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, ItemKind, TargetRef};

pub const REDUCTIONS: [Cost; 2] = [ONE_ENERGY, RAINBOW];

pub fn my_spell_choosing_me(ctx: &Ctx, item: &ChainItem, me: u32) -> bool {
    matches!(item.kind, ItemKind::Spell { .. })
        && item.controller == ctx.controller(me)
        && item.targets.contains(&TargetRef::Card(me))
}

pub fn chosen_reduction_until_the_pay_stage_asks_energy_or_power(
    ctx: &Ctx,
    item: &ChainItem,
) -> Cost {
    let base = cost::base_of_item(ctx, item);
    if base.power.is_empty() {
        ONE_ENERGY
    } else {
        RAINBOW
    }
}

pub fn graceful_discount(ctx: &Ctx, item: &ChainItem, me: u32) -> Cost {
    if !my_spell_choosing_me(ctx, item, me) {
        return Cost::FREE;
    }
    chosen_reduction_until_the_pay_stage_asks_energy_or_power(ctx, item)
}

pub static CARD: Card = with_statics(
    unit("Irelia - Graceful", &[], &[]),
    &[Static::PlayDiscount(graceful_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_friendly_unit, card_target, might_this_turn, play, spell};
    use crate::cards::{script_of, Domain, Flow, Item, Stage};
    use crate::engine::cost::Need;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority};
    use crate::state::{Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const IRELIA: u32 = 90;
    const OTHER: u32 = 91;
    const DISCIPLINE: u32 = 92;
    const PLAIN: u32 = 93;
    const THEIR_IRELIA: u32 = 94;

    fn discipline_run(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
        if let Some(unit) = card_target(ctx, item, 0) {
            might_this_turn(ctx, item, unit, 2, None);
        }
        Flow::Done
    }

    static DISCIPLINE_CARD: Card = spell(
        "Discipline",
        &[],
        &[play(&[a_friendly_unit("a friendly unit")], discipline_run)],
    );

    fn irelia(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(id, fixtures::BASE, seat, "Irelia - Graceful", 4)
        }
    }

    fn dojo() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(irelia(IRELIA, 0));
        fixture.table.cards.push(irelia(THEIR_IRELIA, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(OTHER, fixtures::BASE, 0, "Other", 2));
        let mut discipline = fixtures::spell(DISCIPLINE, fixtures::HAND, 0, "Discipline", 2, 1);
        discipline.domain = vec!["Calm".into()];
        fixture.table.cards.push(discipline);
        fixture
            .table
            .cards
            .push(fixtures::spell(PLAIN, fixtures::HAND, 0, "Plain", 3, 0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(DISCIPLINE, &DISCIPLINE_CARD)
            .with_script(PLAIN, &DISCIPLINE_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(IRELIA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn choosing(card: u32, seat: u8, target: u32) -> ChainItem {
        let mut item = ChainItem::new(1, ItemKind::Spell { card }, seat, Origin::Hand);
        item.targets.push(TargetRef::Card(target));
        item.spec_counts.push(1);
        item
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_spell_discount_offering_energy_or_power() {
        assert!(std::ptr::eq(script_of("Irelia - Graceful").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::PlayDiscount(graceful_discount)));
        assert_eq!(REDUCTIONS[0].energy, 1);
        assert!(REDUCTIONS[0].power.is_empty());
        assert_eq!(REDUCTIONS[1].energy, 0);
        assert_eq!(REDUCTIONS[1].power, [crate::cards::Power::Rainbow]);
    }

    #[test]
    fn a_spell_of_yours_choosing_her_is_a_power_cheaper_and_one_choosing_another_unit_is_not() {
        let mut fixture = dojo();
        let ctx = fixture.ctx();
        let on_her = choosing(DISCIPLINE, 0, IRELIA);
        assert!(my_spell_choosing_me(&ctx, &on_her, IRELIA));
        assert_eq!(graceful_discount(&ctx, &on_her, IRELIA), RAINBOW);
        let priced = cost::of_item(&ctx, &on_her, None);
        assert_eq!(priced.energy, 2);
        assert!(priced.power.is_empty(), "the Calm need is struck");
        let on_other = choosing(DISCIPLINE, 0, OTHER);
        assert!(!my_spell_choosing_me(&ctx, &on_other, IRELIA));
        assert_eq!(graceful_discount(&ctx, &on_other, IRELIA), Cost::FREE);
        let priced = cost::of_item(&ctx, &on_other, None);
        assert_eq!(priced.energy, 2);
        assert_eq!(priced.power, [Need::Domain(Domain::Calm)]);
        assert_eq!(
            graceful_discount(&ctx, &on_her, THEIR_IRELIA),
            Cost::FREE,
            "their Irelia is not the one chosen"
        );
    }

    #[test]
    fn a_spell_without_a_power_cost_takes_the_energy_instead_and_an_enemy_spell_takes_nothing() {
        let mut fixture = dojo();
        let ctx = fixture.ctx();
        let plain = choosing(PLAIN, 0, IRELIA);
        assert_eq!(graceful_discount(&ctx, &plain, IRELIA), ONE_ENERGY);
        assert_eq!(cost::of_item(&ctx, &plain, None).energy, 2);
        let theirs = choosing(DISCIPLINE, 1, IRELIA);
        assert!(
            !my_spell_choosing_me(&ctx, &theirs, IRELIA),
            "an opponent's spell choosing her is not yours"
        );
        assert_eq!(graceful_discount(&ctx, &theirs, IRELIA), Cost::FREE);
        assert_eq!(
            cost::of_item(&ctx, &theirs, None).energy,
            2,
            "no Deflect to add either"
        );
    }

    #[test]
    fn choosing_her_through_the_engine_prices_the_discipline_at_two_energy_and_no_calm() {
        let mut fixture = dojo();
        fixture.table.card_mut(42).unwrap().exhausted = true;
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(DISCIPLINE, &DISCIPLINE_CARD);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "two Fury, no Calm ready");
        let entry = crate::engine::ctx::EntryMove {
            card: DISCIPLINE,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert!(
            legal::classify(&ctx, 0, &entry).is_ok(),
            "classify prices before the target is known and the exhausted Calm rune can still be recycled"
        );
        fixtures::play_from_hand(&mut ctx, 0, DISCIPLINE).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {IRELIA}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "two energy from two runes");
        assert_eq!(
            ctx.runes_of(0).len(),
            4,
            "no rune recycled: the Calm need was struck by her"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(IRELIA), 6);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn choosing_the_other_unit_through_the_engine_recycles_the_calm_rune() {
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DISCIPLINE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {OTHER}}}")).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(ctx.runes_of(0).len(), 3, "the Calm rune paid the power");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(OTHER), 4);
        assert_eq!(ctx.current_might(IRELIA), 4);
    }

    #[test]
    #[ignore = "engine gap · a chosen discount: Static::PlayDiscount returns one Cost, the engine owes a pay-stage pick between REDUCTIONS so the player, not chosen_reduction_until_the_pay_stage_asks_energy_or_power, decides whether the energy or the power comes off"]
    fn the_player_may_take_the_energy_off_a_spell_that_also_has_a_power_cost() {
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DISCIPLINE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {IRELIA}}}")).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PayWith { .. })));
        assert_eq!(fixtures::labels(&ctx), ["1 energy", "1 any power"]);
        fixtures::choose(&mut ctx, 0, "1 energy").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "one energy from one rune");
        assert_eq!(ctx.runes_of(0).len(), 3, "the Calm rune paid the power");
    }
}
