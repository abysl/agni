use super::prelude::{card_target, done, draw, kill, play, spell, target, GEAR};
use super::{Card, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::{Ctx, Killed};

pub const CARDS: usize = 1;

const VICTIM: TargetSpec = target(GEAR, 0, 1, TargetKind::Card, "a gear to kill");

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(gear) = card_target(ctx, item, 0) {
        if kill(ctx, item, gear) == Killed::Yes {
            ctx.narrate(format!("{{card {gear}}} dies"));
        }
    }
    draw(ctx, item.controller, CARDS);
    done()
}

pub static CARD: Card = spell("Salvage", &[Keyword::Action], &[play(&[VICTIM], resolve)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const SALVAGE: u32 = 90;
    const THEIR_SALVAGE: u32 = 91;
    const MY_GEAR: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const ORDER_RUNE: u32 = 100;

    fn salvage(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Salvage", 2, 1);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(salvage(SALVAGE, 0));
        fixture.table.cards.push(salvage(THEIR_SALVAGE, 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
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

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_an_action_with_one_optional_gear_target() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Salvage").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Salvage");
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, [VICTIM]);
        assert_eq!((VICTIM.min, VICTIM.max), (0, 1));
        assert_eq!(VICTIM.filter, GEAR);
    }

    #[test]
    fn salvage_kills_the_chosen_gear_of_either_side_and_draws_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SALVAGE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 93}", "skip", "cancel"],
            "any gear on the board, or none"
        );
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_GEAR)]);
        assert!(ctx.on_board(THEIR_GEAR), "nothing before it resolves");
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(MY_GEAR));
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert!(ctx.blob.log.contains(&"{card 93} dies".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(SALVAGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn declining_the_kill_still_draws_and_so_does_a_board_with_no_gear() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SALVAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain[0].targets.is_empty());
        both_pass(&mut ctx);
        assert!(ctx.on_board(MY_GEAR) && ctx.on_board(THEIR_GEAR));
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(ctx.hand_of(0).len(), hand);
        drop(ctx);

        let mut bare = armed();
        bare.table
            .cards
            .retain(|card| ![MY_GEAR, THEIR_GEAR].contains(&card.id));
        bare.resolve();
        let mut ctx = bare.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SALVAGE).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no gear to offer: the may is answered for you"
        );
        both_pass(&mut ctx);
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.card(SALVAGE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_gear_that_left_the_board_is_left_alone_and_the_draw_still_happens() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SALVAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(THEIR_GEAR, fixtures::HAND, 1), 1)
            .unwrap();
        both_pass(&mut ctx);
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().zone, Some(fixtures::HAND));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert_eq!(drew(&ctx, 0), 1, "356.3.e.5 · the draw is not a target");
    }

    #[test]
    fn units_and_hidden_cards_are_refused_and_the_action_waits_for_your_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SALVAGE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, SALVAGE).unwrap();
        for wrong in [
            fixtures::VI,
            fixtures::THEIR_UNIT,
            fixtures::HAND_GEAR,
            ORDER_RUNE,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a gear on the board"
            );
        }
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(SALVAGE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
