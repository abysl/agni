use super::prelude::{done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage, KIND_GEAR};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::kill;

fn all_gear(ctx: &Ctx) -> Vec<u32> {
    ctx.faces_on_board()
        .filter(|card| card.is_kind(KIND_GEAR))
        .map(|card| card.id)
        .filter(|gear| !ctx.is_facedown(*gear) && !ctx.is_pending_play(*gear))
        .collect()
}

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let gear = all_gear(ctx);
    if gear.is_empty() {
        ctx.narrate("there is no gear to kill");
        return done();
    }
    for dead in kill::batch(ctx, &gear, Cause::Item(item.id)) {
        ctx.narrate(format!("{{card {dead}}} dies"));
    }
    done()
}

pub static CARD: Card = spell("Thermo Beam", &[Keyword::Action], &[play(&[], resolve)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, deathknell, gear};
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const BEAM: u32 = 90;
    const THEIR_BEAM: u32 = 91;
    const MY_GEAR: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const WORN: u32 = 94;
    const GOLD: u32 = 95;
    const FURY_D: u32 = 100;
    const FURY_E: u32 = 101;

    static MOURNED: Card = gear(
        "Mourned",
        &[Keyword::Deathknell],
        &[deathknell(&[], |ctx, item, _| {
            prelude::draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    fn beam(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Thermo Beam", 5, 2);
        card.domain = vec!["Fury".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(beam(BEAM, 0));
        fixture.table.cards.push(beam(THEIR_BEAM, 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Mourned", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Mourned", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(WORN, fixtures::BASE, 1, "Worn", 1));
        fixture.table.cards.push(fixtures::gold(GOLD, 1, false));
        fixture.table.tokens.push(GOLD);
        fixture
            .table
            .cards
            .push(fixtures::rune(FURY_D, 0, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(FURY_E, 0, "Fury", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(MY_GEAR, &MOURNED)
            .with_script(THEIR_GEAR, &MOURNED);
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

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, BEAM).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.d · nothing is targeted");
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_targetless_action() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Thermo Beam").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Thermo Beam");
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(all_gear(&ctx), [MY_GEAR, THEIR_GEAR, WORN, GOLD]);
    }

    #[test]
    fn every_gear_on_both_sides_dies_at_once_tokens_included_and_deathknells_queue_after() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.attach(WORN, fixtures::THEIR_UNIT),
            prelude::Attached::Yes
        );
        let my_hand = ctx.hand_of(0).len();
        let their_hand = ctx.hand_of(1).len();
        cast(&mut ctx);
        for gear in [MY_GEAR, THEIR_GEAR, WORN] {
            assert_eq!(
                ctx.card(gear).unwrap().zone,
                Some(fixtures::TRASH),
                "{gear} died"
            );
        }
        assert!(
            ctx.card(GOLD).is_none(),
            "a Gold token is gear and vanishes"
        );
        assert!(ctx.effects.contains(&Effect::Despawn { card: GOLD }));
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "the wearer is untouched, only its gear went"
        );
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Died { unit: false, .. }))
                .count(),
            4
        );
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "both Mourned Deathknells wait on the chain"
        );
        let sources: Vec<u32> = ctx
            .blob
            .chain
            .iter()
            .map(|item| match item.kind {
                ItemKind::Trigger { source, index: 0 } => source,
                other => panic!("{other:?}"),
            })
            .collect();
        assert!(sources.contains(&MY_GEAR) && sources.contains(&THEIR_GEAR));
        assert_eq!(ctx.card(BEAM).unwrap().zone, Some(fixtures::TRASH));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.hand_of(0).len(),
            my_hand - 1 + 1,
            "the spell left, Mourned drew"
        );
        assert_eq!(ctx.hand_of(1).len(), their_hand + 1);
        assert!(ctx.blob.log.contains(&format!("{{card {GOLD}}} dies")));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn with_no_gear_on_the_board_the_beam_resolves_and_nothing_happens() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![MY_GEAR, THEIR_GEAR, WORN, GOLD].contains(&card.id));
        fixture.table.tokens.retain(|token| *token != GOLD);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert!(ctx
            .blob
            .log
            .contains(&"there is no gear to kill".to_string()));
        assert_eq!(
            ctx.card(BEAM).unwrap().zone,
            Some(fixtures::TRASH),
            "055.1 · still played and resolved"
        );
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_facedown_card_at_a_battlefield_is_no_gear_and_units_are_spared() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(fixtures::HAND_GEAR).hidden_at = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!all_gear(&ctx).contains(&fixtures::HAND_GEAR));
        cast(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::BF1),
            "a hidden gear is not a gear until it is played"
        );
        for unit in [fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT] {
            assert!(ctx.on_board(unit));
        }
    }

    #[test]
    fn the_beam_is_the_turn_players_action_and_needs_its_five_energy() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_BEAM)),
            Err(Refusal::NotYourTurn),
            "an Action has no window on the other seat's turn outside a showdown"
        );
        drop(ctx);
        let mut poor = armed();
        poor.table
            .cards
            .retain(|card| ![FURY_D, FURY_E].contains(&card.id));
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, BEAM)),
            Err(Refusal::NotEnoughRunes {
                needed: 5,
                ready: 4
            }),
            "four ready runes cannot pay five energy"
        );
    }
}
