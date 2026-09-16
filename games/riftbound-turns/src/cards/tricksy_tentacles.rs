use super::fox_fire::total_might;
use super::prelude::{
    asking, card_targets, done, move_destinations, move_unit, play, remember_card,
    remembered_cards, spell, target, with_candidates, Location,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;
use crate::state::TargetRef;

pub const TOTAL_MIGHT: i32 = 8;
pub const ANY_NUMBER: u8 = u8::MAX;
pub const SUBSET: u8 = 1;
pub const DESTINATION: u8 = 2;
pub const QUESTION: &str =
    "which of the chosen units still fit under 8 total Might, then the single location they move to";

pub const ENEMY_UNITS_UNDER_EIGHT_MIGHT: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::Enemy,
        Filter::Movable,
        Filter::TotalMightAtMost(8),
    ]),
    0,
    ANY_NUMBER,
    TargetKind::Card,
    "any number of enemy units with the same controller and a total Might of 8 or less",
);

pub fn with_one_controller(ctx: &Ctx, units: &[u32]) -> Vec<u32> {
    let Some(first) = units.first() else {
        return Vec::new();
    };
    let controller = ctx.controller(*first);
    units
        .iter()
        .copied()
        .filter(|unit| ctx.controller(*unit) == controller)
        .collect()
}

pub fn fits_total_might(ctx: &Ctx, chosen: &[u32], picked: &[u32]) -> Vec<u32> {
    let used = total_might(
        ctx,
        &picked
            .iter()
            .copied()
            .filter(|unit| chosen.contains(unit))
            .collect::<Vec<u32>>(),
    );
    chosen
        .iter()
        .copied()
        .filter(|unit| !picked.contains(unit))
        .filter(|unit| used + ctx.current_might(*unit) <= TOTAL_MIGHT)
        .collect()
}

pub fn single_locations_for(ctx: &Ctx, units: &[u32]) -> Vec<Location> {
    let Some(first) = units.first() else {
        return Vec::new();
    };
    let mut open = vec![Location::Base(ctx.controller(*first))];
    open.extend(
        ctx.zones
            .battlefields
            .iter()
            .copied()
            .map(Location::Battlefield),
    );
    open.retain(|to| {
        units.iter().all(|unit| {
            ctx.location(*unit) == Some(*to) || move_destinations(ctx, *unit).contains(to)
        }) && units.iter().any(|unit| ctx.location(*unit) != Some(*to))
    });
    open
}

fn group_of(ctx: &Ctx, item: &Item) -> Vec<u32> {
    let remembered: Vec<u32> = remembered_cards(item)
        .into_iter()
        .filter(|unit| ctx.on_board(*unit))
        .collect();
    if remembered.is_empty() {
        with_one_controller(ctx, &card_targets(ctx, item))
    } else {
        remembered
    }
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        SUBSET => {
            let chosen = with_one_controller(ctx, &card_targets(ctx, item));
            let picked: Vec<u32> = ctx
                .blob
                .prompt
                .as_ref()
                .map(|prompt| prompt.picked.clone())
                .unwrap_or_default();
            fits_total_might(ctx, &chosen, &picked)
                .into_iter()
                .map(TargetRef::Card)
                .collect()
        }
        DESTINATION => single_locations_for(ctx, &group_of(ctx, item))
            .into_iter()
            .filter_map(|to| ctx.zone_of(to))
            .map(|(zone, _)| TargetRef::Zone(zone))
            .collect(),
        _ => Vec::new(),
    }
}

fn travel(ctx: &mut Ctx, item: &Item, units: &[u32], to: Location) {
    ctx.narrate(format!(
        "the tentacles drag {} unit(s) to {}",
        units.len(),
        describe(to)
    ));
    for unit in units {
        move_unit(ctx, item, *unit, to);
    }
}

fn choose_destination(ctx: &mut Ctx, item: &Item, units: &[u32]) -> Flow {
    match single_locations_for(ctx, units).as_slice() {
        [] => {
            ctx.narrate(format!(
                "{{card {}}} · no single location the chosen units can move to",
                item.kind.source()
            ));
            done()
        }
        [only] => {
            travel(ctx, item, units, *only);
            done()
        }
        _ => {
            for unit in units {
                remember_card(ctx, *unit);
            }
            Flow::Ask(ctx.ask_resume(item, DESTINATION, 1, 1))
        }
    }
}

