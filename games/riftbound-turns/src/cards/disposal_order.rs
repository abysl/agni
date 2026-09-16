use super::prelude::{card_targets, chosen_mode, done, draw, modal, mode, spell, target};
use super::{Card, Filter, Flow, Item, Keyword, ModeSpec, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;
pub const RECYCLES: u8 = 3;
pub const IN_AN_OPPONENTS_TRASH: Filter =
    Filter::And(&[Filter::InTrash, Filter::Not(&Filter::Owned)]);
pub const DISPOSED: TargetSpec = target(
    IN_AN_OPPONENTS_TRASH,
    0,
    RECYCLES,
    TargetKind::Card,
    "up to 3 cards in opponents' trashes to recycle",
);
pub const MODES: &[ModeSpec] = &[
    mode("Recycle", &[DISPOSED], recycle),
    mode("Draw 1", &[], draw_one),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Recycle,
    Draw,
}

pub fn mode_of(item: &Item) -> Option<Mode> {
    match chosen_mode(item)? {
        0 => Some(Mode::Recycle),
        1 => Some(Mode::Draw),
        _ => None,
    }
}

pub fn recycle_from_trash(ctx: &mut Ctx, card: u32) -> bool {
    if !ctx.in_trash(card) {
        return false;
    }
    let owner = ctx.owner(card);
    ctx.recycle_to_bottom(card);
    ctx.narrate(format!(
        "{{seat {owner}}} recycles {{card {card}}} from the trash"
    ));
    true
}

fn recycle(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for card in card_targets(ctx, item) {
        recycle_from_trash(ctx, card);
    }
    done()
}

fn draw_one(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = spell("Disposal Order", &[Keyword::Reaction], &[modal(MODES)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::chain::execution_view;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::priority;
    use crate::state::{
        ChainItem, ItemKind, Origin, PromptWhy, TargetRef, SLOT_PROMISED_REPEAT, SLOT_REPEAT,
    };
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const ORDER: u32 = 90;
    const THEIR_ORDER: u32 = 91;
    const THEIR_DEAD: [u32; 4] = [92, 93, 94, 95];
    const MY_DEAD: u32 = 96;
    const BODY_RUNE: u32 = 46;
    const THEIR_BODY: u32 = 47;

    fn order(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Disposal Order", 2, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn dump() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(order(ORDER, 0));
        fixture.table.cards.push(order(THEIR_ORDER, 1));
        for (index, id) in THEIR_DEAD.into_iter().enumerate() {
            fixture.table.cards.push(fixtures::unit(
                id,
                fixtures::TRASH,
                1,
                &format!("Corpse {index}"),
                2,
            ));
        }
        fixture
            .table
            .cards
            .push(fixtures::spell(MY_DEAD, fixtures::TRASH, 0, "Spent", 1, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(THEIR_BODY, 1, "Body", false));
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_reaction_over_two_named_modes() {
        assert!(std::ptr::eq(script_of("Disposal Order").unwrap(), &CARD));
        assert_eq!(CARD.name, "Disposal Order");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.modes.len(), 2);
        assert_eq!(ability.modes[0].label, "Recycle");
        assert_eq!(ability.modes[0].targets, &[DISPOSED]);
        assert_eq!(ability.modes[1].label, "Draw 1");
        assert!(ability.modes[1].targets.is_empty());
        assert_eq!((DISPOSED.min, DISPOSED.max), (0, 3));
        assert_eq!(DISPOSED.kind, TargetKind::Card);
        assert_eq!((DRAWS, RECYCLES), (1, 3));
        let mut item = ChainItem::new(1, ItemKind::Spell { card: ORDER }, 0, Origin::Hand);
        assert_eq!(mode_of(&item), None);
        item.set_mode(0, 0);
        assert_eq!(mode_of(&item), Some(Mode::Recycle));
        item.set_mode(0, 1);
        assert_eq!(mode_of(&item), Some(Mode::Draw));
        item.set_mode(0, 2);
        assert_eq!(mode_of(&item), None);
    }

    #[test]
    fn three_cards_from_the_opponents_trash_go_to_the_bottom_of_their_deck() {
        let mut fixture = dump();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ORDER).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Mode {
                item: 1,
                execution: 0
            })
        );
        fixtures::choose(&mut ctx, 0, "Recycle").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 92}",
                "{card 93}",
                "{card 94}",
                "{card 95}",
                "done",
                "skip",
                "cancel"
            ],
            "their trash, not mine"
        );
        for corpse in &THEIR_DEAD[..3] {
            fixtures::choose(&mut ctx, 0, &format!("{{card {corpse}}}")).unwrap();
        }
        assert!(
            ctx.blob.prompt.is_some(),
            "three is the most: the group waits for done"
        );
        assert_eq!(fixtures::labels(&ctx), ["done", "cancel"]);
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(92),
                TargetRef::Card(93),
                TargetRef::Card(94)
            ]
        );
        assert_eq!(mode_of(&ctx.blob.chain[0]), Some(Mode::Recycle));
        assert_eq!(deck_of(&ctx, 1), [24, 25], "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            deck_of(&ctx, 1),
            [94, 93, 92, 24, 25],
            "403.1.a · each to the bottom of its owner's deck, in pick order"
        );
        for corpse in &THEIR_DEAD[..3] {
            assert!(ctx.effects.contains(&Effect::Move {
                card: *corpse,
                zone: fixtures::MAIN_DECK,
                seat: 1,
                index: BOTTOM
            }));
            assert!(ctx.blob.log.contains(&format!(
                "{{seat 1}} recycles {{card {corpse}}} from the trash"
            )));
        }
        assert_eq!(ctx.trash_of(1), [95]);
        assert!(ctx.in_trash(MY_DEAD), "my trash is untouched");
        assert_eq!(drew(&ctx, 0), 0, "the other mode was not chosen");
        assert_eq!(ctx.card(ORDER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_draw_mode_asks_no_target_and_draws_one() {
        let mut fixture = dump();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, ORDER).unwrap();
        fixtures::choose(&mut ctx, 0, "Draw 1").unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target in the draw mode");
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert_eq!(mode_of(&ctx.blob.chain[0]), Some(Mode::Draw));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert_eq!(ctx.trash_of(1), THEIR_DEAD, "nothing recycled");
        assert_eq!(deck_of(&ctx, 1), [24, 25]);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn one_pick_is_enough_for_the_recycle_mode_and_a_card_that_left_the_trash_is_skipped() {
        let mut fixture = dump();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ORDER).unwrap();
        fixtures::choose(&mut ctx, 0, "Recycle").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(93, fixtures::HAND, 1), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(deck_of(&ctx, 1), [92, 24, 25]);
        assert_eq!(
            ctx.card(93).unwrap().zone,
            Some(fixtures::HAND),
            "a card no longer in the trash is left where it went"
        );
        assert_eq!(drew(&ctx, 0), 0, "one pick still means recycle, not draw");
    }

    #[test]
    fn the_other_seat_reacts_on_my_turn_and_reaches_into_my_trash() {
        let mut fixture = dump();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_ORDER).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["Recycle", "Draw 1", "cancel"]);
        fixtures::choose(&mut ctx, 1, "Recycle").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {MY_DEAD}}}"),
                "done".to_string(),
                "skip".to_string(),
                "cancel".to_string()
            ],
            "my trash alone; the Spark on the chain is not in it"
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {MY_DEAD}}}")).unwrap();
        fixtures::choose(&mut ctx, 1, "done").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(deck_of(&ctx, 0), [MY_DEAD, 20, 21, 22, 23]);
        assert!(ctx.trash_of(0).is_empty());
    }

    #[test]
    fn my_own_trash_a_unit_on_the_board_a_fourth_card_and_a_third_mode_are_refused() {
        let mut fixture = dump();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ORDER).unwrap();
        assert_eq!(
            play_engine::choose_mode(&mut ctx, 1, 0, 2),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "two modes only"
        );
        fixtures::choose(&mut ctx, 0, "Recycle").unwrap();
        for wrong in [MY_DEAD, fixtures::THEIR_UNIT, fixtures::THEIR_HAND_CARD] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not in an opponent's trash"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &THEIR_DEAD),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "up to three"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(ORDER).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn repeated_modes_keep_target_spans_when_the_middle_execution_has_no_targets() {
        let mut fixture = dump();
        let ctx = fixture.ctx();
        let mut item = ChainItem::new(1, ItemKind::Spell { card: ORDER }, 0, Origin::Hand);
        item.set_slot(SLOT_REPEAT, 1);
        item.set_slot(SLOT_PROMISED_REPEAT, 1);
        item.set_mode(0, 0);
        item.set_mode(1, 1);
        item.set_mode(2, 0);
        item.targets = vec![
            TargetRef::Card(THEIR_DEAD[0]),
            TargetRef::Card(THEIR_DEAD[1]),
        ];
        item.spec_counts = vec![1, 1];

        assert_eq!(crate::engine::targets::group_start(&ctx, &item, 0), 0);
        assert_eq!(crate::engine::targets::group_start(&ctx, &item, 1), 1);

        let first = execution_view(&ctx, &item);
        assert_eq!(first.targets, [TargetRef::Card(THEIR_DEAD[0])]);
        assert_eq!(first.spec_counts, [1]);
        let mut middle = item.clone();
        middle.execution = 1;
        let middle = execution_view(&ctx, &middle);
        assert!(middle.targets.is_empty());
        assert!(middle.spec_counts.is_empty());
        let mut last = item;
        last.execution = 2;
        let last = execution_view(&ctx, &last);
        assert_eq!(last.targets, [TargetRef::Card(THEIR_DEAD[1])]);
        assert_eq!(last.spec_counts, [1]);
    }

    #[test]
    fn the_play_asks_which_mode_by_name_before_any_pick() {
        let mut fixture = dump();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ORDER).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["Recycle", "Draw 1", "cancel"]);
    }
}
