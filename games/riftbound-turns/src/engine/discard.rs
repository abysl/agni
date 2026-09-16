use crate::engine::chain;
use crate::engine::ctx::Ctx;
use crate::state::{Ask, ChainItem, ItemStatus, PromptWhy, FLAG_DISCARDED};
use crate::Refusal;
use agni_plugin_sdk::prompt::Answer;

pub fn ask(ctx: &mut Ctx, item: &ChainItem, stage: u8) -> Option<Ask> {
    if ctx.hand_of(item.controller).is_empty() {
        return None;
    }
    Some(ctx.ask(
        item.controller,
        1,
        1,
        false,
        PromptWhy::Discard {
            item: item.id,
            stage,
        },
    ))
}

pub fn gesture(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
    let Some(PromptWhy::Discard { item, stage }) = ctx.blob.why else {
        return Err(Refusal::NoPrompt);
    };
    if ctx
        .blob
        .prompt
        .as_ref()
        .is_none_or(|prompt| prompt.seat != seat)
    {
        return Err(Refusal::NoPrompt);
    }
    ctx.blob.close_prompt();
    let from = ctx.entry.and_then(|entry| entry.from).or(ctx.zones.hand);
    if let Some(by) = ctx.banishes_instead_of_trash(card, from) {
        ctx.banish_instead_of_trash(card, by);
    }
    resume_with(ctx, seat, item, stage, card)
}

pub fn pick(ctx: &mut Ctx, seat: u8, item: u16, stage: u8, card: u32) -> Result<(), Refusal> {
    if !ctx.hand_of(seat).contains(&card) {
        return Err(Refusal::NoPrompt);
    }
    ctx.file_in_trash(card, seat);
    resume_with(ctx, seat, item, stage, card)
}

fn resume_with(ctx: &mut Ctx, seat: u8, item: u16, stage: u8, card: u32) -> Result<(), Refusal> {
    ctx.blob.drop_card_state(card);
    ctx.narrate(format!("{{seat {seat}}} discards {{card {card}}}"));
    if ctx.kind_of(card).is_none() {
        ctx.set_flag(card, FLAG_DISCARDED, true);
        if let Some(held) = ctx
            .blob
            .chain
            .iter_mut()
            .find(|held| held.id == item && held.status == ItemStatus::Resolving)
        {
            held.stage = stage;
        }
        return Ok(());
    }
    chain::resume(ctx, item, stage, &[card], Answer::Card(card))
}

pub fn awaited(ctx: &Ctx) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .find(|row| row.has(FLAG_DISCARDED))
        .map(|row| row.id)
}

