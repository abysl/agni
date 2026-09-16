use super::possession::ENEMY_UNIT_AT_A_BATTLEFIELD;
use super::prelude::{a_card, asking, card_target, done, play, spell, with_candidates};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::attach;
use crate::engine::ctx::Ctx;
use crate::engine::hide;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};

pub const PLACED: u8 = 1;
pub const QUESTION: &str =
    "the unit again to place it on top of your Main Deck, or skip for the bottom";

pub fn place_in_main_deck(ctx: &mut Ctx, unit: u32, on_top: bool) -> bool {
    if !ctx.on_board(unit) {
        return false;
    }
    let owner = ctx.owner(unit);
    attach::detach_all(ctx, unit);
    if attach::is_attached(ctx, unit) {
        attach::detach(ctx, unit);
    }
    ctx.clear_designation(unit);
    if ctx.is_token(unit) {
        ctx.emit(Effect::Despawn { card: unit });
        ctx.blob.drop_card_state(unit);
        ctx.narrate(format!("{{card {unit}}} is a token and ceases to exist"));
        return true;
    }
    let Some(deck) = ctx.zones.main_deck else {
        return false;
    };
    hide::reveal_before(ctx, unit, deck);
    ctx.emit(Effect::Move {
        card: unit,
        zone: deck,
        seat: owner,
        index: if on_top { TOP } else { BOTTOM },
    });
    ctx.blob.drop_card_state(unit);
    let where_to = if on_top { "top" } else { "bottom" };
    ctx.narrate(format!(
        "{{seat {owner}}} places {{card {unit}}} on the {where_to} of their Main Deck"
    ));
    true
}

fn the_unit(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != PLACED {
        return Vec::new();
    }
    card_target(ctx, item, 0)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn verdict(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    match stage.0 {
        PLACED => {
            let on_top = ctx.picks().contains(&unit);
            place_in_main_deck(ctx, unit, on_top);
            done()
        }
        _ => {
            if ctx.is_token(unit) {
                place_in_main_deck(ctx, unit, false);
                return done();
            }
            let owner = ctx.owner(unit);
            Flow::Ask(ctx.ask_seat_resume(item, owner, PLACED, 0, 1))
        }
    }
}

pub static CARD: Card = spell(
    "Keeper's Verdict",
    &[Keyword::Action],
    &[asking(
        with_candidates(
            play(
                &[a_card(
                    ENEMY_UNIT_AT_A_BATTLEFIELD,
                    "an enemy unit at a battlefield",
                )],
                verdict,
            ),
            the_unit,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attach_gear, Attached};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, prompts};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const VERDICT: u32 = 90;
    const THEIR_VERDICT: u32 = 91;
    const BRUTE: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const BODY_RUNE: u32 = 46;
    const ORDER_RUNE: u32 = 47;

    fn verdict_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Keeper's Verdict", 2, 2);
        card.domain = vec!["Body".into(), "Order".into()];
        card
    }

    fn court() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(verdict_card(VERDICT, 0));
        fixture.table.cards.push(verdict_card(THEIR_VERDICT, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.table.cards.push(fixtures::gear(
            THEIR_GEAR,
            fixtures::BF1,
            1,
            "Long Sword",
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
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

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, VERDICT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_an_action_over_an_enemy_unit_at_a_battlefield_with_the_owners_placement() {
        assert!(std::ptr::eq(script_of("Keeper's Verdict").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, ENEMY_UNIT_AT_A_BATTLEFIELD);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_owner_places_the_unit_on_top_of_their_main_deck_by_picking_it_again() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        assert_eq!(attach_gear(&mut ctx, THEIR_GEAR, BRUTE), Attached::Yes);
        let deck = deck_of(&ctx, 1);
        cast(&mut ctx, BRUTE);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PLACED
            })
        );
        assert_eq!(
            ctx.blob.prompt.as_ref().map(|prompt| prompt.seat),
            Some(1),
            "its owner chooses"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BRUTE}}}"), "skip".to_string()]
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 })),
            "the caster does not place it"
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert_eq!(ctx.card(BRUTE).unwrap().seat, 1, "their deck, not yours");
        assert_eq!(
            deck_of(&ctx, 1).last().copied(),
            Some(BRUTE),
            "on top: the next card they draw"
        );
        assert_eq!(ctx.top_of(fixtures::MAIN_DECK, 1, 1), [BRUTE]);
        assert_eq!(deck_of(&ctx, 1).len(), deck.len() + 1);
        assert!(!ctx.on_board(BRUTE));
        assert!(
            !attach::is_attached(&ctx, THEIR_GEAR),
            "the gear it wore is detached as it leaves the board"
        );
        assert!(ctx.on_board(THEIR_GEAR));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 1}} places {{card {BRUTE}}} on the top of their Main Deck"
        )));
        assert_eq!(ctx.card(VERDICT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_places_it_on_the_bottom() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, BRUTE);
        fixtures::choose(&mut ctx, 1, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert_eq!(
            deck_of(&ctx, 1).first().copied(),
            Some(BRUTE),
            "on the bottom"
        );
        assert_ne!(ctx.top_of(fixtures::MAIN_DECK, 1, 1), [BRUTE]);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 1}} places {{card {BRUTE}}} on the bottom of their Main Deck"
        )));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_token_has_no_deck_to_go_to_and_simply_ceases_to_exist() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, fixtures::SPRITE);
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing to place, nothing to ask"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert_eq!(
            ctx.blob.holder(fixtures::BF2),
            None,
            "the last unit gone leaves the hold behind"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} is a token and ceases to exist".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_unit_that_left_the_battlefield_before_resolution_is_left_alone() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, VERDICT).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        let base = ctx.zones.base.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BRUTE, base, 1), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::BASE));
        assert_eq!(ctx.card(VERDICT).unwrap().zone, Some(fixtures::TRASH));
        assert!(
            !place_in_main_deck(&mut ctx, VERDICT, true),
            "a card off the board"
        );
    }

    #[test]
    fn it_waits_for_your_turn_and_refuses_friendly_units_and_enemies_in_their_base() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_VERDICT)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, VERDICT).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "the enemy units at battlefields only"
        );
        for wrong in [
            fixtures::VI,
            fixtures::THEIR_UNIT,
            THEIR_GEAR,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit at a battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(VERDICT).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
