use super::prelude::{a_unit, card_target, done, draw, play, spell, stun};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::Origin;

const CARDS: usize = 1;

pub fn played_from_hand(item: &Item) -> bool {
    matches!(item.origin, Origin::Hand)
}

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        stun(ctx, unit);
    }
    if played_from_hand(item) {
        draw(ctx, item.controller, CARDS);
    }
    done()
}

pub static CARD: Card = spell(
    "Back Off",
    &[Keyword::Hidden, Keyword::Action],
    &[play(&[a_unit("a unit")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{EntryMove, Event, ANNOTATION_STUNNED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{hide, play as play_engine, priority, prompts, settle, targets};
    use crate::state::{
        ChainItem, ItemKind, PlayLock, Priority, PromptWhy, Showdown, TargetRef,
        FLAG_FROM_FACEDOWN, FLAG_STUNNED,
    };
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::CardInfo;

    const BACK_OFF: u32 = 90;
    const THEIR_BACK_OFF: u32 = 91;
    const ENERGY: u8 = 3;

    fn back_off(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Back Off", ENERGY, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(back_off(BACK_OFF, 0));
        fixture.table.cards.push(back_off(THEIR_BACK_OFF, 1));
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
        play_engine::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
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
            (PromptWhy::Target { item, .. }, Answer::Cancel) => play_engine::cancel(ctx, item),
            (PromptWhy::Target { item, spec }, _) => {
                play_engine::choose_targets(ctx, item, spec, &answered.prompt.picked)?
            }
            (why, answer) => panic!("Back Off opens only target prompts: {why:?} {answer:?}"),
        }
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn stun_lines(ctx: &Ctx) -> usize {
        ctx.blob
            .log
            .iter()
            .filter(|line| line.ends_with("is stunned"))
            .count()
    }

    #[test]
    fn the_script_is_a_hidden_action_with_one_unit_target() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Back Off").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Back Off");
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, crate::cards::Trigger::Play);
        assert!(!ability.optional);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].kind, crate::cards::TargetKind::Card);
        assert_eq!(ability.targets[0].filter, crate::cards::prelude::UNIT);
    }

    #[test]
    fn back_off_stuns_the_chosen_unit_and_draws_one_when_it_came_from_hand() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, BACK_OFF).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit on the board, in a base or at a battlefield"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a unit (0 of 1)"
        );
        assert!(ctx.effects.is_empty(), "targets come before the cost");
        pick(&mut ctx, 0, 2).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(ctx.blob.chain[0].origin, Origin::Hand);
        assert!(ctx.events.contains(&Event::Chosen {
            card: fixtures::THEIR_UNIT,
            by: 0,
            item: 1
        }));
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(41),
                Effect::exhaust(42),
                Effect::exhaust(43)
            ],
            "three energy off ready runes, no power"
        );
        assert!(
            !ctx.is_stunned(fixtures::THEIR_UNIT),
            "nothing until it resolves"
        );
        assert_eq!(priority::holder(&ctx), Some(0));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert!(ctx.has_flag(fixtures::THEIR_UNIT, FLAG_STUNNED));
        assert!(ctx.effects.contains(&Effect::Annotate {
            card: fixtures::THEIR_UNIT,
            key: ANNOTATION_STUNNED.into(),
            value: Some(vec![1])
        }));
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "410.1.c: the printed might is untouched"
        );
        assert_eq!(
            ctx.combat_might(fixtures::THEIR_UNIT),
            0,
            "410.1.b: it deals no combat damage this turn"
        );
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert!(ctx.effects.contains(&Effect::Move {
            card: 23,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert_eq!(ctx.card(BACK_OFF).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{card 81} is stunned".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert!(ctx.blob.seat(0).played_main);
        assert!(ctx.events.contains(&Event::PlayedSpell {
            item: 1,
            controller: 0,
            nth: 1
        }));
    }

    #[test]
    fn played_from_facedown_it_still_stuns_but_the_draw_is_skipped() {
        let mut fixture = armed();
        fixture.table.card_mut(BACK_OFF).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(BACK_OFF).hidden_at = Some(fixtures::BF1);
        let mut fresh = armed();
        let mut hand_ctx = fresh.ctx();
        play_from_hand(&mut hand_ctx, 0, BACK_OFF).unwrap();
        pick(&mut hand_ctx, 0, 2).unwrap();
        let paid_from_hand = hand_ctx.effects.len();
        drop(hand_ctx);

        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BACK_OFF, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            BACK_OFF,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx).len(),
            2,
            "737.1.d · only a unit at the hiding battlefield, plus cancel"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        let paid: Vec<&Effect> = ctx
            .effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .collect();
        assert!(
            paid.is_empty(),
            "a hidden card reacts for free, unlike the {paid_from_hand} effects it costs from hand"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert_eq!(ctx.combat_might(fixtures::THEIR_UNIT), 0);
        assert_eq!(
            drew(&ctx, 0),
            0,
            "the draw is conditional on the hand origin"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.card(BACK_OFF).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn a_target_that_left_the_board_is_unaffected_but_the_draw_still_happens() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, BACK_OFF).unwrap();
        pick(&mut ctx, 0, 2).unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::TRASH, 1),
                1,
            )
            .unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_stunned(fixtures::THEIR_UNIT));
        assert_eq!(stun_lines(&ctx), 0);
        assert_eq!(
            drew(&ctx, 0),
            1,
            "356.3.e.5: the draw is not a target and still happens"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(BACK_OFF).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn an_already_stunned_unit_is_a_legal_target_that_is_not_stunned_twice() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(ctx.stun(fixtures::THEIR_UNIT));
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, BACK_OFF).unwrap();
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "410.1.a.1: a stunned unit is still offered"
        );
        pick(&mut ctx, 0, 2).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert_eq!(
            stun_lines(&ctx),
            0,
            "410.1.a.1: stunning an already stunned unit does nothing"
        );
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn the_focus_holder_plays_back_off_inside_a_showdown_and_focus_moves_on() {
        let mut fixture = armed();
        let jinx = fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap();
        jinx.zone = Some(fixtures::BF1);
        jinx.seat = 0;
        fixture.resolve();
        fixture.blob.showdown = Some(Showdown::open(fixtures::BF1, 0, 1));
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_BACK_OFF)),
            Err(Refusal::NotYourFocus),
            "the attacker holds focus first"
        );
        play_from_hand(&mut ctx, 0, BACK_OFF).unwrap();
        pick(&mut ctx, 0, 2).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        resolve_chain(&mut ctx);
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert_eq!(ctx.combat_might(fixtures::THEIR_UNIT), 0);
        assert_eq!(drew(&ctx, 0), 1);
        let showdown = ctx
            .blob
            .showdown
            .clone()
            .expect("the showdown is still open");
        assert_eq!(
            showdown.focus(),
            1,
            "focus hands to the next seat once the play's chain empties (343)"
        );
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_wrong_seat_a_non_unit_target_an_empty_pick_and_a_short_purse_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_BACK_OFF)),
            Err(Refusal::NotYourTurn),
            "an action has no window on the other seat's turn outside a showdown"
        );
        play_from_hand(&mut ctx, 0, BACK_OFF).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a battlefield is not a unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in hand is not on the board"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "one unit is required"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        let cancel = labels(&ctx).len() as u16 - 1;
        pick(&mut ctx, 0, cancel).unwrap();
        assert_eq!(ctx.card(BACK_OFF).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(drew(&ctx, 0), 0);
        drop(ctx);

        let mut locked = armed();
        locked.blob.seat_mut(0).play_lock = PlayLock::SPELLS;
        let ctx = locked.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, BACK_OFF)),
            Err(Refusal::Illegal(Reason::NoSpells))
        );
        drop(ctx);

        let mut broke = armed();
        broke.table.card_mut(43).unwrap().exhausted = true;
        let ctx = broke.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, BACK_OFF)),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 2
            })
        );
    }

    #[test]
    fn hiding_back_off_through_the_action_path_marks_it_facedown_at_a_battlefield() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        {
            let card = fixture.table.card_mut(BACK_OFF).unwrap();
            card.name = String::new();
            card.kind = None;
            card.energy = None;
            card.domain.clear();
        }
        fixture.resolve();
        let action = fixtures::move_action(BACK_OFF, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        assert!(matches!(intent, legal::Intent::Hide { .. }));
        assert_eq!(crate::engine::act(&mut ctx, 0, intent), Ok(()));
        assert_eq!(
            ctx.state_of(BACK_OFF).map(|row| row.hidden_at),
            Some(Some(fixtures::BF1))
        );
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.blob.core_mut().unwrap().turn += 1;
        {
            let card = fixture.table.card_mut(BACK_OFF).unwrap();
            card.name = "Back Off".into();
            card.kind = Some("Spell".into());
            card.energy = Some(ENERGY);
            card.domain = vec!["Calm".into()];
        }
        fixture.resolve();
        let ctx = fixture.ctx();
        let from_facedown = EntryMove {
            card: BACK_OFF,
            from: Some(fixtures::BF1),
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        let intent = legal::classify(&ctx, 0, &from_facedown).unwrap();
        assert!(matches!(intent, legal::Intent::PlayFromFacedown { .. }));
    }

    fn hiding() -> Fixture {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture
    }

    fn face(fixture: &mut Fixture, card: u32, shown: bool) {
        {
            let held = fixture.table.card_mut(card).unwrap();
            if shown {
                held.name = "Back Off".into();
                held.kind = Some("Spell".into());
                held.energy = Some(ENERGY);
                held.domain = vec!["Calm".into()];
            } else {
                held.name = String::new();
                held.kind = None;
                held.energy = None;
                held.domain.clear();
            }
        }
        fixture.resolve();
    }

    fn hide_now(fixture: &mut Fixture, seat: u8, card: u32, zone: u16) -> Result<(), Refusal> {
        let action = fixtures::move_action(card, zone, 0);
        let mut ctx = fixture.ctx_for(seat, &action);
        let entry = ctx.entry.unwrap();
        let intent = legal::classify(&ctx, seat, &entry)?;
        assert_eq!(intent, legal::Intent::Hide { card, zone });
        let done = crate::engine::act(&mut ctx, seat, intent);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        done
    }

    fn from_facedown(ctx: &Ctx, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: Some(fixtures::BF1),
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn their_spell_on_the_chain(fixture: &mut Fixture) {
        fixture.blob.chain.push(ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            Origin::Hand,
        ));
        fixture.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
    }

    fn unpaid(ctx: &Ctx) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .count()
    }

    #[test]
    fn hidden_back_off_reacts_from_facedown_on_the_other_seats_turn_for_nothing() {
        let mut fixture = hiding();
        face(&mut fixture, BACK_OFF, false);
        let runes = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.zone == Some(fixtures::RUNE_POOL) && card.owner == 0)
            .count();
        hide_now(&mut fixture, 0, BACK_OFF, fixtures::BF1).unwrap();
        assert_eq!(
            fixture
                .blob
                .card_state(BACK_OFF)
                .and_then(|row| row.hidden_at),
            Some(fixtures::BF1),
            "737.1.b · [A] buys a facedown slot at a battlefield seat 0 holds"
        );
        assert_eq!(
            fixture
                .table
                .cards
                .iter()
                .filter(|card| card.zone == Some(fixtures::RUNE_POOL) && card.owner == 0)
                .count(),
            runes - 1,
            "the rainbow power recycles exactly one rune"
        );
        {
            let core = fixture.blob.core_mut().unwrap();
            core.turn += 1;
            core.player = 1;
        }
        face(&mut fixture, BACK_OFF, true);
        their_spell_on_the_chain(&mut fixture);
        let action = fixtures::move_action(BACK_OFF, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(
            !CARD.has_keyword(Keyword::Reaction) && CARD.has_keyword(Keyword::Action),
            "737.6 grants the Reaction; the printed card is an Action"
        );
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        assert_eq!(
            intent,
            legal::Intent::PlayFromFacedown { card: BACK_OFF },
            "737.6 · an Action answers the other seat's chain while it is facedown"
        );
        crate::engine::act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            labels(&ctx),
            ["{card 81}", "cancel"],
            "737.1.d.2 · only a unit at the hiding battlefield is offered"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            unpaid(&ctx),
            0,
            "737.1.b · played from facedown ignoring its base cost"
        );
        assert_eq!(ctx.blob.chain.len(), 2, "it sits on top of their spell");
        assert_eq!(
            ctx.blob.chain[1].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        assert_eq!(
            ctx.blob.chain[1].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            ctx.blob.card_state(BACK_OFF).and_then(|row| row.hidden_at),
            None,
            "the play finalized onto the chain, so the facedown slot is free again"
        );
        assert!(
            ctx.has_flag(BACK_OFF, FLAG_FROM_FACEDOWN),
            "the provenance the draw clause reads"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert_eq!(ctx.combat_might(fixtures::THEIR_UNIT), 0);
        assert_eq!(
            drew(&ctx, 0),
            0,
            "the draw asks for the hand, and this came from the facedown zone"
        );
        assert_eq!(ctx.card(BACK_OFF).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.chain.len(), 1, "their spell is still waiting");
    }

    #[test]
    fn hiding_needs_a_held_battlefield_an_empty_slot_and_a_turn_to_pass() {
        let mut unheld = armed();
        face(&mut unheld, BACK_OFF, false);
        assert_eq!(
            hide_now(&mut unheld, 0, BACK_OFF, fixtures::BF1),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "737.1.b · a battlefield you control"
        );
        assert_eq!(
            unheld
                .blob
                .card_state(BACK_OFF)
                .and_then(|row| row.hidden_at),
            None
        );
        drop(unheld);

        let mut fixture = hiding();
        face(&mut fixture, BACK_OFF, false);
        hide_now(&mut fixture, 0, BACK_OFF, fixtures::BF1).unwrap();
        face(&mut fixture, BACK_OFF, true);
        {
            let ctx = fixture.ctx();
            assert_eq!(
                legal::classify(&ctx, 0, &from_facedown(&ctx, BACK_OFF)),
                Err(Refusal::Illegal(Reason::HiddenThisTurn)),
                "737.1.b · the grant starts on the next turn"
            );
            assert_eq!(
                legal::classify(&ctx, 1, &from_facedown(&ctx, BACK_OFF)),
                Err(Refusal::Illegal(Reason::NotYourCard)),
                "the other seat never plays it"
            );
        }
        assert_eq!(
            hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1),
            Err(Refusal::Illegal(Reason::OneFacedown)),
            "106.4.b · one card per facedown zone"
        );
        fixture.blob.core_mut().unwrap().turn += 1;
        let ctx = fixture.ctx();
        assert!(
            matches!(
                legal::classify(&ctx, 0, &from_facedown(&ctx, BACK_OFF)),
                Ok(legal::Intent::PlayFromFacedown { card: BACK_OFF })
            ),
            "and opens the next turn"
        );
    }

    #[test]
    fn a_hidden_back_off_with_no_unit_at_its_battlefield_cannot_be_played_from_hidden() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        face(&mut fixture, BACK_OFF, false);
        hide_now(&mut fixture, 0, BACK_OFF, fixtures::BF1).expect("the [A] hide is legal");
        fixture.blob.core_mut().unwrap().turn += 1;
        face(&mut fixture, BACK_OFF, true);
        {
            let ctx = fixture.ctx();
            assert!(
                targets::candidates(
                    &ctx,
                    &ChainItem::new(
                        99,
                        ItemKind::Spell { card: BACK_OFF },
                        0,
                        Origin::Facedown {
                            zone: fixtures::BF1
                        },
                    ),
                    &CARD.abilities[0].targets[0],
                )
                .is_empty(),
                "nothing stands at the battlefield it was hidden at"
            );
            assert_eq!(
                hide::play_legal(&ctx, 0, BACK_OFF),
                Err(Refusal::Illegal(Reason::NoLegalTargets)),
                "737.1.d · a hidden spell with no valid target under the restriction is not played"
            );
            assert_eq!(
                legal::classify(&ctx, 0, &from_facedown(&ctx, BACK_OFF)),
                Err(Refusal::Illegal(Reason::NoLegalTargets)),
                "the refusal reaches the drag, so the UI never offers a dead play"
            );
            assert!(
                !legal::highlights(&ctx, 0)
                    .iter()
                    .any(|row| row.card == BACK_OFF),
                "and the M5 rims stop offering it"
            );
        }
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            hide::play_legal(&ctx, 0, BACK_OFF),
            Ok(fixtures::BF1),
            "a unit walking in re-opens the play"
        );
        assert!(legal::highlights(&ctx, 0)
            .iter()
            .any(|row| row.card == BACK_OFF));
    }
}