pub fn revealed(ctx: &mut Ctx, card: u32) -> Result<bool, Refusal> {
    if !ctx.has_flag(card, FLAG_DISCARDED) {
        return Ok(false);
    }
    ctx.blob.drop_card_state(card);
    let Some(top) = ctx
        .blob
        .chain
        .iter()
        .rev()
        .find(|held| held.status == ItemStatus::Resolving)
        .map(|held| (held.id, held.stage))
    else {
        return Ok(false);
    };
    chain::resume(ctx, top.0, top.1, &[card], Answer::Card(card))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, ask_discard, discarded, discarded_kind};
    use crate::cards::{Card, Flow, Trigger, KIND_GEAR, KIND_SPELL, KIND_UNIT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent};
    use crate::engine::{act, chain, prompts, resume, settle};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::CardInfo;

    const PAINTER: u32 = 90;
    const STAGE_BRANCH: u8 = 1;

    fn brood(ctx: &mut Ctx, item: &ChainItem, stage: crate::cards::Stage) -> Flow {
        match stage.0 {
            0 => {
                prelude::draw(ctx, item.controller, 1);
                match ask_discard(ctx, item, STAGE_BRANCH) {
                    Some(ask) => Flow::Ask(ask),
                    None => Flow::Done,
                }
            }
            _ => {
                let me = item.kind.source();
                match discarded_kind(ctx) {
                    Some(KIND_SPELL) => {
                        prelude::draw(ctx, item.controller, 1);
                    }
                    Some(KIND_GEAR) => {
                        prelude::ready_runes(ctx, item.controller, 2);
                    }
                    Some(KIND_UNIT) => prelude::might_this_turn(ctx, item, me, 3, None),
                    _ => {}
                }
                Flow::Done
            }
        }
    }

    static PAINTER_CARD: Card = prelude::unit(
        "Painter",
        &[],
        &[prelude::triggered(
            Trigger::Move {
                of: crate::cards::Who::Me,
                to: crate::cards::Where::Any,
            },
            &[],
            brood,
        )],
    );

    fn easel() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(PAINTER, fixtures::BASE, 0, "Painter", 5));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(PAINTER, &PAINTER_CARD);
        fixture
    }

    fn resolving(ctx: &mut Ctx) -> u16 {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: PAINTER,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.status = ItemStatus::Finalized;
        ctx.blob.chain.push(item);
        chain::resolve_top(ctx);
        id
    }

    #[test]
    fn the_discard_prompt_lists_the_hand_and_a_pick_trashes_the_card_then_branches_on_its_kind() {
        let mut fixture = easel();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let item = resolving(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "draw 1 first");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item,
                stage: STAGE_BRANCH
            })
        );
        assert_eq!(ctx.blob.chain[0].stage, STAGE_BRANCH);
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Resolving);
        let labels: Vec<String> = prompts::offered(&ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect();
        assert_eq!(labels.len(), hand + 1, "every card in hand, no closers");
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
        let option = labels
            .iter()
            .position(|label| label == &format!("{{card {}}}", fixtures::HAND_GEAR))
            .unwrap() as u16;
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        let answered = prompts::answer(&mut ctx, 0, Pick { prompt, option })
            .unwrap()
            .unwrap();
        resume(&mut ctx, &answered).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "the branch ran and the trigger finished"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            4,
            "Gear · the exhausted rune is readied"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} discards {card 72}"));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_hand_to_trash_drag_answers_the_prompt_as_a_gesture() {
        let mut fixture = easel();
        let mut ctx = fixture.ctx();
        let item = resolving(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.scripts = fixture.scripts.clone().with_script(PAINTER, &PAINTER_CARD);
        let action = fixtures::move_action(fixtures::HAND_UNIT, fixtures::TRASH, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.unwrap();
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        assert_eq!(
            intent,
            Intent::Discard {
                card: fixtures::HAND_UNIT
            }
        );
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.iter().all(|held| held.id != item));
        assert_eq!(
            ctx.current_might(PAINTER),
            8,
            "Unit · the painter gains +3 this turn"
        );
        assert_eq!(discarded(&ctx), None, "the pick is consumed with the stage");
        let other = fixtures::move_action(fixtures::THEIR_HAND_CARD, fixtures::TRASH, 1);
        let mut fixture = easel();
        let mut ctx = fixture.ctx();
        resolving(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let ctx = fixture.ctx_for(1, &other);
        assert!(
            legal::classify(&ctx, 1, &ctx.entry.unwrap()).is_err(),
            "the other seat's drag is not the discard"
        );
    }

    #[test]
    fn the_reveal_entry_pays_an_awaited_face_through_decide_and_ignores_one_nobody_awaits() {
        use crate::cards::sabotage::tests::{
            cast, face_of, hand_of_three, THEIR_GEAR_CARD, THEIR_SPELL_CARD, THEIR_UNIT_CARD,
        };
        use agni_plugin_sdk::decide::{Action, Request};
        let mut fixture = hand_of_three();
        let item = cast(&mut fixture, crate::cards::sabotage::tests::SPELL);
        let request = Request {
            plugin_state: fixture.blob.encode(),
            players: fixture.table.players,
            seat: 1,
            action: Action::Reveal {
                card: THEIR_UNIT_CARD,
                face: face_of(THEIR_UNIT_CARD),
            },
            table: fixture.table.clone(),
        };
        let verdict = crate::decide(&request).expect("the owner's reveal is accepted");
        assert!(verdict.accept);
        let after = crate::state::GameBlob::decode(&verdict.plugin_state.unwrap()).unwrap();
        assert_eq!(after.chain[0].id, item);
        assert_eq!(after.chain[0].awaiting, [THEIR_SPELL_CARD, THEIR_GEAR_CARD]);
        let stray = Request {
            plugin_state: fixture.blob.encode(),
            players: fixture.table.players,
            seat: 0,
            action: Action::Reveal {
                card: fixtures::HAND_GEAR,
                face: face_of(fixtures::HAND_GEAR),
            },
            table: fixture.table.clone(),
        };
        let verdict = crate::decide(&stray).unwrap();
        assert!(verdict.accept);
        assert!(
            verdict.plugin_state.is_none(),
            "a face nobody awaits changes nothing"
        );
    }

    #[test]
    fn a_blank_faced_discard_parks_the_item_until_the_host_reveals_it() {
        let mut fixture = easel();
        let blank = CardInfo {
            id: 95,
            zone: Some(fixtures::HAND),
            seat: 0,
            owner: 0,
            ..CardInfo::default()
        };
        fixture.table.cards.push(blank);
        fixture.table.cards.retain(|card| {
            ![
                fixtures::HAND_UNIT,
                fixtures::HAND_SPELL,
                fixtures::HAND_GEAR,
            ]
            .contains(&card.id)
        });
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(PAINTER, &PAINTER_CARD);
        let mut ctx = fixture.ctx();
        let item = resolving(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        let option = prompts::offered(&ctx)
            .iter()
            .position(|opt| opt.label == "{card 95}")
            .unwrap() as u16;
        let answered = prompts::answer(&mut ctx, 0, Pick { prompt, option })
            .unwrap()
            .unwrap();
        resume(&mut ctx, &answered).unwrap();
        assert_eq!(awaited(&ctx), Some(95), "the face is not known yet");
        assert_eq!(ctx.blob.chain.len(), 1, "the item waits, resolving");
        assert_eq!(ctx.blob.chain[0].stage, STAGE_BRANCH);
        assert!(ctx.blob.prompt.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.scripts = fixture.scripts.clone().with_script(PAINTER, &PAINTER_CARD);
        let reveal = agni_plugin_sdk::decide::Action::Reveal {
            card: 95,
            face: agni_plugin_sdk::table::Face::named("Spark").with_kind(KIND_SPELL),
        };
        let mut ctx = fixture.ctx_for(0, &reveal);
        let hand = ctx.hand_of(0).len();
        assert!(revealed(&mut ctx, 95).unwrap());
        assert!(!revealed(&mut ctx, 95).unwrap());
        assert_eq!(awaited(&ctx), None);
        assert!(ctx.blob.chain.iter().all(|held| held.id != item));
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "Spell · draw 1");
        assert!(ctx.fault.is_none());
    }
}
