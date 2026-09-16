use super::prelude::{banish_by, burn, done, gear, play, with_statics};
use super::{Card, Flow, Item, Stage, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const BURN: usize = 7;

pub fn riches_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut hoards: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .map(|held| held.id)
        .filter(|riches| statics::in_play(ctx, *riches) && ctx.controller(*riches) == seat)
        .collect();
    hoards.sort_unstable();
    hoards
}

pub fn skips_draw_phase(ctx: &Ctx, seat: u8) -> bool {
    !riches_of(ctx, seat).is_empty()
}

pub fn skips_your_draw_phase(ctx: &Ctx, riches: u32) -> bool {
    statics::in_play(ctx, riches)
}

pub fn may_play_cards_from_trash(ctx: &Ctx, seat: u8) -> bool {
    !riches_of(ctx, seat).is_empty()
}

pub fn banishes_instead_of_trash(ctx: &Ctx, card: u32, from: Option<u16>) -> bool {
    let Some(held) = ctx.card(card) else {
        return false;
    };
    let from_main_deck = from.is_some() && from == ctx.zones.main_deck;
    !from_main_deck && !riches_of(ctx, held.owner).is_empty()
}

fn hoard(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let hand = ctx.hand_of(seat);
    let trash = ctx.trash_of(seat);
    let hand_banished = hand
        .into_iter()
        .filter(|card| banish_by(ctx, *card, seat))
        .count();
    let trash_banished = trash
        .into_iter()
        .filter(|card| banish_by(ctx, *card, seat))
        .count();
    ctx.narrate(format!(
        "{{seat {seat}}} banishes their hand ({hand_banished}) and trash ({trash_banished})"
    ));
    burn(ctx, seat, BURN);
    done()
}

