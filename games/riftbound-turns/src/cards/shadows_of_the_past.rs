use super::morbid_return::return_from_trash;
use super::prelude::{card_targets, done, play, spell, target};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec, KIND_UNIT};
use crate::engine::ctx::Ctx;

pub const RETURNS: u8 = 2;
pub const UNIT_IN_ANY_TRASH: Filter = Filter::And(&[Filter::Kind(KIND_UNIT), Filter::InTrash]);
pub const RETURNED: TargetSpec = target(
    UNIT_IN_ANY_TRASH,
    0,
    RETURNS,
    TargetKind::Card,
    "up to 2 units in trashes to return to their owners' hands",
);

fn recall(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let units = card_targets(ctx, item);
    if units.is_empty() {
        ctx.narrate(format!("{{seat {}}} returns nothing", item.controller));
    }
    for unit in units {
        return_from_trash(ctx, unit);
    }
    done()
}

pub static CARD: Card = spell("Shadows of the Past", &[], &[play(&[RETURNED], recall)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const SHADOWS: u32 = 90;
    const THEIR_SHADOWS: u32 = 91;
    const MY_DEAD: [u32; 2] = [92, 93];
    const THEIR_DEAD: u32 = 94;
    const MY_SPENT_SPELL: u32 = 95;
    const THEIR_DEAD_GEAR: u32 = 96;
    const CHAOS_RUNE: u32 = 46;

    fn shadows(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Shadows of the Past", 3, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn graveyard() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shadows(SHADOWS, 0));
        fixture.table.cards.push(shadows(THEIR_SHADOWS, 1));
        for (index, id) in MY_DEAD.into_iter().enumerate() {
            fixture.table.cards.push(fixtures::unit(
                id,
                fixtures::TRASH,
                0,
                &format!("Fallen {index}"),
                2,
            ));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_DEAD, fixtures::TRASH, 1, "Corpse", 2));
        fixture.table.cards.push(fixtures::spell(
            MY_SPENT_SPELL,
            fixtures::TRASH,
            0,
            "Spent",
            1,
            0,
        ));
        fixture.table.cards.push(fixtures::gear(
            THEIR_DEAD_GEAR,
            fixtures::TRASH,
            1,
            "Scrap",
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
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

    #[test]
    fn the_script_is_a_sorcery_over_up_to_two_units_in_any_trash() {
        assert!(std::ptr::eq(
            script_of("Shadows of the Past").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Shadows of the Past");
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, &[RETURNED]);
        assert_eq!((RETURNED.min, RETURNED.max), (0, RETURNS));
        assert_eq!(RETURNED.kind, TargetKind::Card);
        assert!(ability.candidates.is_none());
        assert_eq!(RETURNS, 2);
    }

    #[test]
    fn two_units_from_either_trash_go_to_their_owners_hands() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        let mine = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, SHADOWS).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 92}",
                "{card 93}",
                "{card 94}",
                "done",
                "skip",
                "cancel"
            ],
            "units in every trash; the spell and the gear are not units"
        );
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 94}").unwrap();
        assert!(
            ctx.blob.prompt.is_some(),
            "two is the most: the group waits for done"
        );
        assert_eq!(fixtures::labels(&ctx), ["done", "cancel"]);
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(93), TargetRef::Card(94)]
        );
        assert!(
            ctx.in_trash(93) && ctx.in_trash(94),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(93).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(93).unwrap().seat, 0);
        assert_eq!(ctx.card(94).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(
            ctx.card(94).unwrap().seat,
            1,
            "to its owner's hand, not mine"
        );
        assert_eq!(ctx.hand_of(0).len(), mine, "one played, one returned");
        assert_eq!(ctx.hand_of(1).len(), theirs + 1);
        for (card, seat) in [(93, 0), (94, 1)] {
            assert!(ctx.effects.contains(&Effect::Move {
                card,
                zone: fixtures::HAND,
                seat,
                index: TOP
            }));
            assert!(ctx
                .blob
                .log
                .contains(&format!("{{card {card}}} returns from the trash to hand")));
        }
        assert!(ctx.in_trash(92), "the third stays");
        assert!(ctx.in_trash(MY_SPENT_SPELL));
        assert!(ctx.in_trash(THEIR_DEAD_GEAR));
        assert_eq!(ctx.blob.seat(0).draws, 0, "a return is not a draw");
        assert_eq!(ctx.card(SHADOWS).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn one_pick_is_enough_skipping_returns_nothing_and_a_unit_that_left_the_trash_is_passed_over() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHADOWS).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(92).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.in_trash(93));
        drop(ctx);
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SHADOWS).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain[0].targets.is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.in_trash(92) && ctx.in_trash(93) && ctx.in_trash(THEIR_DEAD));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} returns nothing".to_string()));
        drop(ctx);
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHADOWS).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 94}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(94, fixtures::BANISHMENT, 1), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(92).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(
            ctx.card(94).unwrap().zone,
            Some(fixtures::BANISHMENT),
            "a unit no longer in a trash is left where it went"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn spells_gear_board_units_and_a_third_unit_are_refused_and_the_other_seat_waits_its_turn() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SHADOWS)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, SHADOWS).unwrap();
        for wrong in [
            MY_SPENT_SPELL,
            THEIR_DEAD_GEAR,
            fixtures::VI,
            fixtures::THEIR_UNIT,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit in a trash"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[92, 93, THEIR_DEAD]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "up to two"
        );
        assert!(fixtures::choose(&mut ctx, 1, "{card 92}").is_err());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(SHADOWS).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_trash(92) && ctx.in_trash(93) && ctx.in_trash(THEIR_DEAD));
        assert!(ctx.blob.is_neutral_open());
    }
}
