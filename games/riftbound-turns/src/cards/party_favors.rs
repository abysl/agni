use super::prelude::{
    asking, channel_exhausted, choosing, chosen_mode, done, draw, mode, play, run_mode, spell,
};
use super::{Card, Flow, Item, ModeSpec, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;
pub const RUNES: usize = 1;
pub const QUESTION: &str = "Cards or Runes";
pub const MODES: &[ModeSpec] = &[mode("Cards", &[], cards), mode("Runes", &[], runes)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Cards,
    Runes,
}

pub fn mode_of(item: &Item) -> Option<Mode> {
    match chosen_mode(item)? {
        0 => Some(Mode::Cards),
        1 => Some(Mode::Runes),
        _ => None,
    }
}

pub fn other_seats(ctx: &Ctx, seat: u8) -> Vec<u8> {
    let order = ctx.blob.order();
    let mut seats = Vec::new();
    let mut next = order.next_seat(seat);
    while next != seat && !seats.contains(&next) {
        seats.push(next);
        next = order.next_seat(next);
    }
    seats
}

fn chooser_at(ctx: &Ctx, item: &Item, stage: Stage) -> Option<u8> {
    let index = usize::from(stage.0).checked_sub(1)?;
    other_seats(ctx, item.controller).get(index).copied()
}

fn cards(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if let Some(chooser) = chooser_at(ctx, item, stage) {
        ctx.narrate(format!("{{seat {chooser}}} chooses Cards"));
        draw(ctx, item.controller, DRAWS);
        draw(ctx, chooser, DRAWS);
    }
    done()
}

fn runes(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if let Some(chooser) = chooser_at(ctx, item, stage) {
        ctx.narrate(format!("{{seat {chooser}}} chooses Runes"));
        channel_exhausted(ctx, item.controller, RUNES);
        channel_exhausted(ctx, chooser, RUNES);
    }
    done()
}

fn favors(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if chooser_at(ctx, item, stage).is_some() {
        run_mode(ctx, item, stage);
    }
    let next = usize::from(stage.0);
    match other_seats(ctx, item.controller).get(next).copied() {
        Some(seat) => {
            let stage = u8::try_from(next + 1).unwrap_or(u8::MAX);
            Flow::Ask(ctx.ask_seat_resume(item, seat, stage, 1, 1))
        }
        None => done(),
    }
}

pub static CARD: Card = spell(
    "Party Favors",
    &[],
    &[asking(choosing(play(&[], favors), MODES), QUESTION)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts};
    use crate::state::{ChainItem, ItemKind, ItemStatus, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const FAVORS: u32 = 90;
    const MY_TOP_RUNE: u32 = 32;
    const THEIR_TOP_RUNE: u32 = 35;

    fn favors_card(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(FAVORS, fixtures::HAND, seat, "Party Favors", 3, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn party() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(favors_card(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FAVORS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn pool_size(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, seat).count()
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, FAVORS).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no targets: the choices come on resolution"
        );
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume { item: 1, stage: 1 }),
            "the other seat is asked first"
        );
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Resolving);
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        assert_eq!(
            fixtures::labels(ctx),
            ["Cards", "Runes"],
            "the two modes by name; no skip"
        );
        assert_eq!(
            prompts::status(ctx, PromptWhy::Resume { item: 1, stage: 1 }),
            "{seat 1}: choose Cards or Runes (0 of 1)"
        );
    }

    #[test]
    fn the_script_is_a_plain_spell_asking_each_other_seat_for_a_mode() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Party Favors").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_none());
        assert_eq!(ability.modes.len(), 2);
        assert_eq!(ability.modes[0].label, "Cards");
        assert_eq!(ability.modes[1].label, "Runes");
        assert!(ability.modes.iter().all(|mode| mode.targets.is_empty()));
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!((DRAWS, RUNES), (1, 1));
        let mut fixture = party();
        let ctx = fixture.ctx();
        assert_eq!(other_seats(&ctx, 0), [1]);
        assert_eq!(other_seats(&ctx, 1), [0]);
        let mut item = ChainItem::new(1, ItemKind::Spell { card: FAVORS }, 0, Origin::Hand);
        assert_eq!(mode_of(&item), None);
        item.set_mode(0, 0);
        assert_eq!(mode_of(&item), Some(Mode::Cards));
        item.set_mode(0, 1);
        assert_eq!(mode_of(&item), Some(Mode::Runes));
        item.set_mode(0, 2);
        assert_eq!(mode_of(&item), None);
    }

    #[test]
    fn choosing_cards_draws_one_for_both_players() {
        let mut fixture = party();
        let mut ctx = fixture.ctx();
        let my_hand = ctx.hand_of(0).len();
        let their_hand = ctx.hand_of(1).len();
        cast(&mut ctx);
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt: 1,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 })),
            "the host does not choose for the guest"
        );
        fixtures::choose(&mut ctx, 1, "Cards").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "two seats: one guest, one question"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(drew(&ctx, 1), DRAWS);
        assert_eq!(ctx.hand_of(0).len(), my_hand, "one played, one drawn");
        assert_eq!(ctx.hand_of(1).len(), their_hand + 1);
        assert_eq!(pool_size(&ctx, 0), 4);
        assert_eq!(pool_size(&ctx, 1), 2);
        assert!(ctx.blob.log.contains(&"{seat 1} chooses Cards".to_string()));
        assert_eq!(ctx.card(FAVORS).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn choosing_runes_channels_one_exhausted_for_both_players() {
        let mut fixture = party();
        let mut ctx = fixture.ctx();
        let my_hand = ctx.hand_of(0).len();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 1, "Runes").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), 0);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), my_hand - 1);
        assert_eq!(pool_size(&ctx, 0), 5);
        assert_eq!(pool_size(&ctx, 1), 3);
        for rune in [MY_TOP_RUNE, THEIR_TOP_RUNE] {
            assert_eq!(ctx.card(rune).unwrap().zone, Some(fixtures::RUNE_POOL));
            assert!(ctx.card(rune).unwrap().exhausted, "it arrives exhausted");
        }
        assert!(ctx.blob.log.contains(&"{seat 1} chooses Runes".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} channels 1 rune exhausted".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_guest_with_an_empty_rune_deck_still_lets_the_host_channel() {
        let mut fixture = party();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::RUNE_DECK) || card.owner != 1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 1, "Runes").unwrap();
        assert_eq!(pool_size(&ctx, 0), 5);
        assert_eq!(pool_size(&ctx, 1), 2, "nothing to channel");
        assert_eq!(drew(&ctx, 1), 0, "no draw in place of a missing rune");
    }

    #[test]
    fn the_modes_are_offered_by_name() {
        let mut fixture = party();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert_eq!(fixtures::labels(&ctx), ["Cards", "Runes"]);
    }
}
