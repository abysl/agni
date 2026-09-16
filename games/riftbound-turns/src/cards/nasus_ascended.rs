use super::prelude::{done, empower, on_conquer_me, score_point, unit, when};
use super::{Card, Cost, Event, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::Ctx;

pub const DEFLECT: u8 = 2;
pub const EMPOWER: Cost = Cost {
    energy: 8,
    power: &[],
};

fn empowered(ctx: &Ctx, _: &Event, source: Source) -> bool {
    ctx.is_empowered(source.card)
}

fn ascend(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

pub static CARD: Card = unit(
    "Nasus, Ascended",
    &[Keyword::Deflect(DEFLECT), Keyword::Empower(EMPOWER)],
    &[
        empower(EMPOWER),
        when(on_conquer_me(&[], ascend), empowered),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Timing, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, settle};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const NASUS: u32 = 90;
    const EXTRA_RUNES: [u32; 5] = [46, 47, 48, 49, 50];

    fn nasus(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(8),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(NASUS, zone, seat, "Nasus, Ascended", 8)
        }
    }

    fn tomb(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(nasus(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        let mut count = 0;
        for card in fixture.table.cards.iter_mut() {
            if card.is_kind("Rune") && card.owner == 0 {
                card.exhausted = count >= ready;
                count += 1;
            }
        }
        fixture.resolve();
        fixture
    }

    fn his_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == NASUS)
            .collect()
    }

    fn resolve_all(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_champion_prints_deflect_and_empower_and_scores_on_conquer_only_while_empowered() {
        let fixture = tomb(8);
        assert!(std::ptr::eq(fixture.scripts.of_card(NASUS).unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Deflect(0)));
        assert!(CARD.has_keyword(Keyword::Empower(Cost::FREE)));
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        assert!(empower.targets.is_empty());
        assert_eq!(empower.label, Some("empower"));
        let conquer = &CARD.abilities[1];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(conquer.condition.is_some());
        assert!(!conquer.optional);
    }

    #[test]
    fn empower_is_offered_once_pays_eight_and_is_refused_after() {
        let mut fixture = tomb(8);
        let mut ctx = fixture.ctx();
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {NASUS}}}: empower (8 energy)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, NASUS, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target, no confirm");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "eight energy from eight runes"
        );
        assert!(
            !ctx.card(NASUS).unwrap().exhausted,
            "827.1 · Empower never exhausts him"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.is_empowered(NASUS), "nothing until it resolves");
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(NASUS));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Empowered { card, .. } if *card == NASUS))
                .count(),
            1
        );
        assert!(
            his_offers(&ctx).is_empty(),
            "377.2.b · the offer is gone, not greyed"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, NASUS, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
    }

    #[test]
    fn seven_ready_runes_grey_the_offer_and_refuse_the_activation() {
        let mut fixture = tomb(7);
        let mut ctx = fixture.ctx();
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].enabled);
        assert_eq!(
            activate::activate(&mut ctx, 0, NASUS, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 8,
                ready: 7
            })
        );
        assert!(!ctx.is_empowered(NASUS));
    }

    #[test]
    fn a_conquer_scores_a_point_only_while_he_is_empowered() {
        let mut fixture = tomb(8);
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![NASUS],
        });
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "727.1.c.1 · not empowered, no trigger"
        );
        assert_eq!(ctx.points(0), 0);
        activate::activate(&mut ctx, 0, NASUS, 0).unwrap();
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(NASUS));
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![NASUS],
        });
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the conquer trigger waits on the chain"
        );
        assert_eq!(ctx.points(0), 0);
        resolve_all(&mut ctx);
        assert_eq!(ctx.points(0), 1, "an unrestricted point");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} scores 1 point".to_string()));
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![fixtures::VI],
        });
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "another unit's conquer is not his"
        );
    }
}
