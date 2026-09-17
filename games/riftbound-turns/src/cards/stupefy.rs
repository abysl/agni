use super::prelude::{a_unit, card_target, done, draw, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const SHRINK: i16 = -1;
pub const FLOOR: i32 = 1;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, SHRINK, Some(FLOOR));
    }
    draw(ctx, item.controller, 1);
    done()
}

pub static CARD: Card = spell(
    "Stupefy",
    &[Keyword::Reaction],
    &[play(&[a_unit("a unit")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{EntryMove, Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, phases, play as plays, priority, prompts, settle};
    use crate::state::{Expiry, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::Target;

    const THEIR_STUPEFY: u32 = 82;

    fn stupefy(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Stupefy", 1, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let spark = fixture
            .table
            .cards
            .iter()
            .position(|card| card.id == fixtures::HAND_SPELL)
            .unwrap();
        fixture.table.cards[spark] = stupefy(fixtures::HAND_SPELL, 0);
        fixture.table.cards.push(stupefy(THEIR_STUPEFY, 1));
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
        plays::begin(ctx, seat, card, Origin::Hand, None)?;
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
        let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? else {
            return settle(ctx);
        };
        match (answered.why, answered.answer) {
            (PromptWhy::Target { item, .. }, Answer::Cancel) => plays::cancel(ctx, item),
            (PromptWhy::Target { item, spec }, _) => {
                plays::choose_targets(ctx, item, spec, &answered.prompt.picked)?
            }
            (other, _) => panic!("unexpected prompt {other:?}"),
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

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_registered_as_a_reaction_with_one_unit_target() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Stupefy").unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, crate::cards::Filter::Unit);
    }

    #[test]
    fn stupefy_shrinks_a_unit_by_one_this_turn_draws_one_and_the_mod_expires() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand_before = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friendly or enemy, is a candidate"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 71}: choose a unit (0 of 1)"
        );
        assert!(ctx.effects.is_empty(), "targets come before the cost");
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(ctx.events.contains(&Event::Chosen {
            card: fixtures::VI,
            by: 0,
            item: 1
        }));
        assert_eq!(ctx.effects.len(), 1, "{:?}", ctx.effects);
        assert!(
            (41..=43).any(|rune| ctx.effects[0] == Effect::exhaust(rune)),
            "one energy, no power: a single ready rune is exhausted: {:?}",
            ctx.effects
        );
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "nothing happens before it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::VI), -1);
        assert_eq!(ctx.current_might(fixtures::VI), 2);
        assert_eq!(
            ctx.state_of(fixtures::VI).unwrap().might[0].until,
            Expiry::EndOfTurn(ctx.turn())
        );
        assert_eq!(ctx.hand_of(0).len(), hand_before, "one played, one drawn");
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 71} resolves");
        assert!(ctx.blob.seat(0).played_main);
        assert!(ctx.blob.is_neutral_open());
        phases::end_turn(&mut ctx).unwrap();
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(fixtures::VI),
            counter: COUNTER_MIGHT,
            delta: 1
        }));
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx
            .state_of(fixtures::VI)
            .is_none_or(|row| row.might.is_empty()));
    }

    #[test]
    fn the_minimum_of_one_is_snapshotted_so_a_one_might_unit_is_untouched_and_still_draws() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(1);
        let mut ctx = fixture.ctx();
        let hand_before = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        pick(&mut ctx, 0, 2).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 1);
        assert!(ctx
            .state_of(fixtures::THEIR_UNIT)
            .is_none_or(|row| row.might.is_empty()));
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Counter { target: Target::Card(card), counter: COUNTER_MIGHT, .. }
                if *card == fixtures::THEIR_UNIT
        )));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand_before,
            "the draw does not depend on the shrink"
        );
        let until = Expiry::EndOfTurn(ctx.turn());
        ctx.might(fixtures::THEIR_UNIT, 2, until, None, 0);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            3,
            "the clamped -1 was remembered as 0, not applied later"
        );
    }

    #[test]
    fn the_other_seat_needs_priority_and_a_non_unit_is_refused_as_a_target() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STUPEFY)),
            Err(Refusal::NotYourTurn),
            "a reaction has no window in the other seat's neutral open state"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STUPEFY)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 1 waits for priority"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        let their_hand = ctx.hand_of(1).len();
        play_from_hand(&mut ctx, 1, THEIR_STUPEFY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        assert_eq!(
            plays::choose_targets(&mut ctx, 2, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a battlefield is not a unit"
        );
        assert_eq!(
            plays::choose_targets(&mut ctx, 2, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in hand is not on the board"
        );
        assert_eq!(
            plays::choose_targets(&mut ctx, 2, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        pick(&mut ctx, 1, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "their Stupefy resolved first");
        assert_eq!(ctx.current_might(fixtures::VI), 2);
        assert_eq!(ctx.hand_of(1).len(), their_hand, "seat 1 drew");
        assert!(ctx.events.contains(&Event::Drew { seat: 1, nth: 1 }));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::VI), -2, "both copies stack");
        assert_eq!(ctx.current_might(fixtures::VI), 1);
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn stupefy_is_refused_when_the_target_needs_a_unit_and_none_is_on_the_board() {
        let mut fixture = armed();
        fixture.table.cards.retain(|card| {
            card.kind.as_deref() != Some("Unit") || card.zone == Some(fixtures::HAND)
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let offered = labels(&ctx);
        assert!(
            !offered.iter().any(|label| label.starts_with("{card")),
            "no unit is offered: {offered:?}"
        );
        let cancel = offered.iter().position(|label| label == "cancel").unwrap() as u16;
        pick(&mut ctx, 0, cancel).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(ctx.blob.is_neutral_open());
    }
}