fn tentacles(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        SUBSET => {
            let chosen = with_one_controller(ctx, &card_targets(ctx, item));
            let mut kept = Vec::new();
            for unit in ctx.picks().iter().copied() {
                if chosen.contains(&unit)
                    && !kept.contains(&unit)
                    && total_might(ctx, &kept) + ctx.current_might(unit) <= TOTAL_MIGHT
                {
                    kept.push(unit);
                }
            }
            if kept.is_empty() {
                ctx.narrate(format!("{{card {}}} moves no unit", item.kind.source()));
                return done();
            }
            choose_destination(ctx, item, &kept)
        }
        DESTINATION => {
            let units = group_of(ctx, item);
            let Some(first) = units.first().copied() else {
                return done();
            };
            let picked = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, ctx.controller(first), &ctx.zones))
                .filter(|to| single_locations_for(ctx, &units).contains(to));
            if let Some(to) = picked {
                travel(ctx, item, &units, to);
            }
            done()
        }
        _ => {
            let chosen = with_one_controller(ctx, &card_targets(ctx, item));
            if chosen.is_empty() {
                ctx.narrate(format!("{{card {}}} moves no unit", item.kind.source()));
                return done();
            }
            if total_might(ctx, &chosen) <= TOTAL_MIGHT {
                return choose_destination(ctx, item, &chosen);
            }
            ctx.narrate(format!(
                "{{card {}}} · the chosen units now total more than {TOTAL_MIGHT} Might · a subset is chosen",
                item.kind.source()
            ));
            let count = u8::try_from(chosen.len()).unwrap_or(u8::MAX);
            Flow::Ask(ctx.ask_resume(item, SUBSET, 0, count))
        }
    }
}

