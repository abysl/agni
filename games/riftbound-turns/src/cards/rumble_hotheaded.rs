use super::divine_judgment::recycle;
use super::prelude::{
    asking, card_target, done, on_conquer_me, optional, target, unit, with_candidates,
    with_statics, Location, ANOTHER_FRIENDLY_UNIT_THAN_ME,
};
use super::rumble_mechanized_menace::{your_mech, MECH};
use super::{
    Card, Filter, Flow, Grant, Item, Keyword, Paying, Scope, Stage, Static, TargetKind, TargetSpec,
    KIND_UNIT,
};
use crate::engine::ctx::Ctx;
use crate::engine::{cost, pay, play as play_engine, targets};
use crate::state::{Leave, Origin, TargetRef};

pub const ASSAULT: u8 = 1;
pub const QUESTION: &str = "a Mech in your trash to play, then where it is played";
pub const STAGE_PICK: u8 = 1;
pub const STAGE_LOCATE: u8 = 2;
pub const FROM_THE_TRASH: Origin = Origin::Trash {
    leave: Leave::Recycle,
};

pub const UNIT_TO_RECYCLE: TargetSpec = target(
    ANOTHER_FRIENDLY_UNIT_THAN_ME,
    0,
    1,
    TargetKind::Card,
    "another friendly unit to recycle · skip to keep them",
);

pub const MECH_IN_YOUR_TRASH: TargetSpec = target(
    Filter::And(&[
        Filter::Kind(KIND_UNIT),
        Filter::InTrash,
        Filter::Friendly,
        MECH,
    ]),
    0,
    1,
    TargetKind::Card,
    "a Mech in your trash",
);

pub fn reduction_for(ctx: &Ctx, recycled: u32) -> u8 {
    u8::try_from(ctx.current_might(recycled).max(0)).unwrap_or(u8::MAX)
}

pub fn reduced_price(ctx: &Ctx, mech: u32, reduction: u8) -> cost::Cost {
    let printed = cost::printed_of(ctx, mech, false);
    cost::Cost {
        energy: printed.energy.saturating_sub(reduction),
        ..printed
    }
}

pub fn playable_mechs(ctx: &Ctx, item: &Item, reduction: u8) -> Vec<u32> {
    targets::candidates(ctx, item, &MECH_IN_YOUR_TRASH)
        .into_iter()
        .filter_map(|mech| match mech {
            TargetRef::Card(mech) => Some(mech),
            _ => None,
        })
        .filter(|mech| {
            let play = cost::play_item(ctx, item.controller, *mech, FROM_THE_TRASH);
            pay::affordable_for(
                ctx,
                item.controller,
                &reduced_price(ctx, *mech, reduction),
                Paying::Item(&play),
            )
        })
        .collect()
}

fn recycled_unit(ctx: &Ctx, item: &Item) -> Option<u32> {
    card_target(ctx, item, 0).filter(|unit| ctx.on_board(*unit))
}

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 >= STAGE_LOCATE {
        return location_options(ctx, item.controller);
    }
    let Some(unit) = recycled_unit(ctx, item) else {
        return Vec::new();
    };
    playable_mechs(ctx, item, reduction_for(ctx, unit))
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub fn recycle_a_unit_as_the_base_cost_until_the_pay_stage_takes_it_at_finalization(
    ctx: &mut Ctx,
    item: &Item,
    unit: u32,
) -> Option<u8> {
    if !ctx.on_board(unit) || ctx.controller(unit) != item.controller {
        return None;
    }
    let reduction = reduction_for(ctx, unit);
    recycle(ctx, unit);
    ctx.narrate(format!(
        "{{seat {}}} recycles {{card {unit}}} · {reduction} Might off the Mech's Energy",
        item.controller
    ));
    Some(reduction)
}

fn salvage(ctx: &mut Ctx, item: &Item, mech: u32, at: Location) -> bool {
    let seat = item.controller;
    let Some(unit) = recycled_unit(ctx, item) else {
        ctx.narrate(format!(
            "{{card {}}} · the unit to recycle has left the board",
            item.kind.source()
        ));
        return false;
    };
    let price = reduced_price(ctx, mech, reduction_for(ctx, unit));
    let play = cost::play_item(ctx, seat, mech, FROM_THE_TRASH);
    let Ok(plan) = pay::plan_for(ctx, seat, &price, Paying::Item(&play)) else {
        ctx.narrate(format!(
            "{{card {mech}}} stays in the trash · its cost can't be paid"
        ));
        return false;
    };
    let Some(_) = recycle_a_unit_as_the_base_cost_until_the_pay_stage_takes_it_at_finalization(
        ctx, item, unit,
    ) else {
        return false;
    };
    pay::pay(ctx, seat, &plan);
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {mech}}} from the trash for {}",
        price.label()
    ));
    play_engine::begin(ctx, seat, mech, FROM_THE_TRASH, Some(at)).is_ok()
}

