use super::ferrous_forerunner::spawn_mech;
use super::prelude::{done, draw, play, spell, with_statics, Location};
use super::rumble_mechanized_menace::friendly_mechs;
use super::{Card, Cost, Flow, Item, Stage, Static};
use crate::engine::ctx::Ctx;

pub const DISCOUNT: u8 = 2;
pub const CARDS: usize = 1;

pub fn controls_a_mech(ctx: &Ctx, seat: u8) -> bool {
    !friendly_mechs(ctx, seat).is_empty()
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    Cost {
        energy: if controls_a_mech(ctx, seat) {
            DISCOUNT
        } else {
            0
        },
        power: &[],
    }
}

fn surge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    spawn_mech(ctx, seat, Location::Base(seat));
    draw(ctx, seat, CARDS);
    done()
}

pub static CARD: Card = with_statics(
    spell("Production Surge", &[], &[play(&[], surge)]),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::ferrous_forerunner::{mech_face_until_token_mech_lands, MECH_MIGHT};
    use crate::cards::rumble_mechanized_menace::{is_mech, MECH_TOKEN};
    use crate::cards::{script_of, Keyword, Trigger, KIND_UNIT};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cost, legal, priority};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const SURGE: u32 = 90;
    const THEIR_SURGE: u32 = 91;
    const MIND_RUNE: u32 = 100;
    const FIRST_MECH: u32 = 200;

    fn surge(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Production Surge", 4, 1);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(surge(SURGE, 0));
        fixture.table.cards.push(surge(THEIR_SURGE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
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

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, SURGE).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.d · nothing is targeted");
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_is_a_targetless_sorcery_with_a_self_discount_and_a_mech_face() {
        assert!(std::ptr::eq(script_of("Production Surge").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        let face = mech_face_until_token_mech_lands();
        assert_eq!(face.name, MECH_TOKEN);
        assert_eq!(face.might, Some(MECH_MIGHT));
        assert_eq!(face.kind.as_deref(), Some(KIND_UNIT));
    }

    #[test]
    fn without_a_mech_it_costs_four_and_plays_an_exhausted_mech_to_the_base_and_draws_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(!controls_a_mech(&ctx, 0));
        assert_eq!(cost::total(&ctx, SURGE, false).energy, 4);
        let hand = ctx.hand_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        cast(&mut ctx);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 4);
        let mech = ctx.card(FIRST_MECH).expect("the Mech token");
        assert_eq!(mech.name, MECH_TOKEN);
        assert_eq!(mech.might, Some(3));
        assert_eq!(mech.owner, 0);
        assert!(mech.exhausted, "369.3 · a played unit enters exhausted");
        assert!(ctx.is_token(FIRST_MECH));
        assert_eq!(ctx.location(FIRST_MECH), Some(Location::Base(0)));
        assert!(is_mech(&ctx, FIRST_MECH));
        assert!(controls_a_mech(&ctx, 0));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == FIRST_MECH
        )));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1 + 1,
            "the Surge left, one card came"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} plays {card 200} to their base".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(SURGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn with_a_mech_on_the_board_it_costs_two_less() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().name = "Bubble Bot".into();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(is_mech(&ctx, fixtures::VI));
        assert!(controls_a_mech(&ctx, 0));
        assert!(
            !controls_a_mech(&ctx, 1),
            "the opponent's Mech is not yours"
        );
        assert_eq!(cost::total(&ctx, SURGE, false).energy, 2);
        assert_eq!(
            cost::total(&ctx, THEIR_SURGE, false).energy,
            4,
            "the opponent controls no Mech"
        );
        let ready = ctx.ready_runes_of(0).len();
        cast(&mut ctx);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 2);
        assert!(ctx.card(FIRST_MECH).is_some());
    }

    #[test]
    fn a_mech_in_the_hand_or_a_non_unit_does_not_count() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().name = "Mega-Mech".into();
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Forecaster".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!controls_a_mech(&ctx, 0));
        assert_eq!(cost::total(&ctx, SURGE, false).energy, 4);
    }

    #[test]
    fn the_surge_is_the_turn_players_and_needs_its_mind_power() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SURGE)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut poor = armed();
        poor.table.card_mut(MIND_RUNE).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, SURGE)),
            Err(Refusal::NoPowerOf)
        );
    }

    #[test]
    #[ignore = "engine gap · tags on the face: CardInfo carries no tags, so rumble_mechanized_menace::is_mech matches printed names, and a unit whose face carries the Mech tag under a name the list does not know never discounts the Surge"]
    fn a_unit_whose_face_carries_the_mech_tag_discounts_the_surge_whatever_its_name() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().name = "Tagged Elsewhere".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(controls_a_mech(&ctx, 0));
    }
}
