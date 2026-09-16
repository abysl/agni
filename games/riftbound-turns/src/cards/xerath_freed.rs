use super::prelude::{
    a_unit, activated, at_battlefield, card_target, deal, done, exhausting_self, named, unit,
    usable_if,
};
use super::{Card, Cost, Domain, Flow, Item, Power, Source, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 3;
pub const FURY: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};

fn freed(ctx: &Ctx, source: Source) -> bool {
    at_battlefield(ctx, source.card)
}

fn arcanopulse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if deal(ctx, item, unit, DAMAGE) {
        let me = item.kind.source();
        ctx.narrate(format!("{{card {me}}} deals {DAMAGE} to {{card {unit}}}"));
    }
    done()
}

pub static CARD: Card = unit(
    "Xerath - Freed",
    &[],
    &[named(
        usable_if(
            exhausting_self(activated(
                Timing::Sorcery,
                FURY,
                &[a_unit("a unit to deal 3 to")],
                arcanopulse,
            )),
            freed,
        ),
        "deal 3 to a unit",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const XERATH: u32 = 90;
    const RAIDER: u32 = 91;

    fn xerath(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(5),
            domain: vec!["Fury".into()],
            ..fixtures::unit(XERATH, zone, 0, "Xerath - Freed", 5)
        }
    }

    fn his_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == XERATH)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn fury_runes_recycled(ctx: &Ctx) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Move {
                        card,
                        zone: fixtures::RUNE_DECK,
                        seat: 0,
                        ..
                    } if [fixtures::RUNE_A, 41, 43].contains(card)
                )
            })
            .count()
    }

    fn sands(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(xerath(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF2, 1, "Raider", 3));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(XERATH).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_with_one_fury_and_exhaust_ability_usable_only_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Xerath - Freed").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let pulse = &CARD.abilities[0];
        assert_eq!(pulse.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(pulse.self_cost, SelfCost::Exhaust);
        assert_eq!(pulse.cost, Some(FURY));
        assert_eq!(pulse.targets.len(), 1);
        assert_eq!(pulse.targets[0].filter, UNIT);
        assert!(pulse.usable.is_some());
        assert_eq!(pulse.label, Some("deal 3 to a unit"));
        assert_eq!(DAMAGE, 3);
        let mut fixture = sands(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(freed(
            &ctx,
            Source {
                card: XERATH,
                ability: 0
            }
        ));
    }

    #[test]
    fn at_a_battlefield_the_pulse_exhausts_him_recycles_a_fury_rune_and_deals_three_anywhere() {
        let mut fixture = sands(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            his_offers(&ctx),
            [(
                format!("{{card {XERATH}}}: deal 3 to a unit (1 Fury power, exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, XERATH, 0).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        let mut expected = vec![
            format!("{{card {}}}", fixtures::VI),
            format!("{{card {}}}", fixtures::SPRITE),
            format!("{{card {}}}", fixtures::THEIR_UNIT),
            format!("{{card {XERATH}}}"),
            format!("{{card {RAIDER}}}"),
            "cancel".to_string(),
        ];
        expected.sort();
        assert_eq!(offered, expected, "any unit anywhere, himself included");
        fixtures::choose(&mut ctx, 0, &format!("{{card {RAIDER}}}")).unwrap();
        assert!(
            ctx.card(XERATH).unwrap().exhausted,
            "exhausting him is part of the cost"
        );
        assert_eq!(fury_runes_recycled(&ctx), 1, "{:?}", ctx.effects);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.damage_on(RAIDER), 0, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(RAIDER), "3 damage on 3 Might is lethal");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, n: 3, .. } if *card == RAIDER
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {XERATH}}} deals 3 to {{card {RAIDER}}}")));
        assert_eq!(
            activate::activate(&mut ctx, 0, XERATH, 0),
            Err(Refusal::Exhausted),
            "one pulse per ready"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn in_the_base_the_ability_is_neither_offered_nor_usable_and_without_fury_it_is_greyed() {
        let mut fixture = sands(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(
            his_offers(&ctx).is_empty(),
            "use this ability only while I'm at a battlefield"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, XERATH, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, XERATH, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(!ctx.card(XERATH).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
        drop(ctx);

        let mut poor = sands(fixtures::BF1);
        for rune in [fixtures::RUNE_A, 41, 43] {
            poor.table.card_mut(rune).unwrap().domain = vec!["Calm".into()];
        }
        poor.resolve();
        let mut ctx = poor.ctx();
        assert_eq!(
            his_offers(&ctx),
            [(
                format!("{{card {XERATH}}}: deal 3 to a unit (1 Fury power, exhaust)"),
                false
            )],
            "listed, greyed"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, XERATH, 0),
            Err(Refusal::NoPowerOf)
        );
        assert!(ctx.blob.queue.is_empty());
    }
}