fn flame_spitter(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    let Some(unit) = recycled_unit(ctx, item) else {
        if matches!(item.targets.first(), Some(TargetRef::Card(_))) {
            ctx.narrate(format!(
                "{{card {}}} · the unit to recycle has left the board",
                item.kind.source()
            ));
        }
        return done();
    };
    let mechs = playable_mechs(ctx, item, reduction_for(ctx, unit));
    if stage.0 >= STAGE_LOCATE {
        let Some(mech) = mechs.get(usize::from(stage.0 - STAGE_LOCATE)).copied() else {
            return done();
        };
        let Some(at) = ctx
            .picks()
            .first()
            .and_then(|zone| u16::try_from(*zone).ok())
            .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            .filter(|at| ctx.play_locations(seat).contains(at))
        else {
            return done();
        };
        salvage(ctx, item, mech, at);
        return done();
    }
    if stage.0 == STAGE_PICK {
        let Some(index) = ctx
            .picks()
            .first()
            .and_then(|mech| mechs.iter().position(|held| held == mech))
        else {
            return done();
        };
        let locations = ctx.play_locations(seat);
        let Ok(stage) = u8::try_from(index).map(|index| index.saturating_add(STAGE_LOCATE)) else {
            return done();
        };
        if locations.len() > 1 {
            return Flow::Ask(ctx.ask_resume(item, stage, 1, 1));
        }
        salvage(ctx, item, mechs[index], locations[0]);
        return done();
    }
    if mechs.is_empty() {
        ctx.narrate(format!(
            "{{card {}}} finds no Mech to play · {{card {unit}}} stays",
            item.kind.source()
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, STAGE_PICK, 1, 1))
}

pub static CARD: Card = with_statics(
    unit(
        "Rumble - Hotheaded",
        &[],
        &[asking(
            with_candidates(
                optional(on_conquer_me(&[UNIT_TO_RECYCLE], flame_spitter)),
                candidates,
            ),
            QUESTION,
        )],
    ),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: your_mech,
        grants: &[Grant::Keyword(Keyword::Assault(ASSAULT))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle, statics};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const RUMBLE: u32 = 90;
    const SCRAP: u32 = 91;
    const MEGA: u32 = 92;
    const SPARK: u32 = 93;
    const THEIR_MECH: u32 = 94;
    const JAMMER: u32 = 95;

    fn trashed_mech(id: u32, seat: u8, name: &str, energy: u8, power: u8) -> CardInfo {
        CardInfo {
            energy: Some(energy),
            power: Some(power),
            ..fixtures::unit(id, fixtures::TRASH, seat, name, 3)
        }
    }

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut rumble = fixtures::unit(RUMBLE, fixtures::BF1, 0, "Rumble - Hotheaded", 4);
        rumble.energy = Some(4);
        fixture.table.cards.push(rumble);
        fixture
            .table
            .cards
            .push(fixtures::unit(SCRAP, fixtures::BF1, 0, "Scrap", 2));
        fixture
            .table
            .cards
            .push(trashed_mech(MEGA, 0, "Mega-Mech", 5, 1));
        fixture
            .table
            .cards
            .push(fixtures::spell(SPARK, fixtures::TRASH, 0, "Spark", 1, 0));
        fixture
            .table
            .cards
            .push(trashed_mech(THEIR_MECH, 1, "Adaptatron", 1, 0));
        fixture
            .table
            .cards
            .push(trashed_mech(JAMMER, 0, "Gem Jammer", 2, 0));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RUMBLE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_an_assault_aura_over_your_mechs_and_a_may_conquer_trigger_over_a_friendly_unit(
    ) {
        assert!(std::ptr::eq(
            script_of("Rumble - Hotheaded").unwrap(),
            &CARD
        ));
        assert!(
            CARD.keywords.is_empty(),
            "Assault is the aura's, not printed"
        );
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Keyword(Keyword::Assault(1))],
                ..
            }]
        ));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::Me));
        assert!(ability.optional);
        assert_eq!(ability.targets, [UNIT_TO_RECYCLE]);
        assert_eq!((UNIT_TO_RECYCLE.min, UNIT_TO_RECYCLE.max), (0, 1));
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn your_mechs_including_rumble_have_assault_one() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(fixtures::unit(95, fixtures::BASE, 0, "Bubble Bot", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(96, fixtures::BASE, 1, "Bubble Bot", 3));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(RUMBLE, Keyword::Assault(1)), "including me");
        assert!(matches!(
            statics::grants_on(&ctx, 95).as_slice(),
            [Grant::Keyword(Keyword::Assault(1))]
        ));
        assert!(!ctx.has_keyword(SCRAP, Keyword::Assault(1)), "no Mech");
        assert!(!ctx.has_keyword(96, Keyword::Assault(1)), "not yours");
    }

    #[test]
    fn the_reduction_is_the_recycled_units_might_and_only_affordable_mechs_are_offered() {
        let mut fixture = arena();
        let ctx = fixture.ctx();
        assert_eq!(reduction_for(&ctx, SCRAP), 2);
        assert_eq!(reduction_for(&ctx, RUMBLE), 4);
        let printed = cost::printed_of(&ctx, MEGA, false);
        assert_eq!(printed.energy, 5);
        assert_eq!(reduced_price(&ctx, MEGA, 2).energy, 3);
        assert_eq!(reduced_price(&ctx, MEGA, 9).energy, 0, "never below free");
        assert_eq!(
            reduced_price(&ctx, MEGA, 2).power,
            printed.power,
            "Power stays"
        );
        let item = Item::new(
            7,
            ItemKind::Trigger {
                source: RUMBLE,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(playable_mechs(&ctx, &item, 2), [MEGA, JAMMER]);
        assert_eq!(
            playable_mechs(&ctx, &item, 0),
            [JAMMER],
            "five energy on three ready runes is too much without the reduction"
        );
    }

    #[test]
    fn conquering_offers_another_friendly_unit_then_the_playable_mechs_and_plays_one_cheaper() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 91}", "skip"],
            "Vi and the Scrap · never Rumble himself, never the Sprite"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the may is the target · no cost confirm"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == RUMBLE
        ));
        assert!(ctx.on_board(SCRAP), "the recycle waits for resolution");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_PICK
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 95}"],
            "the Mega-Mech at 3 and the Jammer · not the Spark, not their Adaptatron, no skip"
        );
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_LOCATE
            }),
            "the conquest made {{zone 9}} a play location beside the base"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ]
        );
        assert!(
            ctx.on_board(SCRAP),
            "not recycled before the location is known"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BASE)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.on_board(SCRAP), "recycled");
        assert!(ctx.effects.contains(&Effect::Move {
            card: SCRAP,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert_eq!(ctx.location(MEGA), Some(Location::Base(0)));
        assert!(ctx.card(MEGA).unwrap().exhausted);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 3,
            "five less the Scrap's two Might: three energy"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Trash { leave: Leave::Recycle }, .. }
                if *card == MEGA
        )));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} recycles {card 91} · 2 Might off the Mech's Energy".to_string()));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("{seat 0} plays {card 92} from the trash for 3 energy")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_the_unit_recycles_nothing_and_a_lone_affordable_mech_is_played_unasked() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger still resolves");
        resolve_top(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.on_board(SCRAP));
        assert!(ctx.in_trash(MEGA) && ctx.in_trash(JAMMER));
        drop(ctx);
        let mut fixture = arena();
        fixture.table.cards.retain(|card| card.id != JAMMER);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        resolve_top(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_LOCATE
            }),
            "one Mech: picked unasked, only the location is a question"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BASE)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.on_board(SCRAP), "the cost is paid");
        assert_eq!(ctx.location(MEGA), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_affordable_mech_or_a_unit_gone_at_resolution_nothing_happens() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .retain(|card| card.id != MEGA && card.id != JAMMER);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        resolve_top(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.on_board(SCRAP));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} finds no Mech to play · {card 91} stays".to_string()));
        drop(ctx);
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        ctx.kill(SCRAP, crate::engine::ctx::Cause::Rule);
        resolve_top(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.in_trash(MEGA), "no unit to recycle, no Mech");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} · the unit to recycle has left the board".to_string()));
    }

    #[test]
    fn a_held_battlefield_is_asked_for_as_the_mechs_location() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().name = "Proving Grounds".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_LOCATE
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1),
                format!("{{zone {}}}", fixtures::BF2)
            ],
            "the base, the conquered battlefield and the one Vi holds · a Rockfall Path admits no unit"
        );
        assert!(
            ctx.on_board(SCRAP),
            "not recycled before the location is known"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF2)).unwrap();
        assert_eq!(
            ctx.location(MEGA),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(!ctx.on_board(SCRAP));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · 383.3.b makes 'recycle another friendly unit' the base cost of the trigger, paid at finalization by a SelfCost::RecycleTarget the engine lacks; the script recycles on resolution, so a unit killed in response cancels the play instead of the cost having been paid"]
    fn the_unit_is_recycled_at_finalization_before_anyone_may_respond() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert!(
            !ctx.on_board(SCRAP),
            "the cost is paid as the trigger is finalized"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: SCRAP,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
    }
}
