use super::prelude::{empower, is_empowered, unit, with_statics, RAINBOW};
use super::{Card, Cost, Keyword, Static};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::engine::statics;
use crate::state::{ChainItem, ItemKind};

pub const EMPOWER: Cost = Cost {
    energy: 3,
    power: &[],
};
pub const REDUCTION: u8 = 1;
pub const MINIMUM: u8 = 1;
pub const EMPOWER_ABILITY: u8 = 0;

pub fn researching(ctx: &Ctx, item: &ChainItem, me: u32) -> bool {
    matches!(item.kind, ItemKind::Spell { .. })
        && ctx.controller(me) == item.controller
        && statics::in_play(ctx, me)
        && is_empowered(ctx, me)
}

fn researchers_before(ctx: &Ctx, item: &ChainItem, me: u32) -> u8 {
    let earlier = ctx
        .table
        .cards
        .iter()
        .filter(|held| held.id < me)
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| researching(ctx, item, held.id))
        .count();
    u8::try_from(earlier).unwrap_or(u8::MAX)
}

pub fn discount(ctx: &Ctx, item: &ChainItem, me: u32) -> Cost {
    if !researching(ctx, item, me) {
        return Cost::FREE;
    }
    let base = cost::base_of_item(ctx, item).energy;
    let room = base
        .saturating_sub(MINIMUM)
        .saturating_sub(researchers_before(ctx, item, me));
    Cost {
        energy: room.min(REDUCTION),
        power: RAINBOW.power,
    }
}

