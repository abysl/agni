use super::prelude::{done, draw, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

const DRAWS: usize = 2;

fn consult(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = spell(
    "Consult the Past",
    &[Keyword::Hidden, Keyword::Reaction],
    &[play(&[], consult)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ChainItem, ItemKind, Origin, PlayLock, Priority, FLAG_FROM_FACEDOWN};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};

    const CONSULT: u32 = 77;
    const THEIR_CONSULT: u32 = 83;
    const MY_EXTRA_RUNES: [u32; 3] = [46, 47, 48];
    const THEIR_EXTRA_RUNES: [u32; 2] = [90, 91];

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut mine = fixtures::spell(CONSULT, fixtures::HAND, 0, "Consult the Past", 4, 0);
        mine.domain = vec!["Mind".into()];
        let mut theirs =
            fixtures::spell(THEIR_CONSULT, fixtures::HAND, 1, "Consult the Past", 4, 0);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(mine);
        fixture.table.cards.push(theirs);
        for rune in MY_EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        for rune in THEIR_EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Mind", false));
        }
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

    fn exhausted_runes(ctx: &Ctx, seat: u8) -> usize {
        let runes: Vec<u32> = ctx.runes_of(seat).iter().map(|rune| rune.id).collect();
        ctx.effects
            .iter()
            .filter(|effect| runes.iter().any(|rune| **effect == Effect::exhaust(*rune)))
            .count()
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_registered_with_its_keywords_and_a_single_untargeted_play_ability() {
        let script = crate::cards::script_of("Consult the Past").unwrap();
        assert!(std::ptr::eq(script, &CARD));
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(!CARD.abilities[0].optional);
    }

    #[test]
    fn played_as_a_reaction_it_sits_on_the_chain_then_draws_two_for_its_controller() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand_before = ctx.hand_of(0).len();
        let deck_before: Vec<u32> = ctx
            .table
            .held(fixtures::MAIN_DECK, 0)
            .map(|card| card.id)
            .collect();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(priority::holder(&ctx), Some(0));
        play_from_hand(&mut ctx, 0, CONSULT).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].kind.source(), CONSULT);
        assert_eq!(ctx.blob.chain[1].controller, 0);
        assert_eq!(drew(&ctx, 0), 0, "nothing is drawn before it resolves");
        assert_eq!(
            exhausted_runes(&ctx, 0),
            6,
            "four energy for Consult on top of two for Spark"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::HAND_SPELL);
        assert_eq!(drew(&ctx, 0), 2);
        assert_eq!(drew(&ctx, 1), 0);
        let hand = ctx.hand_of(0);
        assert_eq!(hand.len(), hand_before - 2 + 2);
        let top_two: Vec<u32> = deck_before.iter().rev().take(2).copied().collect();
        for card in &top_two {
            assert!(hand.contains(card), "{card} was drawn from the top");
            assert!(ctx.effects.contains(&Effect::Move {
                card: *card,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }));
        }
        assert_eq!(
            ctx.card(CONSULT).unwrap().zone,
            Some(fixtures::TRASH),
            "a resolved spell is trashed"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CONSULT}}} resolves")));
        assert_eq!(priority::holder(&ctx), Some(0));
    }

    #[test]
    fn it_opens_a_chain_on_its_own_and_the_draws_arrive_when_both_seats_pass() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(ctx.blob.is_neutral_open());
        play_from_hand(&mut ctx, 0, CONSULT).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(drew(&ctx, 0), 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(drew(&ctx, 0), 2);
        assert_eq!(ctx.blob.seat(0).draws, 2);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 2 }));
    }

    #[test]
    fn the_other_seat_reacts_with_its_own_copy_once_priority_reaches_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, THEIR_CONSULT).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].controller, 1);
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(drew(&ctx, 1), 2);
        assert_eq!(drew(&ctx, 0), 0);
        assert_eq!(ctx.hand_of(1).len(), 1 + 1 - 1 + 2);
        assert_eq!(ctx.card(THEIR_CONSULT).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_seat_without_priority_a_locked_seat_and_a_short_purse_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CONSULT)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "a reaction still waits for priority"
        );
        assert_eq!(drew(&ctx, 1), 0);
        drop(ctx);

        let mut locked = armed();
        locked.blob.seat_mut(0).play_lock = PlayLock::SPELLS;
        let ctx = locked.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, CONSULT)),
            Err(Refusal::Illegal(Reason::NoSpells))
        );
        drop(ctx);

        let mut broke = armed();
        for rune in MY_EXTRA_RUNES {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = broke.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, CONSULT)),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 3
            })
        );
    }

    #[test]
    fn played_from_facedown_it_costs_nothing_and_still_draws_two() {
        let mut fixture = armed();
        fixture.table.card_mut(CONSULT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(CONSULT).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let paid = ctx
            .effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .count();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(CONSULT, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            CONSULT,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        let spent = ctx
            .effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .count();
        assert_eq!(spent, paid, "a hidden card reacts for free");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(drew(&ctx, 0), 2);
        assert_eq!(ctx.card(CONSULT).unwrap().zone, Some(fixtures::TRASH));
    }
    #[test]
    fn hiding_it_through_the_action_path_marks_it_facedown_at_the_battlefield() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let hide = EntryMove {
            card: fixtures::HAND_HIDDEN,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(fixtures::BF1),
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        let intent = legal::classify(&ctx, 0, &hide).unwrap();
        assert!(matches!(intent, legal::Intent::Hide { .. }));
        assert_eq!(crate::engine::act(&mut ctx, 0, intent), Ok(()));
        assert_eq!(
            ctx.state_of(fixtures::HAND_HIDDEN).map(|row| row.hidden_at),
            Some(Some(fixtures::BF1))
        );
    }

    fn hiding() -> Fixture {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn face(fixture: &mut Fixture, card: u32, shown: bool) {
        {
            let held = fixture.table.card_mut(card).unwrap();
            if shown {
                held.name = "Consult the Past".into();
                held.kind = Some("Spell".into());
                held.energy = Some(4);
                held.domain = vec!["Mind".into()];
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

    fn unpaid(ctx: &Ctx) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .count()
    }

    #[test]
    fn hidden_consult_reacts_from_facedown_into_the_other_seats_chain_and_draws_two() {
        let mut fixture = hiding();
        face(&mut fixture, CONSULT, false);
        let runes = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.zone == Some(fixtures::RUNE_POOL) && card.owner == 0)
            .count();
        hide_now(&mut fixture, 0, CONSULT, fixtures::BF1).unwrap();
        assert_eq!(
            fixture
                .blob
                .card_state(CONSULT)
                .and_then(|row| row.hidden_at),
            Some(fixtures::BF1)
        );
        assert_eq!(
            fixture.table.card(CONSULT).and_then(|card| card.zone),
            Some(fixtures::BF1),
            "737.1.b · it leaves hand for the battlefield's facedown zone"
        );
        assert_eq!(
            fixture
                .table
                .cards
                .iter()
                .filter(|card| { card.zone == Some(fixtures::RUNE_POOL) && card.owner == 0 })
                .count(),
            runes - 1,
            "[A] recycles one rune instead of the printed four energy"
        );
        {
            let core = fixture.blob.core_mut().unwrap();
            core.turn += 1;
            core.player = 1;
        }
        face(&mut fixture, CONSULT, true);
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
        let action = fixtures::move_action(CONSULT, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        assert_eq!(intent, legal::Intent::PlayFromFacedown { card: CONSULT });
        crate::engine::act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "737.1.d has nothing to restrict: this spell chooses no targets"
        );
        assert_eq!(unpaid(&ctx), 0, "737.1.b · ignoring its base cost");
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(
            ctx.blob.chain[1].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        assert!(ctx.has_flag(CONSULT, FLAG_FROM_FACEDOWN));
        assert_eq!(
            ctx.blob.card_state(CONSULT).and_then(|row| row.hidden_at),
            None
        );
        assert_eq!(drew(&ctx, 0), 0, "nothing is drawn before it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(drew(&ctx, 0), 2, "737.2 · the instruction is unchanged");
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.card(CONSULT).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "343 · the reaction resolved above their spell"
        );
    }

    #[test]
    fn hiding_consult_needs_a_held_battlefield_and_the_reaction_waits_a_turn() {
        let mut unheld = armed();
        face(&mut unheld, CONSULT, false);
        assert_eq!(
            hide_now(&mut unheld, 0, CONSULT, fixtures::BF1),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "737.1.b · a battlefield you control"
        );
        assert_eq!(
            hide_now(&mut unheld, 0, CONSULT, fixtures::BASE),
            Err(Refusal::Illegal(Reason::Unrevealed)),
            "106.4.a · a base has no facedown zone, so the blank face goes nowhere"
        );
        drop(unheld);

        let mut fixture = hiding();
        face(&mut fixture, CONSULT, false);
        hide_now(&mut fixture, 0, CONSULT, fixtures::BF1).unwrap();
        face(&mut fixture, CONSULT, true);
        {
            let mut ctx = fixture.ctx();
            assert_eq!(
                legal::classify(&ctx, 0, &from_facedown(&ctx, CONSULT)),
                Err(Refusal::Illegal(Reason::HiddenThisTurn)),
                "737.1.b · the Reaction grant begins on the next turn"
            );
            assert_eq!(
                crate::engine::act(
                    &mut ctx,
                    0,
                    legal::Intent::PlayFromFacedown { card: CONSULT }
                ),
                Err(Refusal::Illegal(Reason::HiddenThisTurn)),
                "and the intent is refused even if it reaches act directly"
            );
            assert_eq!(drew(&ctx, 0), 0);
        }
        fixture.blob.core_mut().unwrap().turn += 1;
        let ctx = fixture.ctx();
        assert!(matches!(
            legal::classify(&ctx, 0, &from_facedown(&ctx, CONSULT)),
            Ok(legal::Intent::PlayFromFacedown { card: CONSULT })
        ));
    }
}
