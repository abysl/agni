use super::prelude::{
    battlefield, exhaust, heal, in_combat, location_of, recall, replaces, with_replacement,
};
use super::{Card, Cost, Power, Source, WouldDie};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::engine::pay;

pub const THREE_RAINBOW: Cost = Cost {
    energy: 0,
    power: &[Power::Rainbow, Power::Rainbow, Power::Rainbow],
};

pub fn price() -> cost::Cost {
    cost::of_script(&THREE_RAINBOW, &[])
}

pub fn a_unit_here_would_die_in_combat_and_its_controller_can_pay(
    ctx: &Ctx,
    would: &WouldDie,
    source: Source,
) -> bool {
    let unit = would.unit;
    ctx.is_unit(unit)
        && ctx.on_board(unit)
        && location_of(ctx, unit).is_some()
        && location_of(ctx, unit) == location_of(ctx, source.card)
        && in_combat(ctx, unit)
        && pay::affordable(ctx, ctx.controller(unit), &price())
}

pub fn pay_heal_exhaust_and_recall_instead(ctx: &mut Ctx, would: &WouldDie, source: Source) {
    let unit = would.unit;
    let seat = ctx.controller(unit);
    let Ok(plan) = pay::plan(ctx, seat, &price()) else {
        return;
    };
    pay::pay(ctx, seat, &plan);
    heal(ctx, unit);
    exhaust(ctx, unit);
    recall(ctx, unit, true);
    ctx.narrate(format!(
        "{{seat {seat}}} pays 3 any power · {{card {}}} sends {{card {unit}}} home healed and exhausted instead of dying",
        source.card
    ));
}