pub static CARD: Card = with_statics(
    unit(
        "Applied Researchers",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
    ),
    &[Static::PlayDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Domain, Power, SelfCost, Timing, Trigger};
    use crate::engine::cost::Need;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, legal, priority};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const RESEARCHERS: u32 = 90;
    const SECOND: u32 = 91;
    const THEIRS: u32 = 92;
    const CHEAP: u32 = 93;
    const EXTRA_RUNES: [u32; 3] = [46, 47, 48];

    fn researchers(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Mind".into()],
            ..fixtures::unit(id, fixtures::BASE, seat, "Applied Researchers", 4)
        }
    }

    fn laboratory() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(researchers(RESEARCHERS, 0));
        fixture.table.cards.push(fixtures::spell(
            CHEAP,
            fixtures::HAND,
            0,
            "Punch First",
            1,
            1,
        ));
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RESEARCHERS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn spark(seat: u8) -> ChainItem {
        ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            seat,
            Origin::Hand,
        )
    }

    fn entry(ctx: &Ctx) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card: fixtures::HAND_SPELL,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    fn empower_them(ctx: &mut Ctx) {
        activate::activate(ctx, 0, RESEARCHERS, EMPOWER_ABILITY).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(RESEARCHERS));
    }

    #[test]
    fn the_unit_prints_empower_and_carries_one_spell_discount_gated_on_the_state() {
        assert!(std::ptr::eq(
            script_of("Applied Researchers").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[usize::from(EMPOWER_ABILITY)];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::PlayDiscount(discount)));
        assert_eq!((REDUCTION, MINIMUM), (1, 1));
    }

    #[test]
    fn before_empower_they_change_no_cost_and_empower_pays_three_once() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        assert!(!researching(&ctx, &spark(0), RESEARCHERS));
        assert_eq!(discount(&ctx, &spark(0), RESEARCHERS), Cost::FREE);
        let priced = cost::of_item(&ctx, &spark(0), None);
        assert_eq!(priced.energy, 2);
        assert_eq!(priced.power, [Need::Domain(Domain::Fury)]);
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == RESEARCHERS)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {RESEARCHERS}}}: empower (3 energy)")
        );
        let runes = ctx.ready_runes_of(0).len();
        empower_them(&mut ctx);
        assert_eq!(ctx.ready_runes_of(0).len(), runes - 3);
        assert_eq!(
            activate::activate(&mut ctx, 0, RESEARCHERS, EMPOWER_ABILITY),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn empowered_their_controllers_spells_cost_one_energy_and_a_rainbow_less_down_to_one() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        empower_them(&mut ctx);
        assert!(researching(&ctx, &spark(0), RESEARCHERS));
        let cut = discount(&ctx, &spark(0), RESEARCHERS);
        assert_eq!(cut.energy, REDUCTION);
        assert_eq!(cut.power, [Power::Rainbow]);
        let priced = cost::of_item(&ctx, &spark(0), None);
        assert_eq!(priced.energy, 1);
        assert!(priced.power.is_empty(), "the rainbow strikes the Fury need");
        assert!(!researching(&ctx, &spark(1), RESEARCHERS));
        assert_eq!(
            cost::of_item(&ctx, &spark(1), None).energy,
            2,
            "spells you play, not the opponent's"
        );
        let punch = ChainItem::new(2, ItemKind::Spell { card: CHEAP }, 0, Origin::Hand);
        assert_eq!(
            discount(&ctx, &punch, RESEARCHERS).energy,
            0,
            "one energy is the floor"
        );
        let priced = cost::of_item(&ctx, &punch, None);
        assert_eq!(priced.energy, 1);
        assert!(priced.power.is_empty(), "the rainbow still goes");
        let unit = ChainItem::new(
            3,
            ItemKind::Permanent {
                card: fixtures::HAND_UNIT,
            },
            0,
            Origin::Hand,
        );
        assert!(
            !researching(&ctx, &unit, RESEARCHERS),
            "a unit is not a spell"
        );
        assert_eq!(cost::of_item(&ctx, &unit, None).energy, 2);
        ctx.disempower(RESEARCHERS);
        assert_eq!(cost::of_item(&ctx, &spark(0), None).energy, 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_empowered_teams_take_one_each_never_below_one_and_an_enemy_team_takes_nothing() {
        let mut fixture = laboratory();
        fixture.table.cards.push(researchers(SECOND, 0));
        fixture.table.cards.push(researchers(THEIRS, 1));
        fixture.table.card_mut(fixtures::HAND_SPELL).unwrap().energy = Some(3);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        for team in [RESEARCHERS, SECOND, THEIRS] {
            assert!(ctx.empower(team));
        }
        assert_eq!(researchers_before(&ctx, &spark(0), RESEARCHERS), 0);
        assert_eq!(researchers_before(&ctx, &spark(0), SECOND), 1);
        assert_eq!(discount(&ctx, &spark(0), RESEARCHERS).energy, 1);
        assert_eq!(discount(&ctx, &spark(0), SECOND).energy, 1);
        assert_eq!(
            discount(&ctx, &spark(0), THEIRS),
            Cost::FREE,
            "the enemy team reads the spell as not theirs"
        );
        let priced = cost::of_item(&ctx, &spark(0), None);
        assert_eq!(priced.energy, 1, "three less one less one");
        assert!(priced.power.is_empty());
        ctx.disempower(SECOND);
        assert_eq!(cost::of_item(&ctx, &spark(0), None).energy, 2);
        drop(ctx);
        let mut two_cost = laboratory();
        two_cost.table.cards.push(researchers(SECOND, 0));
        two_cost.resolve();
        let mut ctx = two_cost.ctx();
        assert!(ctx.empower(RESEARCHERS));
        assert!(ctx.empower(SECOND));
        assert_eq!(discount(&ctx, &spark(0), RESEARCHERS).energy, 1);
        assert_eq!(
            discount(&ctx, &spark(0), SECOND).energy,
            0,
            "the second team finds the minimum already reached"
        );
        assert_eq!(cost::of_item(&ctx, &spark(0), None).energy, 1);
    }

    #[test]
    fn the_discount_is_paid_through_the_engine_and_without_it_the_same_runes_are_too_few() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        empower_them(&mut ctx);
        assert!(ctx.exhaust(46));
        assert_eq!(ctx.ready_runes_of(0).len(), 2);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", fixtures::labels(&ctx));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "one energy and no power: one rune exhausted, none recycled"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        drop(ctx);
        let mut plain = laboratory();
        for rune in [41, 42, 43, 46, 47] {
            plain.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = plain.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx)),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            }),
            "unempowered, one ready rune cannot pay the two energy"
        );
    }

    #[test]
    fn one_ready_rune_is_enough_for_the_spark_when_the_team_is_empowered() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        empower_them(&mut ctx);
        for rune in [46, 47] {
            assert!(ctx.exhaust(rune));
        }
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(cost::of_item(&ctx, &spark(0), None).energy, 1);
        legal::classify(&ctx, 0, &entry(&ctx)).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
    }
}
