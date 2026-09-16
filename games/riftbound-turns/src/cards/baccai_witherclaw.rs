use super::prelude::{
    channel_exhausted, deathknell, done, empower, unit, when, when_empowered, with_statics,
};
use super::{Card, Cost, Flow, Grant, Item, Keyword, Power, Stage, Static};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 1,
    power: &[Power::Rainbow, Power::Rainbow],
};
pub const MIGHT: i16 = 2;
pub const RUNES: usize = 2;

fn empowered_unit(ctx: &Ctx, unit: u32) -> bool {
    ctx.is_empowered(unit)
}

fn wither(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    channel_exhausted(ctx, item.controller, RUNES);
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Baccai Witherclaw",
        &[Keyword::Empower(EMPOWER), Keyword::Deathknell],
        &[
            empower(EMPOWER),
            when(deathknell(&[], wither), when_empowered),
        ],
    ),
    &[Static::While(empowered_unit, &[Grant::Might(MIGHT)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const WITHERCLAW: u32 = 90;
    const DEATH: u8 = 1;

    fn witherclaw() -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Body".into()],
            ..fixtures::unit(WITHERCLAW, fixtures::BASE, 0, "Baccai Witherclaw", 4)
        }
    }

    fn tomb() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(witherclaw());
        fixture.table.card_mut(45).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WITHERCLAW).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_all(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    fn empowered_by_paying(ctx: &mut Ctx) {
        activate::activate(ctx, 0, WITHERCLAW, 0).unwrap();
        resolve_all(ctx);
        assert!(ctx.is_empowered(WITHERCLAW));
    }

    fn pool_of(ctx: &Ctx, seat: u8) -> (usize, usize) {
        let pool: Vec<bool> = ctx
            .table
            .held(fixtures::RUNE_POOL, seat)
            .map(|rune| rune.exhausted)
            .collect();
        (pool.len(), pool.iter().filter(|held| **held).count())
    }

    #[test]
    fn the_script_prints_empower_and_deathknell_with_a_conditional_deathknell_and_an_empowered_might_static(
    ) {
        assert!(std::ptr::eq(script_of("Baccai Witherclaw").unwrap(), &CARD));
        assert_eq!(
            CARD.keywords,
            [Keyword::Empower(EMPOWER), Keyword::Deathknell]
        );
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 1);
        assert_eq!(EMPOWER.power, [Power::Rainbow, Power::Rainbow]);
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        let death = &CARD.abilities[usize::from(DEATH)];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.condition.is_some(), "while Empowered");
        assert!(death.targets.is_empty());
        assert!(!death.optional);
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(2)])]
        ));
        assert_eq!(RUNES, 2);
    }

    #[test]
    fn empowering_pays_one_and_two_rainbows_and_makes_him_six_might() {
        let mut fixture = tomb();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(WITHERCLAW), 4);
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == WITHERCLAW)
            .collect();
        assert_eq!(offers.len(), 1);
        assert!(offers[0].enabled);
        assert_eq!(
            offers[0].label,
            format!("{{card {WITHERCLAW}}}: empower (1 energy and 2 any power)")
        );
        let runes = ctx.runes_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, WITHERCLAW, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 2,
            "two runes recycled for the power"
        );
        assert!(
            ctx.ready_runes_of(0).len() < ready,
            "one exhausted for the energy"
        );
        assert!(
            !ctx.card(WITHERCLAW).unwrap().exhausted,
            "827.1 · Empower never exhausts him"
        );
        assert!(!ctx.is_empowered(WITHERCLAW), "nothing until it resolves");
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(WITHERCLAW));
        assert_eq!(ctx.current_might(WITHERCLAW), 6);
        assert!(activate::offers(&ctx, 0)
            .iter()
            .all(|offer| offer.source != WITHERCLAW));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn dying_empowered_channels_two_runes_exhausted_when_the_deathknell_resolves() {
        let mut fixture = tomb();
        let mut ctx = fixture.ctx();
        empowered_by_paying(&mut ctx);
        let (runes, exhausted) = pool_of(&ctx, 0);
        assert_eq!(ctx.kill(WITHERCLAW, Cause::Item(9)), Killed::Yes);
        assert!(ctx.in_trash(WITHERCLAW));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: DEATH } if source == WITHERCLAW
        ));
        assert_eq!(pool_of(&ctx, 0).0, runes, "the channel waits for the chain");
        resolve_all(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            pool_of(&ctx, 0),
            (runes + 2, exhausted + 2),
            "two more runes, both exhausted"
        );
        assert_eq!(pool_of(&ctx, 1).0, 2, "the opponent channels nothing");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 2 runes exhausted".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn dying_unempowered_is_no_deathknell_at_all() {
        let mut fixture = tomb();
        let mut ctx = fixture.ctx();
        let pool = pool_of(&ctx, 0);
        assert_eq!(ctx.kill(WITHERCLAW, Cause::Item(9)), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "727.1.c.1 · not Empowered, the Deathknell never triggers"
        );
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(pool_of(&ctx, 0), pool);
        assert!(ctx.fault.is_none());
    }
}
