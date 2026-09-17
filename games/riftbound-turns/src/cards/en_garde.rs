use super::prelude::{a_friendly_unit, alone_there, card_target, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub static CARD: Card = spell(
    "En Garde",
    &[Keyword::Reaction],
    &[play(&[a_friendly_unit("a friendly unit")], guard)],
);

fn guard(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, 1, None);
        if alone_there(ctx, unit) {
            might_this_turn(ctx, item, unit, 1, None);
        }
    }
    Flow::Done
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Cause, EntryMove, Location, Token, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, phases, play, priority, prompts, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::Target;

    const EN_GARDE: u32 = 90;
    const THEIR_EN_GARDE: u32 = 91;

    fn en_garde(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "En Garde", 1, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(en_garde(EN_GARDE, 0));
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
            match (answered.why, answered.answer) {
                (PromptWhy::Target { item, .. }, Answer::Cancel) => play::cancel(ctx, item),
                (PromptWhy::Target { item, spec }, _) => {
                    play::choose_targets(ctx, item, spec, &answered.prompt.picked)?
                }
                (why, _) => panic!("En Garde opens only target prompts, got {why:?}"),
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

    fn resolve_by_passing(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.priority.is_none());
    }

    fn might_counters(ctx: &Ctx, card: u32) -> Vec<i32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Counter {
                    target: Target::Card(held),
                    counter: COUNTER_MIGHT,
                    delta,
                } if *held == card => Some(*delta),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_card_is_a_reaction_with_one_friendly_unit_target() {
        assert_eq!(CARD.name, "En Garde");
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert!(std::ptr::eq(
            crate::cards::script_of("En Garde").unwrap(),
            &CARD
        ));
    }

    #[test]
    fn a_unit_alone_in_its_base_gets_two_might_until_the_turn_ends() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, EN_GARDE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(labels(&ctx), ["{card 50}", "cancel"]);
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a friendly unit (0 of 1)"
        );
        assert!(ctx.effects.is_empty(), "the target comes before the cost");
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(
            ctx.effects,
            [Effect::exhaust(41)],
            "one energy from the first ready rune"
        );
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        resolve_by_passing(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(might_counters(&ctx, fixtures::VI), [1, 1]);
        assert_eq!(ctx.card(EN_GARDE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert!(ctx.blob.is_neutral_open());
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(might_counters(&ctx, fixtures::VI), [-2]);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
    }

    #[test]
    fn a_unit_with_company_gets_only_one_and_the_prompt_offers_every_friendly_unit() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, EN_GARDE).unwrap();
        assert_eq!(labels(&ctx), ["{card 50}", "{card 70}", "cancel"]);
        pick(&mut ctx, 0, 0).unwrap();
        resolve_by_passing(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(might_counters(&ctx, fixtures::VI), [1]);
        assert_eq!(ctx.current_might(fixtures::HAND_UNIT), 2);
        assert!(might_counters(&ctx, fixtures::HAND_UNIT).is_empty());
    }

    #[test]
    fn being_alone_is_read_when_the_spell_resolves_not_when_it_is_played() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, EN_GARDE).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.alone_at(fixtures::VI));
        let sprite = ctx
            .spawn(0, Token::Sprite, Location::Base(0), true)
            .unwrap();
        assert!(!ctx.alone_at(fixtures::VI));
        resolve_by_passing(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(might_counters(&ctx, fixtures::VI), [1]);
        assert!(might_counters(&ctx, sprite).is_empty());
    }

    #[test]
    fn a_target_that_left_the_board_gets_nothing_and_the_spell_still_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, EN_GARDE).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        ctx.kill(fixtures::VI, Cause::Rule);
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        resolve_by_passing(&mut ctx);
        assert!(might_counters(&ctx, fixtures::VI).is_empty());
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(EN_GARDE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn an_enemy_unit_is_refused_as_a_target_and_a_seat_without_units_can_only_cancel() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, EN_GARDE).unwrap();
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in hand is not on the board"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.blob.chain.is_empty());
        let mut lonely = armed();
        lonely.table.cards.retain(|card| card.id != fixtures::VI);
        lonely.resolve();
        let mut ctx = lonely.ctx();
        play_from_hand(&mut ctx, 0, EN_GARDE).unwrap();
        assert_eq!(labels(&ctx), ["cancel"]);
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(EN_GARDE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn the_other_seat_waits_for_priority_before_reacting() {
        let mut fixture = armed();
        fixture.table.cards.push(en_garde(THEIR_EN_GARDE, 1));
        fixture.table.card_mut(44).unwrap().domain = vec!["Calm".into()];
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_EN_GARDE)),
            Err(Refusal::NotYourTurn),
            "no chain, not their turn"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_EN_GARDE)),
            Err(Refusal::Illegal(Reason::ChainClosed))
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, THEIR_EN_GARDE).unwrap();
        assert_eq!(
            labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "seat 1's friendly units are its sprite and Jinx"
        );
        pick(&mut ctx, 1, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(
            ctx.blob.chain[1].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4);
        assert_eq!(might_counters(&ctx, fixtures::THEIR_UNIT), [1, 1]);
    }
}