pub static CARD: Card = spell(
    "Tricksy Tentacles",
    &[],
    &[asking(
        with_candidates(
            play(&[ENEMY_UNITS_UNDER_EIGHT_MIGHT], tentacles),
            candidates,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::might_this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const TENTACLES: u32 = 90;
    const THEIR_TENTACLES: u32 = 91;
    const FOUR: u32 = 92;
    const HEAVY: u32 = 93;
    const CALM_RUNES: [u32; 2] = [46, 47];

    fn tentacles_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Tricksy Tentacles", 4, 1);
        card.domain = vec!["Calm".into()];
        card
    }

    fn deep() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(tentacles_card(TENTACLES, 0));
        fixture.table.cards.push(tentacles_card(THEIR_TENTACLES, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(FOUR, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(HEAVY, fixtures::BASE, 1, "Colossus", 9));
        for rune in CALM_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_plain_spell_over_a_group_of_enemy_units_with_a_resume_destination() {
        assert!(std::ptr::eq(script_of("Tricksy Tentacles").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        let spec = ability.targets[0];
        assert_eq!((spec.min, spec.max), (0, ANY_NUMBER));
        assert_eq!(spec.filter, ENEMY_UNITS_UNDER_EIGHT_MIGHT.filter);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(TOTAL_MIGHT, 8);
        let mut fixture = deep();
        let ctx = fixture.ctx();
        assert_eq!(
            total_might(&ctx, &[fixtures::SPRITE, fixtures::THEIR_UNIT, FOUR]),
            9
        );
        assert_eq!(
            fits_total_might(&ctx, &[fixtures::SPRITE, fixtures::THEIR_UNIT, FOUR], &[]),
            [fixtures::SPRITE, fixtures::THEIR_UNIT, FOUR]
        );
        assert_eq!(
            fits_total_might(
                &ctx,
                &[fixtures::SPRITE, fixtures::THEIR_UNIT, FOUR],
                &[FOUR, fixtures::SPRITE]
            ),
            [],
            "4 + 3 leaves no room for the 2"
        );
        assert_eq!(
            with_one_controller(&ctx, &[fixtures::SPRITE, fixtures::VI, FOUR]),
            [fixtures::SPRITE, FOUR],
            "the first pick's controller sets the group"
        );
        assert_eq!(
            single_locations_for(&ctx, &[fixtures::SPRITE, fixtures::THEIR_UNIT]),
            [
                Location::Base(1),
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ],
            "each location at least one of them can move to and all can reach or already stand at"
        );
        assert!(single_locations_for(&ctx, &[]).is_empty());
    }

    #[test]
    fn enemies_under_eight_total_might_are_dragged_to_one_location_of_the_casters_choice() {
        let mut fixture = deep();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TENTACLES).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 60}",
                "{card 81}",
                "{card 92}",
                "done",
                "skip",
                "cancel"
            ],
            "enemy units of 8 Might or less; the 9-Might Colossus is out"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.blob.prompt.is_none(),
            "the location is asked as it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: DESTINATION
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "{zone 9}", "{zone 10}"],
            "their base, or either battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::SPRITE,
            from: Some(Location::Battlefield(fixtures::BF2)),
            to: Location::Battlefield(fixtures::BF1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert_eq!(
            ctx.location(FOUR),
            Some(Location::Battlefield(fixtures::BF1)),
            "the Brute already stood there"
        );
        assert!(ctx.blob.log.contains(&format!(
            "the tentacles drag 2 unit(s) to {{zone {}}}",
            fixtures::BF1
        )));
        assert_eq!(ctx.card(TENTACLES).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_whole_group_may_go_to_their_base_and_a_lone_destination_is_taken_without_asking() {
        let mut fixture = deep();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TENTACLES).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        assert_eq!(ctx.location(fixtures::SPRITE), Some(Location::Base(1)));
        assert_eq!(ctx.location(FOUR), Some(Location::Base(1)));
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let mut single = deep();
        single.table.card_mut(fixtures::GROUNDS).unwrap().zone = Some(fixtures::TRASH);
        single.table.card_mut(FOUR).unwrap().zone = Some(fixtures::BF2);
        single.resolve();
        let mut ctx = single.ctx();
        assert_eq!(ctx.zones.battlefields, [fixtures::BF2]);
        fixtures::play_from_hand(&mut ctx, 0, TENTACLES).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "both stand at the one battlefield, so their base is the only location"
        );
        assert_eq!(ctx.location(fixtures::SPRITE), Some(Location::Base(1)));
        assert_eq!(ctx.location(FOUR), Some(Location::Base(1)));
    }

    #[test]
    fn a_pump_in_response_forces_a_subset_that_still_fits_under_eight() {
        let mut fixture = deep();
        fixture.table.card_mut(FOUR).unwrap().might = Some(3);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TENTACLES).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, fixtures::THEIR_UNIT, 2, None);
        assert_eq!(
            total_might(&ctx, &[fixtures::SPRITE, fixtures::THEIR_UNIT, FOUR]),
            10
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: SUBSET
            }),
            "355.11.b · 3 + 4 + 3 no longer fits"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 92}", "done", "skip"]
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "done"],
            "3 used: either 3 or 4 still fits"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(
            ctx.blob.prompt.is_none()
                || ctx.blob.why
                    != Some(PromptWhy::Resume {
                        item: 1,
                        stage: SUBSET
                    }),
            "7 used: the 3-Might Sprite no longer fits, the subset closes"
        );
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: DESTINATION
            })
        );
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.location(FOUR),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2)),
            "the Sprite was dropped from the group and stays where it was"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn zero_picks_is_a_legal_play_that_moves_nobody_and_a_friendly_unit_is_refused() {
        let mut fixture = deep();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_TENTACLES)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, TENTACLES).unwrap();
        for wrong in [fixtures::VI, HEAVY, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit of 8 Might or less"
            );
        }
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "no unit, no location to ask");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} moves no unit".to_string()));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert_eq!(ctx.card(TENTACLES).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn at_play_a_pick_that_breaks_eight_total_might_is_not_offered() {
        let mut fixture = deep();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TENTACLES).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["done", "cancel"],
            "4 + 3 leaves no room for the 2-Might Jinx"
        );
    }
}
