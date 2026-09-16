use super::party_favors::other_seats;
use super::prelude::{asking, done, play, spawn_gold, unit, with_candidates};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const QUESTION: &str = "yourself to play a Gold gear token exhausted · skip to pass";
pub const GOLD_ARRIVES_READY: bool = false;
pub const STAGE_HOST: u8 = 1;

pub fn chooser_at(ctx: &Ctx, item: &Item, stage: Stage) -> Option<u8> {
    let host = item.controller;
    match usize::from(stage.0).checked_sub(usize::from(STAGE_HOST))? {
        0 => Some(host),
        index => other_seats(ctx, host).get(index - 1).copied(),
    }
}

fn takers(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    chooser_at(ctx, item, stage)
        .map(TargetRef::Seat)
        .into_iter()
        .collect()
}

fn deal_in(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let host = item.controller;
    if let Some(chooser) = chooser_at(ctx, item, stage) {
        if ctx.picks().first() == Some(&u32::from(chooser)) {
            ctx.narrate(format!("{{seat {chooser}}} takes a Gold"));
            spawn_gold(ctx, chooser, GOLD_ARRIVES_READY);
            if chooser != host {
                ctx.narrate(format!(
                    "{{seat {host}}} plays a Gold for {{seat {chooser}}}'s"
                ));
                spawn_gold(ctx, host, GOLD_ARRIVES_READY);
            }
        } else {
            ctx.narrate(format!("{{seat {chooser}}} passes"));
        }
    }
    let next = Stage(stage.0.saturating_add(1).max(STAGE_HOST));
    match chooser_at(ctx, item, next) {
        Some(seat) => Flow::Ask(ctx.ask_seat_resume(item, seat, next.0, 0, 1)),
        None => done(),
    }
}

pub static CARD: Card = unit(
    "Card Sharp",
    &[],
    &[asking(
        with_candidates(play(&[], deal_in), takers),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::trove_golem::tests::golds_of;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts};
    use crate::state::{ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const SHARP: u32 = 90;

    fn sharp() -> CardInfo {
        let mut card = fixtures::unit(SHARP, fixtures::HAND, 0, "Card Sharp", 3);
        card.domain = vec!["Mind".into()];
        card.energy = Some(1);
        card
    }

    fn table() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sharp());
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SHARP).unwrap(), &CARD));
        fixture
    }

    fn deal(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, SHARP).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no targets: the mays come on resolution"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 2,
                stage: STAGE_HOST
            }),
            "the host is asked first · item 1 was the play"
        );
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Resolving);
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
        assert_eq!(fixtures::labels(ctx), ["{seat 0}", "skip"]);
        assert_eq!(
            prompts::status(
                ctx,
                PromptWhy::Resume {
                    item: 2,
                    stage: STAGE_HOST
                }
            ),
            format!("{{card {SHARP}}}: choose {QUESTION} (0 of 1)")
        );
    }

    #[test]
    fn the_script_is_a_plain_unit_asking_the_host_then_each_opponent_for_a_may() {
        assert!(std::ptr::eq(script_of("Card Sharp").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = table();
        let ctx = fixture.ctx();
        let item = Item::new(
            7,
            crate::state::ItemKind::Trigger {
                source: SHARP,
                index: 0,
            },
            0,
            crate::state::Origin::Board,
        );
        assert_eq!(chooser_at(&ctx, &item, Stage(0)), None);
        assert_eq!(chooser_at(&ctx, &item, Stage(STAGE_HOST)), Some(0));
        assert_eq!(chooser_at(&ctx, &item, Stage(STAGE_HOST + 1)), Some(1));
        assert_eq!(chooser_at(&ctx, &item, Stage(STAGE_HOST + 2)), None);
        assert_eq!(
            takers(&ctx, &item, Stage(STAGE_HOST + 1)),
            [TargetRef::Seat(1)]
        );
    }

    #[test]
    fn both_taking_a_gold_gives_the_host_two_and_the_opponent_one_all_exhausted() {
        let mut fixture = table();
        let mut ctx = fixture.ctx();
        deal(&mut ctx);
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: 1,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the guest does not answer for the host"
        );
        fixtures::choose(&mut ctx, 0, "{seat 0}").unwrap();
        assert_eq!(golds_of(&ctx, 0).len(), 1, "the host's own Gold");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 2,
                stage: STAGE_HOST + 1
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        assert_eq!(fixtures::labels(&ctx), ["{seat 1}", "skip"]);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{seat 1}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 1, "{seat 1}").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "two seats: one guest, one question"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(golds_of(&ctx, 1).len(), 1);
        assert_eq!(
            golds_of(&ctx, 0).len(),
            2,
            "one for the host, one for the guest who did"
        );
        for seat in [0, 1] {
            for gold in golds_of(&ctx, seat) {
                assert!(ctx.card(gold).unwrap().exhausted);
                assert_eq!(ctx.location(gold), Some(Location::Base(seat)));
                assert_eq!(ctx.controller(gold), seat);
            }
        }
        assert!(ctx.blob.log.contains(&"{seat 0} takes a Gold".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 1} takes a Gold".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} plays a Gold for {seat 1}'s".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_host_who_passes_and_an_opponent_who_passes_mint_nothing() {
        let mut fixture = table();
        let mut ctx = fixture.ctx();
        deal(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(golds_of(&ctx, 0).is_empty());
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        fixtures::choose(&mut ctx, 1, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(golds_of(&ctx, 0).is_empty());
        assert!(golds_of(&ctx, 1).is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} passes".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 1} passes".to_string()));
    }

    #[test]
    fn only_the_opponents_gold_earns_the_host_one_when_the_host_passed() {
        let mut fixture = table();
        let mut ctx = fixture.ctx();
        deal(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::choose(&mut ctx, 1, "{seat 1}").unwrap();
        assert_eq!(golds_of(&ctx, 1).len(), 1);
        assert_eq!(golds_of(&ctx, 0).len(), 1, "for the opponent who did");
        assert!(ctx.fault.is_none());
    }
}