pub static CARD: Card = with_statics(
    gear("Endless Riches", &[], &[play(&[], hoard)]),
    &[
        Static::SkipsDrawPhase(skips_your_draw_phase),
        Static::BanishesInsteadOfTrash(banishes_instead_of_trash),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, priority};
    use crate::state::{ItemKind, Phase};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const RICHES: u32 = 90;
    const SPENT: u32 = 91;
    const THEIR_SPENT: u32 = 92;
    const PLAY: u8 = 0;

    fn riches(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::gear(RICHES, zone, seat, "Endless Riches", 5)
        }
    }

    fn vault(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(riches(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(SPENT, fixtures::TRASH, 0, "Spent Wailer", 2));
        fixture.table.cards.push(fixtures::unit(
            THEIR_SPENT,
            fixtures::TRASH,
            1,
            "Their Wailer",
            2,
        ));
        for id in [46, 47, 48] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Fury", false));
        }
        for id in [40, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RICHES).unwrap(),
            &CARD
        ));
        fixture
    }

    fn burned(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Burned { seat: who, card } if *who == seat => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_keywordless_gear_with_one_play_trigger_and_two_statics() {
        assert!(std::ptr::eq(script_of("Endless Riches").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let hoard = &CARD.abilities[usize::from(PLAY)];
        assert_eq!(hoard.trigger, Trigger::Play);
        assert!(hoard.targets.is_empty());
        assert!(!hoard.optional);
        assert!(matches!(
            CARD.statics,
            [Static::SkipsDrawPhase(_), Static::BanishesInsteadOfTrash(_)]
        ));
        assert!(CARD.replacement.is_none());
        assert_eq!(BURN, 7);
    }

    #[test]
    fn playing_it_banishes_your_hand_and_trash_then_burns_seven_and_leaves_the_opponent_alone() {
        let mut fixture = vault(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let hand: Vec<u32> = ctx
            .hand_of(0)
            .into_iter()
            .filter(|card| *card != RICHES)
            .collect();
        assert_eq!(hand.len(), 4);
        assert_eq!(ctx.trash_of(0), [SPENT]);
        let deck = ctx.top_of(fixtures::MAIN_DECK, 0, 7);
        assert_eq!(deck.len(), 4, "the fixture deck holds four");
        fixtures::play_from_hand(&mut ctx, 0, RICHES).unwrap();
        assert_eq!(ctx.location(RICHES), Some(Location::Base(0)));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: PLAY } if source == RICHES
        ));
        assert_eq!(
            ctx.hand_of(0).len(),
            4,
            "nothing until the trigger resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.hand_of(0).is_empty());
        let mut banished = ctx.banished_of(0);
        banished.sort_unstable();
        let mut expected = hand.clone();
        expected.push(SPENT);
        expected.sort_unstable();
        assert_eq!(banished, expected);
        assert_eq!(
            burned(&ctx, 0).len(),
            4,
            "Burn 7 into a deck of four burns four"
        );
        let mut trash = ctx.trash_of(0);
        trash.sort_unstable();
        let mut deck = deck;
        deck.sort_unstable();
        assert_eq!(trash, deck, "the burned cards are the whole trash now");
        assert_eq!(ctx.top_of(fixtures::MAIN_DECK, 0, 1), Vec::<u32>::new());
        assert_eq!(ctx.hand_of(1), [fixtures::THEIR_HAND_CARD]);
        assert_eq!(ctx.trash_of(1), [THEIR_SPENT]);
        assert!(ctx.banished_of(1).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} banishes their hand (4) and trash (1)".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} burns 4".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_seams_read_the_controllers_riches_on_the_board_only() {
        let mut fixture = vault(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(riches_of(&ctx, 0), [RICHES]);
        assert!(riches_of(&ctx, 1).is_empty());
        assert!(skips_draw_phase(&ctx, 0));
        assert!(!skips_draw_phase(&ctx, 1));
        assert!(may_play_cards_from_trash(&ctx, 0));
        assert!(!may_play_cards_from_trash(&ctx, 1));
        assert!(banishes_instead_of_trash(
            &ctx,
            fixtures::HAND_SPELL,
            Some(fixtures::HAND)
        ));
        assert!(banishes_instead_of_trash(
            &ctx,
            fixtures::VI,
            Some(fixtures::BASE)
        ));
        assert!(
            !banishes_instead_of_trash(&ctx, 23, Some(fixtures::MAIN_DECK)),
            "a Burn from the Main Deck still lands in the trash"
        );
        assert!(
            !banishes_instead_of_trash(&ctx, fixtures::THEIR_HAND_CARD, Some(fixtures::HAND)),
            "the opponent's cards are not yours"
        );
        assert!(ctx.set_controller(RICHES, 1, fixtures::SPRITE));
        assert!(!skips_draw_phase(&ctx, 0));
        assert!(skips_draw_phase(&ctx, 1));
        assert!(
            !banishes_instead_of_trash(&ctx, fixtures::HAND_SPELL, Some(fixtures::HAND)),
            "your trash is the owner's trash and the riches are theirs now"
        );
        drop(ctx);
        let mut fixture = vault(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(riches_of(&ctx, 0).is_empty());
        assert!(!skips_draw_phase(&ctx, 0));
        assert!(!may_play_cards_from_trash(&ctx, 0));
        assert!(!banishes_instead_of_trash(
            &ctx,
            fixtures::HAND_SPELL,
            Some(fixtures::HAND)
        ));
    }

    #[test]
    fn the_other_seat_cannot_play_it_and_an_empty_hand_and_trash_banish_nothing() {
        let mut fixture = vault(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 1, RICHES),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        let mut fixture = vault(fixtures::HAND);
        fixture.table.cards.retain(|card| {
            card.id == RICHES || card.owner != 0 || card.zone != Some(fixtures::HAND)
        });
        fixture.table.cards.retain(|card| card.id != SPENT);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RICHES).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.banished_of(0).is_empty());
        assert_eq!(burned(&ctx, 0).len(), 4);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} banishes their hand (0) and trash (0)".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn today_the_trash_stays_final() {
        let mut fixture = vault(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(may_play_cards_from_trash(&ctx, 0));
        let entry = crate::engine::ctx::EntryMove {
            card: SPENT,
            from: Some(fixtures::TRASH),
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::TrashIsFinal)),
            "seam · legal::from_trash admits Flow spells only"
        );
    }

    #[test]
    fn with_the_riches_in_play_your_draw_phase_draws_nothing() {
        let mut fixture = vault(fixtures::BASE);
        fixture.blob.set_phase(Phase::Beginning);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        phases::continue_beginning(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    #[ignore = "engine gap · game-rule statics: legal::from_trash admits a Flow spell alone and legal::locations_for lists the chain for the trash only under Flow; the primitive is a consult of may_play_cards_from_trash(ctx, seat) that prices a trash card as a hand play with Origin::Trash { leave: Leave::Banish } (the banish-instead replacement sends a resolved spell to Banishment)"]
    fn with_the_riches_in_play_a_unit_in_your_trash_is_a_legal_play() {
        let mut fixture = vault(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: SPENT,
            from: Some(fixtures::TRASH),
            from_seat: 0,
            to: ctx.zones.base,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert!(legal::classify(&ctx, 0, &entry).is_ok());
        assert!(legal::locations_for(&ctx, 0, SPENT).contains(&Location::Base(0)));
        let entry = crate::engine::ctx::EntryMove {
            card: THEIR_SPENT,
            from: Some(fixtures::TRASH),
            from_seat: 1,
            to: ctx.zones.base,
            to_seat: 1,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::Illegal(Reason::TrashIsFinal))
        );
        let _ = &mut ctx;
    }

    #[test]
    fn with_the_riches_in_play_a_discard_and_a_death_are_banished_while_a_burn_is_trashed() {
        let mut fixture = vault(fixtures::BASE);
        let mut ctx = fixture.ctx();
        ctx.trash(fixtures::HAND_SPELL);
        assert!(ctx.in_banishment(fixtures::HAND_SPELL));
        assert_eq!(
            ctx.kill(fixtures::VI, crate::engine::ctx::Cause::Rule),
            crate::engine::ctx::Killed::Yes
        );
        crate::engine::cleanup::run(&mut ctx, None);
        assert!(ctx.in_banishment(fixtures::VI));
        assert_eq!(burn(&mut ctx, 0, 1), 1);
        assert_eq!(ctx.trash_of(0), [SPENT, 23]);
    }
}
