use super::prelude::{a_spell, counter_spell, done, lock_spells, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(controller) = counter_spell(ctx, item, 0) {
        lock_spells(ctx, controller);
    }
    done()
}

pub static CARD: Card = spell(
    "Lilting Lullaby",
    &[Keyword::Reaction],
    &[play(&[a_spell("a spell to counter")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::CARD;
    use crate::cards::{Keyword, TargetKind, Trigger};
    use crate::engine::ctx::{Ctx, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, play, priority, prompts, settle};
    use crate::state::{Origin, PlayLock, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::CardInfo;

    const LULLABY: u32 = 90;
    const SECOND_LULLABY: u32 = 91;
    const MY_LULLABY: u32 = 92;
    const SECOND_SPARK: u32 = 93;
    const THEIR_CALM_A: u32 = 100;
    const THEIR_CALM_B: u32 = 101;
    const MY_CALM: u32 = 102;
    const MY_MIND: u32 = 103;
    const MY_FURY: u32 = 104;

    fn lullaby(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Calm".into(), "Mind".into()],
            ..fixtures::spell(id, fixtures::HAND, seat, "Lilting Lullaby", 2, 2)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(lullaby(LULLABY, 1));
        fixture.table.cards.push(lullaby(SECOND_LULLABY, 1));
        fixture.table.cards.push(lullaby(MY_LULLABY, 0));
        fixture.table.cards.push(fixtures::spell(
            SECOND_SPARK,
            fixtures::HAND,
            0,
            "Spark",
            1,
            0,
        ));
        for (id, seat, domain) in [
            (THEIR_CALM_A, 1, "Calm"),
            (THEIR_CALM_B, 1, "Calm"),
            (MY_CALM, 0, "Calm"),
            (MY_MIND, 0, "Mind"),
            (MY_FURY, 0, "Fury"),
        ] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, seat, domain, false));
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

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let answered = prompts::answer(ctx, seat, Pick { prompt, option })?;
        if let Some(answered) = answered {
            let PromptWhy::Target { item, spec } = answered.why else {
                panic!(
                    "a Lullaby only ever asks for its target: {:?}",
                    answered.why
                );
            };
            match answered.answer {
                Answer::Cancel => play::cancel(ctx, item),
                _ => play::choose_targets(ctx, item, spec, &answered.prompt.picked)?,
            }
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    #[test]
    fn the_script_is_a_reaction_that_chooses_one_spell_on_the_chain() {
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].kind, TargetKind::Item);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        let fixture = armed();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LULLABY).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn it_counters_the_chosen_spell_and_its_controller_cannot_play_spells_until_the_turn_ends() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, LULLABY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(labels(&ctx), ["{card 71} on the chain", "cancel"]);
        assert_eq!(prompts::offered(&ctx)[0].answer, Answer::Item(1));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 2, spec: 0 }),
            "{card 90}: choose a spell to counter (0 of 1)"
        );
        pick(&mut ctx, 1, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Item(1)]);
        assert_eq!(
            ctx.effects.iter().filter(|effect| matches!(effect, Effect::Move { zone, .. } if *zone == fixtures::RUNE_DECK)).count(),
            3,
            "Spark recycled one rune, the Lullaby recycled a Calm and a Mind rune"
        );
        assert!(
            ctx.blob.seat(0).play_lock.is_empty(),
            "nothing is locked before the counter resolves"
        );
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.priority.is_none());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.card(LULLABY).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.seat(0).play_lock, PlayLock::SPELLS);
        assert!(ctx.blob.seat(1).play_lock.is_empty());
        assert!(ctx.blob.log.contains(&"{card 71} is countered".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} can't play spells this turn".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line == "{card 71} resolves"));
        let spells: Vec<&Event> = ctx
            .events
            .iter()
            .filter(|event| matches!(event, Event::PlayedSpell { .. }))
            .collect();
        assert_eq!(
            spells,
            [&Event::PlayedSpell {
                item: 2,
                controller: 1,
                nth: 1
            }],
            "the countered Spark was never played"
        );
        assert!(!ctx.blob.seat(0).played_main);
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, SECOND_SPARK)),
            Err(Refusal::Illegal(Reason::NoSpells))
        );
        assert!(
            legal::classify(
                &ctx,
                0,
                &EntryMove {
                    card: fixtures::HAND_UNIT,
                    from: ctx.zones.hand,
                    from_seat: 0,
                    to: Some(fixtures::BASE),
                    to_seat: 0,
                    index: TOP,
                    hidden: false,
                }
            )
            .is_ok(),
            "only spells are locked"
        );
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        assert!(
            ctx.blob.seat(0).play_lock.is_empty(),
            "the lock lasts the turn"
        );
    }

    #[test]
    fn a_second_lullaby_on_the_same_spell_finds_nothing_left_to_counter_and_locks_nobody_twice() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, LULLABY).unwrap();
        pick(&mut ctx, 1, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, SECOND_LULLABY).unwrap();
        assert_eq!(
            labels(&ctx),
            ["{card 71} on the chain", "{card 90} on the chain", "cancel"],
            "a Lullaby is itself a spell on the chain"
        );
        pick(&mut ctx, 1, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 3);
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the second Lullaby countered Spark"
        );
        assert_eq!(ctx.blob.chain[0].kind.source(), LULLABY);
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.blob.seat(0).play_lock, PlayLock::SPELLS);
        assert!(ctx.blob.log.contains(&"{card 71} is countered".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} can't play spells this turn".to_string()));
        ctx.blob.log.clear();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(LULLABY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{card 90} resolves".to_string()));
        assert!(
            !ctx.blob
                .log
                .iter()
                .any(|line| line.contains("countered") || line.contains("can't play spells")),
            "the first Lullaby's target was gone, so it did nothing: {:?}",
            ctx.blob.log
        );
    }

    #[test]
    fn countering_a_lullaby_with_a_lullaby_locks_the_reacting_seat_and_spares_the_first_spell() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, LULLABY).unwrap();
        pick(&mut ctx, 1, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        play_from_hand(&mut ctx, 0, MY_LULLABY).unwrap();
        assert_eq!(
            labels(&ctx),
            ["{card 71} on the chain", "{card 90} on the chain", "cancel"]
        );
        pick(&mut ctx, 0, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 3);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::HAND_SPELL);
        assert_eq!(ctx.card(LULLABY).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.seat(1).play_lock, PlayLock::SPELLS);
        assert!(ctx.blob.seat(0).play_lock.is_empty());
        assert_eq!(priority::holder(&ctx), Some(0));
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, SECOND_LULLABY)),
            Err(Refusal::Illegal(Reason::NoSpells)),
            "the locked seat cannot react with its other Lullaby"
        );
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&"{card 71} resolves".to_string()));
        assert!(ctx.blob.seat(0).played_main);
    }

    #[test]
    fn it_is_refused_out_of_priority_and_offers_only_cancel_with_no_spell_on_the_chain() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            play_from_hand(&mut ctx, 1, LULLABY),
            Err(Refusal::NotYourTurn),
            "the other seat has no window in a Neutral Open"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            play_from_hand(&mut ctx, 1, LULLABY),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "priority is with the spell's controller first"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        let paid = ctx.effects.len();
        play_from_hand(&mut ctx, 0, MY_LULLABY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        let offered = labels(&ctx);
        assert!(
            !offered.iter().any(|label| label.contains("on the chain")),
            "no spell to counter: {offered:?}"
        );
        assert_eq!(offered.last().map(String::as_str), Some("cancel"));
        assert_eq!(
            ctx.effects.len(),
            paid,
            "nothing is paid for a spell with no target"
        );
        pick(&mut ctx, 0, offered.len() as u16 - 1).unwrap();
        assert_eq!(ctx.effects.len(), paid + 1, "only the move back to hand");
        assert_eq!(ctx.card(MY_LULLABY).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.seat(0).play_lock.is_empty());
        assert!(ctx.blob.seat(1).play_lock.is_empty());
    }
}
