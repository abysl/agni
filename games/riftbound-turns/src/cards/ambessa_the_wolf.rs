use super::prelude::{empower, in_combat, is_empowered, unit, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const EMPOWER: Cost = Cost {
    energy: 3,
    power: &[Power::Domain(Domain::Body)],
};
pub const MIGHT: i16 = 3;
pub const EMPOWER_ABILITY: u8 = 0;

pub fn takes_no_damage(ctx: &Ctx, me: u32) -> bool {
    ctx.script(me)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
        && statics::in_play(ctx, me)
        && is_empowered(ctx, me)
        && !in_combat(ctx, me)
}

pub static CARD: Card = with_statics(
    unit(
        "Ambessa, The Wolf",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER)],
    ),
    &[
        Static::While(is_empowered, &[Grant::Might(MIGHT)]),
        Static::NoDamage(|ctx, me, _| takes_no_damage(ctx, me)),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const AMBESSA: u32 = 90;
    const BODY_RUNE: u32 = 46;

    fn ambessa(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Body".into()],
            ..fixtures::unit(AMBESSA, zone, 0, "Ambessa, The Wolf", 4)
        }
    }

    fn war_camp() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ambessa(fixtures::BF1));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(AMBESSA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn her_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == AMBESSA)
            .collect()
    }

    fn empower_her(ctx: &mut Ctx) {
        activate::activate(ctx, 0, AMBESSA, EMPOWER_ABILITY).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(AMBESSA));
    }

    #[test]
    fn the_champion_prints_empower_and_her_empowered_line_is_a_while_plus_a_no_damage_static() {
        assert!(std::ptr::eq(script_of("Ambessa, The Wolf").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[usize::from(EMPOWER_ABILITY)];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(MIGHT)]), Static::NoDamage(_)]
        ));
        assert_eq!(MIGHT, 3);
    }

    #[test]
    fn empower_pays_three_and_a_body_then_she_reads_seven_and_is_refused_a_second_time() {
        let mut fixture = war_camp();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(AMBESSA), 4);
        let offers = her_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {AMBESSA}}}: empower (3 energy and 1 Body power)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, AMBESSA, EMPOWER_ABILITY).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "three runes exhausted for the energy, the Body rune among them"
        );
        assert_ne!(
            ctx.card(BODY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL),
            "the Body rune is recycled for the power"
        );
        assert_eq!(ctx.current_might(AMBESSA), 4, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(is_empowered(&ctx, AMBESSA));
        assert_eq!(ctx.current_might(AMBESSA), 4 + i32::from(MIGHT));
        assert!(her_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, AMBESSA, EMPOWER_ABILITY),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        ctx.disempower(AMBESSA);
        assert_eq!(ctx.current_might(AMBESSA), 4, "the While follows the state");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_body_rune_the_offer_is_greyed_and_the_activation_refused() {
        let mut fixture = war_camp();
        fixture.table.cards.retain(|card| card.id != BODY_RUNE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let offers = her_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].enabled);
        assert_eq!(
            activate::activate(&mut ctx, 0, AMBESSA, EMPOWER_ABILITY),
            Err(Refusal::NoPowerOf)
        );
        assert!(!ctx.is_empowered(AMBESSA));
        assert_eq!(ctx.current_might(AMBESSA), 4);
    }

    #[test]
    fn the_immunity_reads_empowered_and_out_of_combat_only() {
        let mut fixture = war_camp();
        let mut ctx = fixture.ctx();
        assert!(!takes_no_damage(&ctx, AMBESSA), "not empowered");
        empower_her(&mut ctx);
        assert!(takes_no_damage(&ctx, AMBESSA));
        assert!(!takes_no_damage(&ctx, fixtures::VI), "Vi is not Ambessa");
        assert!(ctx.mark_attacker(AMBESSA));
        assert!(in_combat(&ctx, AMBESSA));
        assert!(
            !takes_no_damage(&ctx, AMBESSA),
            "in combat she is dealt damage"
        );
        ctx.clear_designation(AMBESSA);
        assert!(!in_combat(&ctx, AMBESSA));
        assert!(takes_no_damage(&ctx, AMBESSA));
        ctx.recall(AMBESSA, true);
        assert!(takes_no_damage(&ctx, AMBESSA), "the base is in play too");
        ctx.disempower(AMBESSA);
        assert!(!takes_no_damage(&ctx, AMBESSA));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn while_empowered_she_is_dealt_no_damage_unless_she_is_in_combat() {
        let mut fixture = war_camp();
        let mut ctx = fixture.ctx();
        empower_her(&mut ctx);
        assert!(
            !ctx.damage(AMBESSA, 6, Cause::Item(1)),
            "no damage is dealt"
        );
        assert_eq!(ctx.damage_on(AMBESSA), 0);
        assert!(ctx.on_board(AMBESSA));
        assert!(ctx.mark_attacker(AMBESSA));
        assert!(ctx.damage(AMBESSA, 2, Cause::Combat));
        assert_eq!(ctx.damage_on(AMBESSA), 2);
        assert!(
            ctx.damage(AMBESSA, 1, Cause::Item(1)),
            "in combat even an item's damage lands"
        );
        assert_eq!(ctx.damage_on(AMBESSA), 3);
    }
}
