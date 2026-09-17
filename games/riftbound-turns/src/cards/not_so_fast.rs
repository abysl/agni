use super::prelude::{
    an_item, counter_spell, play, spell, ENEMY_ITEM_CHOOSING_FRIENDLY_UNIT_OR_GEAR,
};
use super::{Card, Flow, Keyword};

pub static CARD: Card = spell(
    "Not So Fast",
    &[Keyword::Reaction],
    &[play(
        &[an_item(
            ENEMY_ITEM_CHOOSING_FRIENDLY_UNIT_OR_GEAR,
            "an enemy spell or ability that chooses a friendly unit or gear",
        )],
        |ctx, item, _| {
            counter_spell(ctx, item, 0);
            Flow::Done
        },
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, a_card, a_unit, triggered};
    use crate::cards::{Filter, Trigger};
    use crate::engine::ctx::{Ctx, Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play, priority, prompts, settle};
    use crate::state::{GameBlob, ItemKind, Mode, Origin, Phase, Priority, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::{CardInfo, Target};

    static ZAP: Card = prelude::spell(
        "Zap",
        &[],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = prelude::card_target(ctx, item, 0) {
                prelude::deal(ctx, item, unit, 3);
            }
            Flow::Done
        })],
    );

    static PING: Card = prelude::spell(
        "Ping",
        &[],
        &[play(
            &[a_card(Filter::Legend, "a legend")],
            |ctx, item, _| {
                prelude::draw(ctx, item.controller, 1);
                Flow::Done
            },
        )],
    );

    static NUDGE: Card = prelude::spell("Nudge", &[], &[play(&[], |_, _, _| Flow::Done)]);

    static ZAPPER: Card = prelude::unit(
        "Zapper",
        &[],
        &[triggered(
            Trigger::YouPlaySpell,
            &[a_unit("a unit")],
            |ctx, item, _| {
                if let Some(unit) = prelude::card_target(ctx, item, 0) {
                    prelude::deal(ctx, item, unit, 3);
                }
                Flow::Done
            },
        )],
    );

    const NOT_SO_FAST: u32 = 90;
    const THEIR_SPELL: u32 = 91;

    fn their_turn(their_spell: &str, script: &'static Card) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 1, Mode::Enforced);
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.set_phase(Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.table.cards.push(CardInfo {
            domain: vec!["Calm".into()],
            ..fixtures::spell(NOT_SO_FAST, fixtures::HAND, 0, "Not So Fast", 2, 1)
        });
        fixture.table.cards.push(CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::spell(THEIR_SPELL, fixtures::HAND, 1, their_spell, 1, 0)
        });
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(THEIR_SPELL, script);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(NOT_SO_FAST).unwrap(),
            &CARD
        ));
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
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
        crate::engine::play::begin(ctx, seat, card, Origin::Hand, None)?;
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
                panic!("only target prompts are answered here: {:?}", answered.why);
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

    fn damage(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn played_spells(ctx: &Ctx) -> Vec<u16> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::PlayedSpell { item, .. } => Some(*item),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn counters_an_enemy_spell_that_chose_a_friendly_unit() {
        let mut fixture = their_turn("Zap", &ZAP);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 1, THEIR_SPELL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        pick(&mut ctx, 1, 0).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(priority::holder(&ctx), Some(1));
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, NOT_SO_FAST)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 0 waits for priority"
        );
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        let before = ctx.effects.len();
        play_from_hand(&mut ctx, 0, NOT_SO_FAST).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(labels(&ctx), ["{card 91} on the chain", "cancel"]);
        let offered = prompts::offered(&ctx);
        assert_eq!(offered[0].answer, Answer::Item(1));
        assert_eq!(offered[0].card, Some(THEIR_SPELL));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 2, spec: 0 }),
            "{card 90}: choose an enemy spell or ability that chooses a friendly unit or gear (0 of 1)"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Item(1)]);
        assert_eq!(
            ctx.effects[before..],
            [
                Effect::exhaust(42),
                Effect::exhaust(41),
                Effect::Move {
                    card: 42,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: agni_plugin_sdk::decide::BOTTOM
                },
            ],
            "two runes exhausted for energy, the Calm rune recycled for Calm power"
        );
        assert_eq!(
            ctx.blob.priority,
            Some(Priority {
                active: 0,
                passes: 0
            })
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.priority.is_none());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(
            ctx.card(THEIR_SPELL).unwrap().zone,
            Some(fixtures::TRASH),
            "Zap goes to its owner's trash"
        );
        assert_eq!(ctx.card(THEIR_SPELL).unwrap().seat, 1);
        assert_eq!(ctx.card(NOT_SO_FAST).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_SPELL,
            zone: fixtures::TRASH,
            seat: 1,
            index: TOP
        }));
        assert_eq!(damage(&ctx, fixtures::VI), 0, "Zap never resolved");
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(crate::engine::ctx::Location::Base(0))
        );
        assert!(ctx.blob.log.contains(&"{card 91} is countered".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line == "{card 91} resolves"));
        assert!(ctx.blob.log.contains(&"{card 90} resolves".to_string()));
        assert_eq!(played_spells(&ctx), [2], "only Not So Fast was played");
        assert!(ctx.blob.seat(0).played_main);
        assert!(!ctx.blob.seat(1).played_main);
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Annotate { card, .. } if *card == 44 || *card == 45
            ) && effect != &Effect::exhaust(44)
                && effect != &Effect::exhaust(45)),
            "nothing is refunded to seat 1"
        );
    }

    #[test]
    fn counters_an_enemy_ability_that_chose_a_friendly_unit_and_leaves_its_source() {
        let mut fixture = their_turn("Nudge", &NUDGE);
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::THEIR_UNIT, &ZAPPER);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 1, THEIR_SPELL).unwrap();
        assert!(ctx.blob.prompt.is_none());
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 2, spec: 0 }),
            "Zapper triggers on Nudge resolving and asks seat 1 for a unit"
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        pick(&mut ctx, 1, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == fixtures::THEIR_UNIT
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        play_from_hand(&mut ctx, 0, NOT_SO_FAST).unwrap();
        assert_eq!(labels(&ctx), ["{card 81} on the chain", "cancel"]);
        assert_eq!(prompts::offered(&ctx)[0].answer, Answer::Item(2));
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} ability is countered".to_string()));
        assert_eq!(damage(&ctx, fixtures::VI), 0, "the ability never resolved");
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::BASE),
            "countering an ability leaves its source on the board"
        );
        assert_eq!(ctx.card(NOT_SO_FAST).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(played_spells(&ctx), [1, 3]);
    }

    #[test]
    fn an_enemy_spell_choosing_only_its_own_unit_is_no_target_and_neither_is_an_empty_chain() {
        let mut fixture = their_turn("Zap", &ZAP);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, NOT_SO_FAST)),
            Err(Refusal::NotYourTurn),
            "a Reaction has nothing to react to in the other seat's Neutral Open"
        );
        play_from_hand(&mut ctx, 1, THEIR_SPELL).unwrap();
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"]
        );
        pick(&mut ctx, 1, 2).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        priority::pass(&mut ctx, 1).unwrap();
        let before = ctx.effects.len();
        play_from_hand(&mut ctx, 0, NOT_SO_FAST).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["cancel"],
            "Zap chose an enemy unit, so Not So Fast has no legal target"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            ctx.effects[before..],
            [Effect::Move {
                card: NOT_SO_FAST,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }],
            "the reaction goes back unpaid"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(priority::holder(&ctx), Some(0));
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::THEIR_UNIT,
            n: 3,
            source: crate::engine::ctx::Cause::Item(1)
        }));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH),
            "Zap resolved and its three damage killed Jinx at cleanup"
        );
        assert_eq!(ctx.card(NOT_SO_FAST).unwrap().zone, Some(fixtures::HAND));
    }

    #[test]
    fn an_enemy_spell_that_chose_only_a_friendly_legend_is_never_offered() {
        let mut fixture = their_turn("Ping", &PING);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 1, THEIR_SPELL).unwrap();
        assert_eq!(labels(&ctx), ["{card 75}", "cancel"]);
        pick(&mut ctx, 1, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        play_from_hand(&mut ctx, 0, NOT_SO_FAST).unwrap();
        assert_eq!(
            labels(&ctx),
            ["cancel"],
            "a legend is neither a unit nor gear, so Ping is no target"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 0, &[1]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.card(NOT_SO_FAST).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.blob.chain.len(), 1);
        let hand = ctx.hand_of(1).len();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(1).len(), hand + 1, "Ping resolved and drew");
        assert_eq!(played_spells(&ctx), [1]);
    }
}
