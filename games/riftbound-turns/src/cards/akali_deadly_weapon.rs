use super::prelude::{
    asking, choosable, deal, done, empower, is_empowered, on_move, optional, pay_deflect, unit,
    with_candidates, with_statics, Location,
};
use super::{Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, Static};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const EMPOWER: Cost = Cost {
    energy: 2,
    power: &[Power::Domain(Domain::Fury)],
};
pub const DAMAGE: u8 = 1;
pub const EMPOWERED_DAMAGE: u8 = 2;
pub const MIGHT: i16 = 1;
pub const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str =
    "a unit at a battlefield she moved to or from to deal 1, or 2 while she is Empowered";

pub fn battlefields_moved_to_or_from(ctx: &Ctx, item: &Item) -> Vec<u16> {
    let me = item.kind.source();
    let from = item
        .noted
        .and_then(|noted| Location::of_zone(noted.zone, item.controller, &ctx.zones))
        .and_then(Location::battlefield);
    let to = ctx.location(me).and_then(Location::battlefield);
    let mut zones: Vec<u16> = from.into_iter().chain(to).collect();
    zones.sort_unstable();
    zones.dedup();
    zones
}

pub fn units_moved_past(ctx: &Ctx, item: &Item) -> Vec<u32> {
    battlefields_moved_to_or_from(ctx, item)
        .into_iter()
        .flat_map(|zone| ctx.units_at(Location::Battlefield(zone)))
        .collect()
}

pub fn units_she_may_strike(ctx: &Ctx, item: &Item) -> Vec<u32> {
    units_moved_past(ctx, item)
        .into_iter()
        .filter(|unit| choosable(ctx, item, *unit))
        .collect()
}

