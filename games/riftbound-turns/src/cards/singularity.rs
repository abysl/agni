use super::prelude::{card_targets, deal, done, play, spell, units_up_to};
use super::Card;

pub const DAMAGE: u8 = 6;

pub static CARD: Card = spell(
    "Singularity",
    &[],
    &[play(
        &[units_up_to(2, "up to two units")],
        |ctx, item, _| {
            for unit in card_targets(ctx, item) {
                deal(ctx, item, unit, DAMAGE);
            }
            done()
        },
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Cause, Ctx, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play, priority, prompts, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};

    const SINGULARITY: u32 = 90;
    const MIND_RUNES: [u32; 6] = [100, 101, 102, 103, 104, 105];

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1);
        for rune in MIND_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        let mut singularity = fixtures::spell(SINGULARITY, fixtures::HAND, 0, "Singularity", 6, 2);
        singularity.domain = vec!["Mind".into()];
        fixture.table.cards.push(singularity);
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
                panic!("Singularity only opens target prompts: {:?}", answered.why);
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

    fn option(ctx: &Ctx, label: &str) -> u16 {
        labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx))) as u16
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let picked = option(ctx, label);
        pick(ctx, seat, picked)
    }

    fn resolve(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_card_is_registered_with_one_play_ability_over_up_to_two_units() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Singularity").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let spec = CARD.abilities[0].targets[0];
        assert_eq!((spec.min, spec.max), (0, 2));
        assert_eq!(spec.filter, crate::cards::prelude::UNIT);
    }

    #[test]
    fn two_chosen_units_take_six_each_and_die_at_cleanup() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, SINGULARITY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 2));
        assert!(prompt.cancel);
        assert_eq!(
            labels(&ctx),
            [
                "{card 50}",
                "{card 60}",
                "{card 81}",
                "done",
                "skip",
                "cancel"
            ]
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose up to two units (0 of 2)"
        );
        assert!(ctx.effects.is_empty(), "targets are chosen before paying");
        choose(&mut ctx, 0, "{card 60}").unwrap();
        choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(labels(&ctx), ["done", "cancel"]);
        choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::SPRITE),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
        let exhausted = ctx
            .effects
            .iter()
            .filter(|effect| {
                MIND_RUNES
                    .iter()
                    .any(|rune| **effect == Effect::exhaust(*rune))
            })
            .count();
        assert_eq!(exhausted, 6, "six energy: {:?}", ctx.effects);
        let recycled = ctx
            .effects
            .iter()
            .filter(|effect| {
                matches!(effect, Effect::Move { card, zone, .. }
                    if MIND_RUNES.contains(card) && Some(*zone) == ctx.zones.rune_deck)
            })
            .count();
        assert_eq!(recycled, 2, "two Mind power: {:?}", ctx.effects);
        for unit in [fixtures::SPRITE, fixtures::THEIR_UNIT] {
            assert!(ctx.events.contains(&Event::Chosen {
                card: unit,
                by: 0,
                item: 1
            }));
        }
        assert_eq!(priority::holder(&ctx), Some(0));
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for unit in [fixtures::SPRITE, fixtures::THEIR_UNIT] {
            assert!(ctx.events.contains(&Event::DamageDealt {
                card: unit,
                n: 6,
                source: Cause::Item(1)
            }));
        }
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, .. } if *card == fixtures::VI
        )));
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.card(SINGULARITY).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
        assert!(ctx.events.contains(&Event::PlayedSpell {
            item: 1,
            controller: 0,
            nth: 1
        }));
        assert!(ctx.blob.seat(0).played_main);
        assert!(ctx.blob.log.contains(&"{card 90} resolves".to_string()));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_surviving_unit_keeps_its_damage_and_a_target_that_left_is_skipped() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().might = Some(8);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, SINGULARITY).unwrap();
        choose(&mut ctx, 0, "{card 50}").unwrap();
        choose(&mut ctx, 0, "{card 81}").unwrap();
        choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.chain[0].targets.len(), 2);
        ctx.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::HAND);
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::VI,
            n: 6,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(fixtures::VI), 6);
        assert_eq!(ctx.current_might(fixtures::VI), 8);
        assert!(
            ctx.on_board(fixtures::VI),
            "six damage is not lethal to eight might"
        );
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::HAND),
            "a unit that left the board before resolution is no longer a target"
        );
        assert_eq!(ctx.card(SINGULARITY).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn up_to_means_none_is_fine_and_the_spell_still_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, SINGULARITY).unwrap();
        choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert!(
            ctx.effects.contains(&Effect::exhaust(MIND_RUNES[0])),
            "the cost is paid even with no targets"
        );
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.on_board(fixtures::SPRITE));
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert_eq!(ctx.card(SINGULARITY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.contains(&Event::PlayedSpell {
            item: 1,
            controller: 0,
            nth: 1
        }));
        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| !card.is_kind("Unit") || card.zone == Some(fixtures::HAND));
        empty.resolve();
        let mut ctx = empty.ctx();
        play_from_hand(&mut ctx, 0, SINGULARITY).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no unit anywhere: the target step is skipped"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve(&mut ctx);
        assert_eq!(ctx.card(SINGULARITY).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn cancel_takes_the_spell_back_unpaid() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, SINGULARITY).unwrap();
        choose(&mut ctx, 0, "{card 81}").unwrap();
        choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(
            ctx.effects,
            [Effect::Move {
                card: SINGULARITY,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }]
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn it_is_refused_off_turn_on_a_closed_chain_without_mind_power_and_for_a_non_unit() {
        let mut fixture = armed();
        let mut theirs = fixtures::spell(91, fixtures::HAND, 1, "Singularity", 6, 2);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        for rune in 106..110 {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Mind", false));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, 91)),
            Err(Refusal::NotYourTurn),
            "no Action or Reaction keyword: only on your own turn"
        );
        play_from_hand(&mut ctx, 0, SINGULARITY).unwrap();
        choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, 91)),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "holding priority is not enough for a card without Reaction"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, fixtures::HAND_UNIT)),
            Err(Refusal::Illegal(Reason::ChainClosed))
        );
        drop(ctx);
        let mut poor = armed();
        for rune in MIND_RUNES {
            poor.table.card_mut(rune).unwrap().domain = vec!["Fury".into()];
        }
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, SINGULARITY)),
            Err(Refusal::NoPowerOf),
            "six Fury runes pay the energy but not the Mind power"
        );
        drop(ctx);
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, SINGULARITY).unwrap();
        assert!(!labels(&ctx).contains(&"{card 72}".to_string()));
        for wrong in [
            fixtures::HAND_GEAR,
            fixtures::LEGEND_CARD,
            fixtures::GROUNDS,
            fixtures::HAND_UNIT,
            fixtures::CHAMPION_CARD,
            SINGULARITY,
        ] {
            assert_eq!(
                play::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::VI, fixtures::HAND_GEAR]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "one bad pick refuses the whole answer"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert_eq!(ctx.blob.chain.len(), 0);
        assert!(ctx.effects.is_empty(), "nothing was paid");
    }
}
