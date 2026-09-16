use super::prelude::{
    a_card, activated, asking, attach_gear, card_target, done, equipment_of, named, once_each_turn,
    paying_with, swap_units, unit, with_candidates, Swapped, ANOTHER_FRIENDLY_UNIT_THAN_ME,
};
use super::{Card, Cost, Domain, Flow, Item, Power, SelfCost, Stage, Timing};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const CALM: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};
pub const QUESTION: &str = "one of its Equipment to attach to me";
pub const STAGE_EQUIPMENT: u8 = 1;

fn its_equipment(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let Some(unit) = card_target(ctx, item, 0) else {
        return Vec::new();
    };
    equipment_of(ctx, unit)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn ascend(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if stage.0 == STAGE_EQUIPMENT {
        let offered = equipment_of(ctx, unit);
        let Some(gear) = ctx
            .picks()
            .first()
            .copied()
            .filter(|gear| offered.contains(gear))
        else {
            return done();
        };
        attach_gear(ctx, gear, me);
        return done();
    }
    match swap_units(ctx, me, unit) {
        Swapped::Swapped => {}
        Swapped::SameLocation => {
            ctx.narrate(format!(
                "{{card {me}}} and {{card {unit}}} already share a location"
            ));
        }
        Swapped::Capped | Swapped::NotUnits => {
            ctx.narrate(format!(
                "{{card {me}}} and {{card {unit}}} stay where they are"
            ));
        }
    }
    if !ctx.on_board(me) || equipment_of(ctx, unit).is_empty() {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, STAGE_EQUIPMENT, 0, 1))
}

pub static CARD: Card = unit(
    "Azir - Ascendant",
    &[],
    &[asking(
        with_candidates(
            named(
                once_each_turn(paying_with(
                    activated(
                        Timing::Action,
                        CALM,
                        &[a_card(
                            ANOTHER_FRIENDLY_UNIT_THAN_ME,
                            "a unit you control to trade places with",
                        )],
                        ascend,
                    ),
                    SelfCost::Free,
                )),
                "trade places with a unit you control",
            ),
            its_equipment,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, Location};
    use crate::cards::{script_of, Once, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, attach, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const AZIR: u32 = 90;
    const SOLDIER: u32 = 91;
    const BLADE: u32 = 92;
    const SPARE: u32 = 93;
    const CALM_RUNES: [u32; 2] = [46, 47];

    fn azir(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(AZIR, zone, 0, "Azir - Ascendant", 6)
        }
    }

    fn desert(soldier_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(azir(fixtures::BASE));
        fixture
            .table
            .cards
            .push(fixtures::unit(SOLDIER, soldier_zone, 0, "Sand Soldier", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(BLADE, soldier_zone, 0, "Brutalizer", 2));
        fixture.table.cards.push(fixtures::gear(
            SPARE,
            soldier_zone,
            0,
            "Boots of Swiftness",
            2,
        ));
        for rune in CALM_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn armed(soldier_zone: u16) -> Fixture {
        let mut fixture = desert(soldier_zone);
        let mut ctx = fixture.ctx();
        assert_eq!(
            attach::attach(&mut ctx, BLADE, SOLDIER),
            attach::Attached::Yes
        );
        assert_eq!(
            attach::attach(&mut ctx, SPARE, SOLDIER),
            attach::Attached::Yes
        );
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture
    }

    fn his_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == AZIR)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_once_per_turn_calm_action_over_another_friendly_unit() {
        assert!(std::ptr::eq(script_of("Azir - Ascendant").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(ability.cost, Some(CALM));
        assert_eq!(ability.self_cost, SelfCost::Free, "no exhaust is printed");
        assert_eq!(ability.once, Once::PerTurn);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, ANOTHER_FRIENDLY_UNIT_THAN_ME);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = armed(fixtures::BF1);
        let ctx = fixture.ctx();
        assert_eq!(equipment_of(&ctx, SOLDIER), [BLADE, SPARE]);
        assert!(equipment_of(&ctx, AZIR).is_empty());
    }

    #[test]
    fn the_action_swaps_azir_with_the_chosen_unit_and_offers_one_of_its_equipment_to_take() {
        let mut fixture = armed(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            his_offers(&ctx),
            [(
                format!("{{card {AZIR}}}: trade places with a unit you control (1 Calm power)"),
                true
            )]
        );
        let runes = ctx.runes_of(0).len();
        activate::activate(&mut ctx, 0, AZIR, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {SOLDIER}}}"),
                "cancel".to_string()
            ],
            "your other units, wherever they are"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SOLDIER}}}")).unwrap();
        assert!(!ctx.card(AZIR).unwrap().exhausted, "no exhaust in the cost");
        assert_eq!(ctx.runes_of(0).len(), runes - 1, "one Calm recycled");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.location(AZIR), Some(Location::Base(0)));
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.location(AZIR),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(SOLDIER), Some(Location::Base(0)));
        assert_eq!(
            ctx.location(BLADE),
            Some(Location::Base(0)),
            "its Equipment follows the Soldier"
        );
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_EQUIPMENT
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BLADE}}}"),
                format!("{{card {SPARE}}}"),
                "skip".to_string()
            ]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {AZIR}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BLADE}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, BLADE), Some(AZIR));
        assert_eq!(
            ctx.location(BLADE),
            Some(Location::Battlefield(fixtures::BF1)),
            "the Blade follows Azir now"
        );
        assert_eq!(attached_to(&ctx, SPARE), Some(SOLDIER));
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "attach, not Equip · no cost"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BLADE}}} is attached to {{card {AZIR}}}")));
        assert_eq!(
            activate::activate(&mut ctx, 0, AZIR, 0),
            Err(Refusal::Illegal(Reason::AlreadyActivated)),
            "use only once per turn"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_leaves_the_equipment_and_an_unequipped_unit_asks_nothing() {
        let mut fixture = armed(fixtures::BF1);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, AZIR, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SOLDIER}}}")).unwrap();
        resolve_chain(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, BLADE), Some(SOLDIER));
        assert_eq!(attached_to(&ctx, SPARE), Some(SOLDIER));
        drop(ctx);

        let mut fixture = desert(fixtures::BF1);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, AZIR, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SOLDIER}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "{:?}", fixtures::labels(&ctx));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(AZIR),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(SOLDIER), Some(Location::Base(0)));
        assert!(
            !attach::is_attached(&ctx, BLADE),
            "unattached gear stays put"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_sharing_his_location_moves_nothing_but_may_still_hand_over_its_equipment() {
        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, AZIR, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SOLDIER}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.location(AZIR), Some(Location::Base(0)));
        assert_eq!(ctx.location(SOLDIER), Some(Location::Base(0)));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {AZIR}}} and {{card {SOLDIER}}} already share a location"
        )));
        fixtures::choose(&mut ctx, 0, &format!("{{card {SPARE}}}")).unwrap();
        assert_eq!(attached_to(&ctx, SPARE), Some(AZIR));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_calm_rune_or_another_unit_the_action_is_refused() {
        let mut fixture = armed(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| !CALM_RUNES.contains(&card.id) && card.id != 42);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            his_offers(&ctx)
                .iter()
                .map(|(_, enabled)| *enabled)
                .collect::<Vec<bool>>(),
            [false]
        );
        assert!(activate::activate(&mut ctx, 0, AZIR, 0).is_err());
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut fixture = desert(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| card.id != SOLDIER && card.id != fixtures::VI);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(
            his_offers(&ctx).is_empty(),
            "402.3 · no legal target, no offer"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, AZIR, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, AZIR, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
    }
}
