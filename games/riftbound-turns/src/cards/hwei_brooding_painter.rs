use super::prelude::{
    ask_discard, asking, discarded_kind, done, draw, might_this_turn, on_move, ready, unit,
    with_candidates,
};
use super::{Card, Flow, Item, Stage, KIND_GEAR, KIND_SPELL, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const MIGHT: i16 = 3;
pub const RUNES: u8 = 2;
const DRAWS: usize = 1;
const STAGE_BRANCH: u8 = 1;
const STAGE_RUNES: u8 = 2;

fn exhausted_runes(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != STAGE_RUNES {
        return Vec::new();
    }
    ctx.runes_of(item.controller)
        .into_iter()
        .filter(|rune| rune.exhausted)
        .map(|rune| TargetRef::Card(rune.id))
        .collect()
}

fn picked_runes(ctx: &Ctx, item: &Item) -> Vec<u32> {
    let offered = exhausted_runes(ctx, item, Stage(STAGE_RUNES));
    ctx.picks()
        .iter()
        .copied()
        .filter(|rune| offered.contains(&TargetRef::Card(*rune)))
        .take(usize::from(RUNES))
        .collect()
}

fn brood(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    match stage.0 {
        STAGE_BRANCH => match discarded_kind(ctx) {
            Some(KIND_SPELL) => {
                ctx.narrate(format!("{{card {me}}} · a Spell · draw 1"));
                draw(ctx, seat, DRAWS);
                done()
            }
            Some(KIND_GEAR) => {
                if exhausted_runes(ctx, item, Stage(STAGE_RUNES)).is_empty() {
                    ctx.narrate(format!("{{card {me}}} · Gear · no rune to ready"));
                    return done();
                }
                Flow::Ask(ctx.ask_resume(item, STAGE_RUNES, 0, RUNES))
            }
            Some(KIND_UNIT) => {
                if ctx.on_board(me) {
                    ctx.narrate(format!("{{card {me}}} · a Unit · +{MIGHT} this turn"));
                    might_this_turn(ctx, item, me, MIGHT, None);
                }
                done()
            }
            _ => done(),
        },
        STAGE_RUNES => {
            for rune in picked_runes(ctx, item) {
                ready(ctx, rune);
            }
            done()
        }
        _ => {
            draw(ctx, seat, DRAWS);
            match ask_discard(ctx, item, STAGE_BRANCH) {
                Some(ask) => Flow::Ask(ask),
                None => done(),
            }
        }
    }
}

pub static CARD: Card = unit(
    "Hwei - Brooding Painter",
    &[],
    &[asking(
        with_candidates(on_move(&[], brood), exhausted_runes),
        "up to two runes to ready",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{move_unit, swap_units, Location, Swapped};
    use crate::cards::{Trigger, Where, Who};
    use crate::engine::ctx::{Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent};
    use crate::engine::{act, march, phases, priority, prompts, resume, settle};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::Target;

    const HWEI: u32 = 90;
    const RUNE_B: u32 = 41;
    const RUNE_C: u32 = 42;
    const RUNE_D: u32 = 43;

    fn studio() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut hwei = fixtures::unit(HWEI, fixtures::BASE, 0, "Hwei - Brooding Painter", 5);
        hwei.domain = vec!["Mind".into()];
        hwei.energy = Some(5);
        hwei.power = Some(1);
        fixture.table.cards.push(hwei);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        fixture
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        let prompt = ctx.blob.prompt.as_ref().map(|p| p.id).unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)
    }

    fn card_label(card: u32) -> String {
        format!("{{card {card}}}")
    }

    fn resolve(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn trigger_id(ctx: &Ctx) -> u16 {
        let top = ctx
            .blob
            .chain
            .last()
            .expect("the move trigger is on the chain");
        assert!(matches!(
            top.kind,
            ItemKind::Trigger { source, index: 0 } if source == HWEI
        ));
        top.id
    }

    fn brooding(ctx: &Ctx, item: u16) {
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item,
                stage: STAGE_BRANCH
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|p| p.seat), Some(0));
        assert_eq!(
            ctx.blob.chain.last().map(|top| top.status),
            Some(ItemStatus::Resolving)
        );
        assert_eq!(
            ctx.blob.chain.last().map(|top| top.stage),
            Some(STAGE_BRANCH)
        );
    }

    #[test]
    fn the_registry_resolves_hwei_with_one_trigger_on_any_move() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Hwei - Brooding Painter").unwrap(),
            &CARD
        ));
        let fixture = studio();
        assert!(std::ptr::eq(fixture.scripts.of_card(HWEI).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert!(
            ability.candidates.is_some(),
            "the rune pick is a Resume prompt"
        );
        assert_eq!(ability.question, Some("up to two runes to ready"));
    }

    #[test]
    fn a_march_draws_then_asks_for_a_discard_and_a_spell_draws_one_more() {
        let mut fixture = studio();
        let action = fixtures::move_action(HWEI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        march::standard_move(
            &mut ctx,
            0,
            HWEI,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the move trigger waits on the chain"
        );
        let item = trigger_id(&ctx);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "nothing happens before it resolves"
        );
        resolve(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "draw 1 first");
        brooding(&ctx, item);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
        let offered = labels(&ctx);
        assert_eq!(offered.len(), hand + 1, "every card in hand, no closers");
        assert!(offered.contains(&card_label(fixtures::HAND_SPELL)));
        choose(&mut ctx, 0, &card_label(fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH),
            "the discard is in the trash"
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.chain.iter().all(|held| held.id != item),
            "the trigger finished"
        );
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + 1,
            "Spell · draw 1 more: +1, -1 discarded, +1"
        );
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
                .count(),
            2
        );
        assert_eq!(might_counter(&ctx, HWEI), 0);
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune readied");
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{seat 0}} discards {{card {}}}", fixtures::HAND_SPELL)));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {HWEI}}} · a Spell · draw 1")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_effect_move_counts_and_a_gear_readies_up_to_two_chosen_runes() {
        let mut fixture = studio();
        for rune in [RUNE_B, RUNE_C] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        let hand = ctx.hand_of(0).len();
        move_unit(
            &mut ctx,
            &fixtures::effect_of(0),
            HWEI,
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, cause: crate::engine::ctx::MoveCause::Effect, .. } if *card == HWEI
        )));
        let item = trigger_id(&ctx);
        resolve(&mut ctx);
        brooding(&ctx, item);
        choose(&mut ctx, 0, &card_label(fixtures::HAND_GEAR)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: STAGE_RUNES
            }),
            "Gear · pick the runes to ready"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {HWEI}}}: choose up to two runes to ready (0 of 2)")
        );
        assert_eq!(
            labels(&ctx),
            [
                card_label(fixtures::RUNE_A),
                card_label(RUNE_B),
                card_label(RUNE_C),
                "done".to_string(),
                "skip".to_string()
            ],
            "the exhausted runes, and the ready one is not offered"
        );
        choose(&mut ctx, 0, &card_label(RUNE_B)).unwrap();
        assert_eq!(
            labels(&ctx),
            [
                card_label(fixtures::RUNE_A),
                card_label(RUNE_C),
                "done".to_string()
            ]
        );
        choose(&mut ctx, 0, &card_label(fixtures::RUNE_A)).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "two is the most: done is the only option left and settle takes it"
        );
        assert!(ctx.blob.chain.iter().all(|held| held.id != item));
        assert!(!ctx.card(fixtures::RUNE_A).unwrap().exhausted);
        assert!(!ctx.card(RUNE_B).unwrap().exhausted);
        assert!(
            ctx.card(RUNE_C).unwrap().exhausted,
            "the third exhausted rune stays as it was"
        );
        assert!(!ctx.card(RUNE_D).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Readied { by: 0, .. }))
                .count(),
            2
        );
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "+1 drawn, -1 discarded, no second draw"
        );
        assert_eq!(might_counter(&ctx, HWEI), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_exhausted_rune_the_gear_branch_asks_nothing_and_a_skip_readies_none() {
        let mut fixture = studio();
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        move_unit(
            &mut ctx,
            &fixtures::effect_of(0),
            HWEI,
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        let item = trigger_id(&ctx);
        resolve(&mut ctx);
        choose(&mut ctx, 0, &card_label(fixtures::HAND_GEAR)).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no rune to ready, nothing to ask"
        );
        assert!(ctx.blob.chain.iter().all(|held| held.id != item));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {HWEI}}} · Gear · no rune to ready")));

        let mut fixture = studio();
        let mut ctx = fixture.ctx();
        move_unit(
            &mut ctx,
            &fixtures::effect_of(0),
            HWEI,
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        let item = trigger_id(&ctx);
        resolve(&mut ctx);
        choose(&mut ctx, 0, &card_label(fixtures::HAND_GEAR)).unwrap();
        assert_eq!(
            labels(&ctx),
            [
                card_label(fixtures::RUNE_A),
                "done".to_string(),
                "skip".to_string()
            ],
            "up to two: one exhausted rune is still a choice"
        );
        choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.iter().all(|held| held.id != item));
        assert!(
            ctx.card(fixtures::RUNE_A).unwrap().exhausted,
            "up to two includes none"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_swap_counts_as_a_move_and_a_unit_discarded_by_gesture_gives_him_three_might() {
        let mut fixture = studio();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(swap_units(&mut ctx, HWEI, fixtures::VI), Swapped::Swapped);
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(HWEI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(ctx.blob.chain.len(), 1, "only Hwei has a move trigger");
        let item = trigger_id(&ctx);
        resolve(&mut ctx);
        brooding(&ctx, item);
        let table = ctx.table.clone();
        let blob = ctx.blob.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = blob;
        let action = fixtures::move_action(fixtures::HAND_UNIT, fixtures::TRASH, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.unwrap();
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Ok(Intent::Discard {
                card: fixtures::HAND_UNIT
            }),
            "a hand to trash drag answers the discard"
        );
        act(
            &mut ctx,
            0,
            Intent::Discard {
                card: fixtures::HAND_UNIT,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.iter().all(|held| held.id != item));
        assert_eq!(might_counter(&ctx, HWEI), i32::from(MIGHT));
        assert_eq!(ctx.current_might(HWEI), 8, "Unit · +3 this turn");
        assert_eq!(might_counter(&ctx, fixtures::VI), 0, "only Hwei grows");
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {HWEI}}} · a Unit · +3 this turn")));
        if ctx.blob.showdown.is_some() {
            resolve(&mut ctx);
        }
        assert!(ctx.blob.showdown.is_none());
        let blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(might_counter(&ctx, HWEI), 0);
        assert_eq!(ctx.current_might(HWEI), 5, "the bonus is only for the turn");
    }

    #[test]
    fn the_discard_is_the_controllers_alone_and_the_turn_waits_on_it() {
        let mut fixture = studio();
        fixture.table.card_mut(HWEI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        move_unit(&mut ctx, &fixtures::effect_of(0), HWEI, Location::Base(0));
        settle(&mut ctx).unwrap();
        let item = trigger_id(&ctx);
        resolve(&mut ctx);
        brooding(&ctx, item);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the opponent cannot discard for him"
        );
        let count = labels(&ctx).len() as u16;
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt,
                    option: count
                }
            ),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: count,
                count: usize::from(count)
            })),
            "only the hand is offered"
        );
        assert_eq!(
            phases::end_turn(&mut ctx),
            Err(Refusal::PromptOpen),
            "the turn cannot end around the discard"
        );
        assert!(
            !labels(&ctx).contains(&card_label(HWEI)),
            "a unit on the board is not in hand"
        );
        brooding(&ctx, item);
        let table = ctx.table.clone();
        let blob = ctx.blob.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = blob;
        let theirs = fixtures::move_action(fixtures::THEIR_HAND_CARD, fixtures::TRASH, 1);
        let ctx = fixture.ctx_for(1, &theirs);
        assert!(
            legal::classify(&ctx, 1, &ctx.entry.unwrap()).is_err(),
            "the other seat's drag is not the discard"
        );
    }

    #[test]
    fn with_an_empty_hand_and_deck_there_is_nothing_to_discard_and_no_branch() {
        let mut fixture = studio();
        fixture.table.cards.retain(|card| {
            !(card.owner == 0
                && matches!(card.zone, Some(fixtures::HAND) | Some(fixtures::MAIN_DECK)))
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.hand_of(0).is_empty());
        move_unit(
            &mut ctx,
            &fixtures::effect_of(0),
            HWEI,
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        while let Some(held) = ctx.blob.priority {
            if ctx.blob.chain.is_empty() {
                break;
            }
            priority::pass(&mut ctx, held.active).unwrap();
        }
        assert!(
            ctx.blob.prompt.is_none(),
            "409.4 · nothing to discard is ignored"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!("{{card {HWEI}}} triggers")));
        assert!(ctx.hand_of(0).is_empty());
        assert_eq!(might_counter(&ctx, HWEI), 0);
        assert!(ctx.card(fixtures::RUNE_A).unwrap().exhausted);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_recall_is_not_a_move() {
        let mut fixture = studio();
        fixture.table.card_mut(HWEI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.recall(HWEI, false);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .events
            .iter()
            .all(|event| !matches!(event, Event::Moved { .. })));
    }
}