pub static CARD: Card = with_replacement(
    battlefield("Altar of Blood", &[], &[]),
    replaces(
        a_unit_here_would_die_in_combat_and_its_controller_can_pay,
        pay_heal_exhaust_and_recall_instead,
    ),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::Location;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, kill, settle, showdown};
    use agni_plugin_sdk::decide::{Effect, BOTTOM};

    const ALTAR: u32 = fixtures::GROUNDS;
    const RAIDER: u32 = 90;

    fn source() -> Source {
        Source {
            card: ALTAR,
            ability: 0,
        }
    }

    fn would(unit: u32) -> WouldDie {
        WouldDie {
            unit,
            cause: Cause::Cleanup { last_item: None },
        }
    }

    fn altar() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ALTAR).unwrap().name = "Altar of Blood".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ALTAR).unwrap(), &CARD));
        fixture
    }

    fn raided(raider_might: u8) -> Fixture {
        let mut fixture = altar();
        fixture.table.cards.push(fixtures::unit(
            RAIDER,
            fixtures::BF1,
            1,
            "Raider",
            raider_might,
        ));
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn fight(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        for _ in 0..4 {
            let Some(held) = ctx.blob.showdown.clone() else {
                break;
            };
            showdown::pass(ctx, held.focus()).unwrap();
        }
        assert!(ctx.blob.showdown.is_none());
        settle(ctx).unwrap();
    }

    fn recycled_runes(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card,
                    zone,
                    seat: owner,
                    index: BOTTOM,
                } if *zone == fixtures::RUNE_DECK && *owner == seat => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_altar_is_a_battlefield_whose_only_text_is_a_priced_replacement() {
        assert!(std::ptr::eq(script_of("Altar of Blood").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_some());
        assert_eq!(THREE_RAINBOW.energy, 0);
        assert_eq!(
            THREE_RAINBOW.power,
            [Power::Rainbow, Power::Rainbow, Power::Rainbow]
        );
        let priced = price();
        assert_eq!(priced.energy, 0);
        assert_eq!(priced.power.len(), 3);
    }

    #[test]
    fn the_replacement_applies_to_a_unit_here_in_combat_whose_controller_holds_three_runes() {
        let mut fixture = raided(3);
        let mut ctx = fixture.ctx();
        assert!(
            !a_unit_here_would_die_in_combat_and_its_controller_can_pay(
                &ctx,
                &would(fixtures::VI),
                source()
            ),
            "not in combat yet"
        );
        assert!(ctx.mark_defender(fixtures::VI));
        assert!(ctx.mark_attacker(RAIDER));
        assert!(a_unit_here_would_die_in_combat_and_its_controller_can_pay(
            &ctx,
            &would(fixtures::VI),
            source()
        ));
        assert!(
            !a_unit_here_would_die_in_combat_and_its_controller_can_pay(
                &ctx,
                &would(RAIDER),
                source()
            ),
            "seat 1 holds two runes, one short"
        );
        assert_eq!(
            kill::applicable(&ctx, &would(fixtures::VI)),
            [source()],
            "a battlefield's face is on the board, so the kill path consults it"
        );
        assert!(kill::applicable(&ctx, &would(RAIDER)).is_empty());
        drop(ctx);
        let mut elsewhere = raided(3);
        elsewhere.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BF2);
        let mut ctx = elsewhere.ctx();
        assert!(ctx.mark_defender(fixtures::SPRITE));
        assert!(
            !a_unit_here_would_die_in_combat_and_its_controller_can_pay(
                &ctx,
                &would(fixtures::SPRITE),
                source()
            ),
            "the Sprite fights at the other battlefield"
        );
        drop(ctx);
        let mut broke = raided(3);
        broke
            .table
            .cards
            .retain(|card| card.id != 43 && card.id != 42);
        broke.resolve();
        let mut ctx = broke.ctx();
        assert!(ctx.mark_defender(fixtures::VI));
        assert!(
            !a_unit_here_would_die_in_combat_and_its_controller_can_pay(
                &ctx,
                &would(fixtures::VI),
                source()
            ),
            "two runes cannot pay three"
        );
    }

    #[test]
    fn the_run_recycles_three_runes_heals_exhausts_and_recalls_the_unit() {
        let mut fixture = raided(3);
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_defender(fixtures::VI));
        ctx.damage(fixtures::VI, 3, Cause::Combat);
        assert_eq!(ctx.damage_on(fixtures::VI), 3);
        let runes = ctx.runes_of(0).len();
        pay_heal_exhaust_and_recall_instead(&mut ctx, &would(fixtures::VI), source());
        assert_eq!(recycled_runes(&ctx, 0).len(), 3);
        assert_eq!(ctx.runes_of(0).len(), runes - 3);
        assert_eq!(ctx.damage_on(fixtures::VI), 0, "healed");
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.on_board(fixtures::VI), "recalled, not dead");
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} pays 3 any power · {{card {ALTAR}}} sends {{card {}}} home healed and exhausted instead of dying",
            fixtures::VI
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn in_a_fight_here_the_defender_who_can_pay_is_sent_home_and_the_raider_who_cannot_dies() {
        let mut fixture = raided(3);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(ctx.runes_of(1).len(), 2);
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 3 might vs defenders 3 might"));
        assert!(
            ctx.on_board(fixtures::VI),
            "three damage would kill her; the altar replaces it"
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert_eq!(
            ctx.runes_of(0).len(),
            1,
            "three runes recycled for the rainbow"
        );
        assert_eq!(recycled_runes(&ctx, 0).len(), 3);
        assert!(
            !ctx.on_board(RAIDER),
            "seat 1 cannot pay, so the raider dies"
        );
        assert_eq!(ctx.runes_of(1).len(), 2);
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "nobody is left standing here"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {ALTAR}}} replaces the death of {{card {}}}",
            fixtures::VI
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_death_outside_combat_here_and_a_combat_death_elsewhere_are_not_replaced() {
        let mut fixture = altar();
        let mut ctx = fixture.ctx();
        assert!(kill::applicable(&ctx, &would(fixtures::VI)).is_empty());
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        assert!(!ctx.on_board(fixtures::VI));
        assert_eq!(ctx.runes_of(0).len(), 4, "no rune is spent");
        drop(ctx);
        let mut away = altar();
        away.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        away.table.cards.retain(|card| card.id != fixtures::SPRITE);
        away.table
            .cards
            .push(fixtures::unit(91, fixtures::BF2, 1, "Their Holder", 2));
        away.blob.set_holder(fixtures::BF1, None);
        away.blob.set_holder(fixtures::BF2, Some(1));
        away.blob.set_contested(fixtures::BF2, Some(0));
        away.resolve();
        let mut ctx = away.ctx();
        fight(&mut ctx);
        assert!(
            !ctx.on_board(91),
            "three on two, the holder dies at Rockfall Path"
        );
        assert_eq!(ctx.runes_of(1).len(), 2);
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert!(ctx.on_board(fixtures::VI));
    }

    #[test]
    #[ignore = "engine gap · the may on the kill path: the replacement pays whenever the price can be paid, because the kill path has no prompt (Sett - The Boss's row); a 'no' must let the unit die"]
    fn the_units_controller_is_asked_before_paying_and_no_lets_it_die() {
        let mut fixture = raided(3);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx.blob.prompt.is_some(), "pay 3 any power to save Vi?");
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(!ctx.on_board(fixtures::VI));
        assert_eq!(ctx.runes_of(0).len(), 4);
    }
}