pub fn damage_dealt_by(ctx: &Ctx, akali: u32) -> u8 {
    if is_empowered(ctx, akali) {
        EMPOWERED_DAMAGE
    } else {
        DAMAGE
    }
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != STAGE_PICKED {
        return Vec::new();
    }
    units_she_may_strike(ctx, item)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn strike(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_PICKED => {
            let offered = units_she_may_strike(ctx, item);
            let Some(unit) = ctx
                .picks()
                .first()
                .copied()
                .filter(|unit| offered.contains(unit))
            else {
                ctx.narrate(format!("{{card {me}}} strikes nobody"));
                return done();
            };
            if !pay_deflect(ctx, item, unit) {
                ctx.narrate(format!("{{card {me}}} strikes nobody"));
                return done();
            }
            let amount = damage_dealt_by(ctx, me);
            if deal(ctx, item, unit, amount) {
                ctx.narrate(format!("{{card {unit}}} takes {amount}"));
            }
            done()
        }
        _ => {
            if units_she_may_strike(ctx, item).is_empty() {
                ctx.narrate(format!("{{card {me}}} passes no unit to strike"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = with_statics(
    unit(
        "Akali, Deadly Weapon",
        &[Keyword::Empower(EMPOWER)],
        &[
            empower(EMPOWER),
            asking(
                with_candidates(optional(on_move(&[], strike)), candidates),
                QUESTION,
            ),
        ],
    ),
    &[Static::While(is_empowered, &[Grant::Might(MIGHT)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, SelfCost, Timing, Trigger, Where, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, march, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const AKALI: u32 = 90;
    const ALLY: u32 = 91;
    const BYSTANDER: u32 = 92;
    const THIRD_FIELD: u32 = 93;
    const FURY_RUNE: u32 = 46;

    fn akali(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Fury".into()],
            ..fixtures::unit(AKALI, zone, seat, "Akali, Deadly Weapon", 3)
        }
    }

    fn shadows(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(akali(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(BYSTANDER, fixtures::BF3, 0, "Bystander", 4));
        fixture.table.cards.push(fixtures::card(
            THIRD_FIELD,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture
            .table
            .cards
            .push(fixtures::rune(FURY_RUNE, 0, "Fury", false));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_holder(fixtures::BF3, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(AKALI).unwrap(), &CARD));
        fixture
    }

    fn strike_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 1 } if source == AKALI))
            .count()
    }

    fn walk(ctx: &mut Ctx, from: Location, to: Location) {
        march::standard_move(ctx, 0, AKALI, from, to);
        settle(ctx).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::GroupMove { .. })) {
            fixtures::choose(ctx, 0, "done").unwrap();
        }
        assert_eq!(strike_items(ctx), 1, "the move trigger waits on the chain");
        assert!(ctx.blob.prompt.is_none(), "the pick comes at resolution");
        pass_until_parked(ctx);
    }

    fn empower_her(ctx: &mut Ctx) {
        activate::activate(ctx, 0, AKALI, 0).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(AKALI));
    }

    #[test]
    fn the_script_prints_empower_a_may_move_strike_and_one_might_while_empowered() {
        assert!(std::ptr::eq(
            script_of("Akali, Deadly Weapon").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(EMPOWER.energy, 2);
        assert_eq!(EMPOWER.power, [Power::Domain(Domain::Fury)]);
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        let strike = &CARD.abilities[1];
        assert_eq!(
            strike.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(strike.optional);
        assert!(strike.targets.is_empty());
        assert!(strike.candidates.is_some());
        assert_eq!(strike.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Might(MIGHT)])]
        ));
        assert_eq!((DAMAGE, EMPOWERED_DAMAGE), (1, 2));
    }

    #[test]
    fn moving_to_a_battlefield_offers_the_units_there_and_the_pick_takes_one() {
        let mut fixture = shadows(fixtures::BASE);
        let action = fixtures::move_action(AKALI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        walk(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        let item = ctx.blob.chain[0].clone();
        assert_eq!(battlefields_moved_to_or_from(&ctx, &item), [fixtures::BF1]);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: item.id,
                stage: STAGE_PICKED
            })
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            ["skip".to_string(), format!("{{card {AKALI}}}"), format!("{{card {ALLY}}}")],
            "the units at the battlefield she moved to, herself included; the bystander elsewhere is not offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {AKALI}}}: choose {QUESTION} (0 of 1)")
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "her controller picks"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(ALLY), 1);
        assert_eq!(ctx.damage_on(AKALI), 0);
        assert!(ctx.blob.log.contains(&format!("{{card {ALLY}}} takes 1")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_walk_home_reads_the_battlefield_she_left_and_skip_strikes_nobody() {
        let mut fixture = shadows(fixtures::BF3);
        let action = fixtures::move_action(AKALI, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        walk(
            &mut ctx,
            Location::Battlefield(fixtures::BF3),
            Location::Base(0),
        );
        let item = ctx.blob.chain[0].clone();
        assert_eq!(
            battlefields_moved_to_or_from(&ctx, &item),
            [fixtures::BF3],
            "the base she moved to is no battlefield; the one she left is"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BYSTANDER}}}"), "skip".to_string()]
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(BYSTANDER), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {AKALI}}} strikes nobody")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn empowered_she_has_one_more_might_and_deals_two() {
        let mut fixture = shadows(fixtures::BASE);
        let action = fixtures::move_action(AKALI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.current_might(AKALI), 3);
        assert_eq!(damage_dealt_by(&ctx, AKALI), DAMAGE);
        empower_her(&mut ctx);
        assert_eq!(ctx.current_might(AKALI), 3 + i32::from(MIGHT));
        assert_eq!(damage_dealt_by(&ctx, AKALI), EMPOWERED_DAMAGE);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "two energy, one of them the Fury"
        );
        walk(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert_eq!(ctx.damage_on(ALLY), 2, "If I'm [Empowered], deal 2 instead");
        assert!(ctx.blob.log.contains(&format!("{{card {ALLY}}} takes 2")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_nobody_to_strike_the_trigger_finishes_without_asking_and_a_recall_is_not_a_move() {
        let mut fixture = shadows(fixtures::BASE);
        fixture.table.cards.retain(|card| card.id != ALLY);
        fixture.table.card_mut(AKALI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let action = fixtures::move_action(AKALI, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            AKALI,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::GroupMove { .. })),
            "the ready bystander could walk home with her"
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(strike_items(&ctx), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {AKALI}}} passes no unit to strike")));
        drop(ctx);

        let mut fixture = shadows(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.recall(AKALI, true);
        settle(&mut ctx).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert_eq!(ctx.location(AKALI), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "434.1 · a recall is not a move");
    }
}

#[cfg(test)]
mod deflect {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, settle};
    use crate::state::{Expiry, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const AKALI: u32 = 90;
    const DEFLECTOR: u32 = 94;
    const RUNE: u32 = 46;

    fn akali(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Fury".into()],
            ..fixtures::unit(AKALI, zone, seat, "Akali, Deadly Weapon", 3)
        }
    }

    fn a_deflector(runes: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(akali(fixtures::BASE, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(DEFLECTOR, fixtures::BF1, 1, "Deflector", 4));
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1);
        for rune in 0..runes {
            fixture.table.cards.push(fixtures::rune(
                RUNE + u32::try_from(rune).unwrap(),
                0,
                "Fury",
                false,
            ));
        }
        fixture
            .blob
            .card_state_mut(DEFLECTOR)
            .granted
            .push((Keyword::Deflect(1), Expiry::Permanent));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn walk_in(ctx: &mut Ctx) {
        assert_eq!(ctx.deflect_of(DEFLECTOR), 1);
        march::standard_move(
            ctx,
            0,
            AKALI,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        settle(ctx).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::GroupMove { .. })) {
            fixtures::choose(ctx, 0, "done").unwrap();
        }
        pass_until_parked(ctx);
    }

    #[test]
    fn the_deflect_enemy_is_not_offered_while_her_controller_cannot_pay_the_tax() {
        let mut fixture = a_deflector(0);
        let action = fixtures::move_action(AKALI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "no rune to pay the Deflect"
        );
        walk_in(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {AKALI}}}"), "skip".to_string()],
            "herself, never the unaffordable Deflector"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(DEFLECTOR), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_a_rune_the_deflect_enemy_is_offered_and_the_pick_pays_the_tax_before_the_strike() {
        let mut fixture = a_deflector(1);
        let action = fixtures::move_action(AKALI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        walk_in(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                stage: STAGE_PICKED,
                ..
            })
        ));
        assert!(fixtures::labels(&ctx).contains(&format!("{{card {DEFLECTOR}}}")));
        fixtures::choose(&mut ctx, 0, &format!("{{card {DEFLECTOR}}}")).unwrap();
        assert_eq!(ctx.damage_on(DEFLECTOR), 1);
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "the rune paid the Deflect"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} pays 1 any power for the Deflect of {{card {DEFLECTOR}}}"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · 355.5.b and 383.3.a: the unit she strikes is a target chosen as the trigger is finalized, when the Deflect is paid and opponents react knowing it; today the pick is a Resume at resolution that offers only affordable, targetable units and pays the Deflect on the pick"]
    fn the_unit_is_chosen_and_the_deflect_paid_as_the_trigger_is_finalized() {
        let mut fixture = a_deflector(1);
        let action = fixtures::move_action(AKALI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        walk_in(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
    }
}
