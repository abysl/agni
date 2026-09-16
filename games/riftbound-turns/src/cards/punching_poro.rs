use super::prelude::{
    activated, ask_discard, done, is_empowered, named, paying_with, unit, usable_if, with_statics,
};
use super::{Card, Cost, Flow, Grant, Item, Keyword, SelfCost, Source, Stage, Static, Timing};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost::FREE;
pub const DISCARDS: usize = 1;
pub const DISCARDED: u8 = 1;
pub const MIGHT: i16 = 1;

pub fn discard_cost_payable(ctx: &Ctx, seat: u8) -> bool {
    ctx.hand_of(seat).len() >= DISCARDS
}

pub fn discarded_as_the_empower_cost_until_the_pay_stage_asks_for_it(
    ctx: &Ctx,
    source: Source,
) -> bool {
    !ctx.is_empowered(source.card) && discard_cost_payable(ctx, ctx.controller(source.card))
}

fn punch(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    if stage.0 != DISCARDED {
        return match ask_discard(ctx, item, DISCARDED) {
            Some(ask) => Flow::Ask(ask),
            None => {
                ctx.narrate(format!(
                    "{{card {me}}} has nothing to discard · it is not Empowered"
                ));
                done()
            }
        };
    }
    ctx.empower_by(me, item.controller);
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Punching Poro",
        &[Keyword::Empower(EMPOWER)],
        &[usable_if(
            named(
                paying_with(
                    activated(Timing::Sorcery, EMPOWER, &[], punch),
                    SelfCost::Free,
                ),
                "empower",
            ),
            discarded_as_the_empower_cost_until_the_pay_stage_asks_for_it,
        )],
    ),
    &[Static::While(is_empowered, &[Grant::Might(MIGHT)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::state::{ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const PORO: u32 = 90;

    fn poro() -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Fury".into()],
            ..fixtures::unit(PORO, fixtures::BASE, 0, "Punching Poro", 2)
        }
    }

    fn ring(with_hand: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poro());
        if !with_hand {
            fixture
                .table
                .cards
                .retain(|card| !(card.zone == Some(fixtures::HAND) && card.owner == 0));
        }
        fixture.resolve();
        fixture
    }

    fn his_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == PORO)
            .collect()
    }

    #[test]
    fn the_script_prints_a_free_empower_whose_discard_is_the_seam_and_grants_one_might() {
        assert!(std::ptr::eq(script_of("Punching Poro").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(Cost::FREE)]);
        assert_eq!(
            CARD.empower_cost(),
            Some(Cost::FREE),
            "a discard is no resource · cards::Cost is energy and power"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(Cost::FREE));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(empower.targets.is_empty());
        assert!(empower.usable.is_some());
        assert_eq!(DISCARDS, 1);
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(1)])]
        ));
    }

    #[test]
    fn empowering_it_discards_one_at_resolution_and_it_stands_at_three_might() {
        let mut fixture = ring(true);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let runes = ctx.ready_runes_of(0).len();
        assert!(discard_cost_payable(&ctx, 0));
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].label, format!("{{card {PORO}}}: empower"));
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, PORO, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.ready_runes_of(0).len(), runes, "no rune is spent");
        assert!(
            !ctx.card(PORO).unwrap().exhausted,
            "827.1 · Empower never exhausts it"
        );
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the discard waits for resolution"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: DISCARDED
            })
        );
        assert_eq!(
            ctx.blob.chain.last().map(|top| top.status),
            Some(ItemStatus::Resolving)
        );
        assert!(!ctx.is_empowered(PORO), "nothing before the discard");
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_empowered(PORO));
        assert!(matches!(
            statics::grants_on(&ctx, PORO).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(ctx.current_might(PORO), 3);
        assert!(his_offers(&ctx).is_empty(), "377.2.b · the offer is gone");
        assert_eq!(
            activate::activate(&mut ctx, 0, PORO, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_empty_hand_withholds_the_offer_and_refuses_the_activation() {
        let mut fixture = ring(false);
        let mut ctx = fixture.ctx();
        assert!(ctx.hand_of(0).is_empty());
        assert!(!discard_cost_payable(&ctx, 0));
        assert!(
            his_offers(&ctx).is_empty(),
            "356.2.a.1 · without a card to discard the cost cannot be paid"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, PORO, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(!ctx.is_empowered(PORO));
        assert_eq!(ctx.current_might(PORO), 2);
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · a discard as an Empower cost is paid at the pay stage (355.10.c, 357), before the ability is on the chain; today the discard is asked at resolution, where an opponent's response can empty the hand and void the empowerment"]
    fn the_discard_is_paid_before_the_empower_reaches_the_chain() {
        let mut fixture = ring(true);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, PORO, 0).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1,
            "357.2 · the discard is paid before the ability exists"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.is_empowered(PORO));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(PORO));
        assert_eq!(ctx.current_might(PORO), 3);
    }
}
