use super::faithful_manufactor::play_recruits;
use super::prelude::{deathknell, done, empower, unit, when, when_empowered, Location};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Order)],
};
pub const RECRUITS: usize = 2;

fn last_orders(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_recruits(ctx, seat, Location::Base(seat), RECRUITS);
    done()
}

pub static CARD: Card = unit(
    "Noxian Emissary",
    &[Keyword::Empower(EMPOWER), Keyword::Deathknell],
    &[
        empower(EMPOWER),
        when(deathknell(&[], last_orders), when_empowered),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::recruits_of;
    use crate::cards::faithful_manufactor::RECRUIT_MIGHT;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority, settle};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const EMISSARY: u32 = 90;
    const ORDER_RUNE: u32 = 46;
    const DEATH: u8 = 1;

    fn emissary() -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Order".into()],
            ..fixtures::unit(EMISSARY, fixtures::BF1, 0, "Noxian Emissary", 2)
        }
    }

    fn embassy(with_order: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(emissary());
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        if with_order {
            fixture
                .table
                .cards
                .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(EMISSARY).unwrap(),
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

    #[test]
    fn the_script_prints_empower_and_deathknell_and_the_deathknell_is_conditional() {
        assert!(std::ptr::eq(script_of("Noxian Emissary").unwrap(), &CARD));
        assert_eq!(
            CARD.keywords,
            [Keyword::Empower(EMPOWER), Keyword::Deathknell]
        );
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 1);
        assert_eq!(EMPOWER.power, [Power::Domain(Domain::Order)]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        let death = &CARD.abilities[usize::from(DEATH)];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.condition.is_some(), "while Empowered");
        assert!(death.targets.is_empty());
        assert!(!death.optional);
        assert_eq!(RECRUITS, 2);
        assert_eq!(RECRUIT_MIGHT, 1);
    }

    #[test]
    fn dying_empowered_plays_two_recruits_to_the_base_when_the_deathknell_resolves() {
        let mut fixture = embassy(true);
        let mut ctx = fixture.ctx();
        let runes = ctx.runes_of(0).len();
        activate::activate(&mut ctx, 0, EMISSARY, 0).unwrap();
        assert_eq!(ctx.runes_of(0).len(), runes - 1, "the Order rune recycled");
        assert!(!ctx.card(EMISSARY).unwrap().exhausted);
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(EMISSARY));
        assert!(recruits_of(&ctx, 0).is_empty());
        assert_eq!(ctx.kill(EMISSARY, Cause::Combat), Killed::Yes);
        assert!(ctx.in_trash(EMISSARY));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: DEATH } if source == EMISSARY
        ));
        assert!(
            recruits_of(&ctx, 0).is_empty(),
            "the Recruits wait for the chain"
        );
        resolve_all(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits.len(), 2);
        for recruit in &recruits {
            assert_eq!(ctx.location(*recruit), Some(Location::Base(0)));
            assert!(ctx.is_unit(*recruit));
            assert_eq!(ctx.current_might(*recruit), 1);
            assert!(
                ctx.card(*recruit).unwrap().exhausted,
                "played without Accelerate, they enter exhausted"
            );
        }
        assert!(recruits_of(&ctx, 1).is_empty(), "your base, your Recruits");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn dying_unempowered_plays_nothing() {
        let mut fixture = embassy(true);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(EMISSARY, Cause::Combat), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "727.1.c.1 · not Empowered, the Deathknell never triggers"
        );
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_an_order_rune_the_empower_is_refused() {
        let mut fixture = embassy(false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, EMISSARY, 0),
            Err(Refusal::NoPowerOf)
        );
        assert!(!ctx.is_empowered(EMISSARY));
        assert_eq!(
            activate::activate(&mut ctx, 1, EMISSARY, 0),
            Err(Refusal::Illegal(crate::engine::legal::Reason::NotYourCard))
        );
    }
}
