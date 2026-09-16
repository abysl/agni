use super::prelude::{
    a_spell, counter_spell, done, item_controller, item_target, play, spell, with_cost,
};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::{chain, cost};

pub const RANSOM: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};
const ANSWERED: u8 = 1;

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == ANSWERED {
        return settle(ctx, item);
    }
    let Some(target) = item_target(ctx, item, 0) else {
        return done();
    };
    let Some(payer) = item_controller(ctx, target) else {
        return done();
    };
    let ransom = cost::of_script(&RANSOM, &[]);
    Flow::Ask(ctx.ask_pay_or_let(item, payer, &ransom, ANSWERED))
}

fn settle(ctx: &mut Ctx, item: &Item) -> Flow {
    if chain::paid(ctx) {
        if let Some(target) = item_target(ctx, item, 0) {
            ctx.narrate(format!(
                "{{card {}}} stays on the chain",
                spell_of(ctx, target)
            ));
        }
        return done();
    }
    counter_spell(ctx, item, 0);
    done()
}

fn spell_of(ctx: &Ctx, item: u16) -> u32 {
    ctx.chain_item(item)
        .map(|held| held.kind.source())
        .unwrap_or(0)
}

pub static CARD: Card = spell(
    "Hard Bargain",
    &[Keyword::Reaction, Keyword::Repeat(REPEAT)],
    &[with_cost(
        play(&[a_spell("a spell to counter")], run),
        RANSOM,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, prompts, resume, settle};
    use crate::state::{ItemStatus, PromptWhy, TargetRef, SLOT_REPEAT};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick, PickRefusal};

    const BARGAIN: u32 = 90;
    const MY_BARGAIN: u32 = 91;
    const THEIR_EXTRA: [u32; 3] = [46, 47, 48];
    const MY_EXTRA: [u32; 2] = [36, 37];

    fn bargain(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Hard Bargain", 2, 0);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bargain(BARGAIN, 1));
        fixture.table.cards.push(bargain(MY_BARGAIN, 0));
        for rune in THEIR_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Chaos", false));
        }
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
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

    fn countered_spark(ctx: &mut Ctx, repeat: bool) {
        fixtures::play_from_hand(ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(ctx, 0).unwrap();
        fixtures::play_from_hand(ctx, 1, BARGAIN).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_REPEAT as u8
            })
        );
        assert_eq!(
            prompts::status(ctx, ctx.blob.why.unwrap()),
            format!("repeat {{card {BARGAIN}}} for 2 energy?")
        );
        fixtures::choose(ctx, 1, if repeat { "yes" } else { "no" }).unwrap();
        let groups = if repeat { 2 } else { 1 };
        for spec in 0..groups {
            assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec }));
            fixtures::choose(
                ctx,
                1,
                &format!("{{card {}}} on the chain", fixtures::HAND_SPELL),
            )
            .unwrap();
        }
        assert_eq!(
            ctx.blob.chain[1].targets,
            vec![TargetRef::Item(1); usize::from(groups)]
        );
        priority::pass(ctx, 1).unwrap();
        priority::pass(ctx, 0).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_repeatable_reaction_whose_play_ability_carries_the_ransom() {
        assert_eq!(CARD.name, "Hard Bargain");
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.cost, Some(RANSOM));
        assert_eq!(ability.targets.len(), 1);
    }

    #[test]
    fn the_countered_spells_controller_is_asked_and_paying_two_keeps_the_spell() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        countered_spark(&mut ctx, false);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::PayOrLet {
                item: 2,
                stage: ANSWERED
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0, "the payer is Spark's controller");
        assert_eq!(ctx.blob.chain[1].status, ItemStatus::Resolving);
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("pay 2 energy to keep {{card {}}}?", fixtures::HAND_SPELL)
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("{seat 0} may pay 2 energy to keep")));
        let ready = ctx.ready_runes_of(0).len();
        let answered = prompts::answer(
            &mut ctx,
            0,
            Pick {
                prompt: prompt.id,
                option: 0,
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(answered.answer, Answer::Yes);
        resume(&mut ctx, &answered).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 2,
            "two runes exhausted"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} pays 2 energy"));
        assert_eq!(ctx.blob.chain.len(), 1, "Spark stays");
        assert_eq!(ctx.blob.chain[0].id, 1);
        assert_eq!(
            ctx.card(BARGAIN).unwrap().zone,
            Some(fixtures::TRASH),
            "Hard Bargain resolved and is trashed"
        );
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::PlayedSpell { item: 2, .. })));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH),
            "Spark resolved on its own afterwards"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_counters_the_spell_and_an_unaffordable_ransom_is_declined_without_a_prompt() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        countered_spark(&mut ctx, false);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::HAND_SPELL,
            zone: fixtures::TRASH,
            seat: 0,
            index: TOP
        }));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {}}} is countered", fixtures::HAND_SPELL)));
        let mut poor = armed();
        poor.table.cards.retain(|card| !MY_EXTRA.contains(&card.id));
        poor.resolve();
        let mut ctx = poor.ctx();
        countered_spark(&mut ctx, false);
        assert!(
            ctx.blob.prompt.is_none(),
            "one ready rune cannot pay two: the confirm is answered no by itself"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn hard_bargain_needs_a_spell_on_the_chain_and_cannot_be_cast_on_the_opponents_turn_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, BARGAIN)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, MY_BARGAIN).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "nothing on the chain to counter"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(MY_BARGAIN).unwrap().zone, Some(fixtures::HAND));
    }

    #[test]
    fn repeated_against_one_spell_asks_its_controller_twice_and_the_second_ransom_is_short() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        countered_spark(&mut ctx, true);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::PayOrLet {
                item: 2,
                stage: ANSWERED
            })
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "the first ransom is paid");
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {BARGAIN}}} repeats")));
        assert!(
            ctx.blob.prompt.is_none(),
            "one rune cannot pay the second ransom: answered no without a click"
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "the second execution counters Spark"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {}}} is countered", fixtures::HAND_SPELL)));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| line.starts_with("{seat 0} may pay 2 energy to keep"))
                .count(),
            2,
            "each execution asks the controller separately"
        );
        assert!(ctx.fault.is_none());
    }
}
