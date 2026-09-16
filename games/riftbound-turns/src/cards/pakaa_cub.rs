use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit("Pakaa Cub", &[Keyword::Hidden], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{Intent, Reason};
    use crate::engine::{act, hide, legal, play as play_engine, settle};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, Effect, BOTTOM, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const CUB: u32 = 90;
    const THEIR_CUB: u32 = 91;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 3;

    fn cub(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Body".into()],
            ..fixtures::unit(id, zone, seat, "Pakaa Cub", MIGHT)
        }
    }

    fn holding(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(zone);
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.blob.set_holder(zone, Some(0));
        fixture.table.cards.push(cub(CUB, fixtures::HAND, 0));
        fixture.table.cards.push(cub(THEIR_CUB, fixtures::HAND, 1));
        fixture.resolve();
        fixture
    }

    fn hide_now(fixture: &mut Fixture, seat: u8, card: u32, zone: u16) -> Result<(), Refusal> {
        let action = Action::Move {
            card,
            to: Some(zone),
            seat,
            index: TOP,
            hidden: true,
        };
        let mut ctx = fixture.ctx_for(seat, &action);
        let entry = ctx.entry.unwrap();
        let intent = legal::classify(&ctx, seat, &entry)?;
        assert_eq!(intent, Intent::Hide { card, zone });
        let done = act(&mut ctx, seat, intent);
        let recycled = ctx
            .effects
            .iter()
            .filter(|effect| {
                matches!(effect, Effect::Move { zone, index, .. }
                    if *zone == fixtures::RUNE_DECK && *index == BOTTOM)
            })
            .count();
        assert_eq!(recycled, 1, "737.1 · hiding costs one rune");
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        done
    }

    fn face_up(fixture: &mut Fixture, card: u32) {
        {
            let face = fixture.table.card_mut(card).unwrap();
            face.name = "Pakaa Cub".into();
            face.kind = Some("Unit".into());
            face.energy = Some(ENERGY);
            face.might = Some(MIGHT);
            face.domain = vec!["Body".into()];
        }
        fixture.resolve();
    }

    fn hidden_at(zone: u16) -> Fixture {
        let mut fixture = holding(zone);
        hide_now(&mut fixture, 0, CUB, zone).unwrap();
        face_up(&mut fixture, CUB);
        fixture
    }

    fn next_turn(fixture: &mut Fixture) {
        fixture.blob.core_mut().unwrap().turn += 1;
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: Some(fixtures::BASE),
            to_seat: seat,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_hidden_unit_with_no_abilities() {
        assert_eq!(CARD.name, "Pakaa Cub");
        assert_eq!(CARD.keywords, &[Keyword::Hidden]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = holding(fixtures::BF1);
        assert!(std::ptr::eq(fixture.scripts.of_card(CUB).unwrap(), &CARD));
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(CUB, Keyword::Hidden));
        assert!(!ctx.is_facedown(CUB));
    }

    #[test]
    fn hidden_for_a_rune_it_waits_a_turn_then_reacts_onto_its_battlefield_for_nothing() {
        let mut fixture = hidden_at(fixtures::BF1);
        {
            let ctx = fixture.ctx();
            assert!(ctx.is_facedown(CUB));
            assert_eq!(hide::zone_of(&ctx, CUB), Some(fixtures::BF1));
            assert!(!hide::playable(&ctx, CUB));
            assert!(!hide::reacts(&ctx, CUB));
            assert_eq!(
                hide::play_legal(&ctx, 0, CUB),
                Err(Refusal::Illegal(Reason::HiddenThisTurn)),
                "737.1.b · not on the turn it was hidden"
            );
        }
        next_turn(&mut fixture);
        let mut ctx = fixture.ctx();
        assert!(hide::playable(&ctx, CUB));
        assert!(
            hide::reacts(&ctx, CUB),
            "737.1 · a hidden card older than this turn has Reaction"
        );
        assert_eq!(hide::play_legal(&ctx, 0, CUB), Ok(fixtures::BF1));
        let runes = ctx.ready_runes_of(0).len();
        hide::play_from_facedown(&mut ctx, 0, CUB).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert_eq!(
            ctx.location(CUB),
            Some(Location::Battlefield(fixtures::BF1)),
            "737.1.d.1 · it lands at the battlefield it was hidden at"
        );
        assert!(!ctx.is_facedown(CUB));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            runes,
            "737.1.c · played for :rb_energy_0:"
        );
        assert!(
            ctx.card(CUB).unwrap().exhausted,
            "a played unit enters exhausted"
        );
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn it_cannot_come_out_where_units_may_not_be_played_nor_for_the_other_seat() {
        let mut blocked = hidden_at(fixtures::BF2);
        next_turn(&mut blocked);
        let mut ctx = blocked.ctx();
        assert!(
            !ctx.units_played_here(fixtures::BF2),
            "Rockfall Path sits at battlefield 2"
        );
        assert_eq!(
            hide::play_legal(&ctx, 0, CUB),
            Err(Refusal::Illegal(Reason::NoUnitsPlayedHere)),
            "737.1.d.1 · the forced placement is no free ride"
        );
        assert_eq!(
            hide::play_from_facedown(&mut ctx, 0, CUB),
            Err(Refusal::Illegal(Reason::NoUnitsPlayedHere))
        );
        assert!(ctx.is_facedown(CUB), "it stays hidden");
        assert!(ctx.effects.is_empty());
        drop(ctx);
        let mut fine = hidden_at(fixtures::BF1);
        next_turn(&mut fine);
        let ctx = fine.ctx();
        assert_eq!(
            hide::play_legal(&ctx, 1, CUB),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            hide::play_legal(&ctx, 0, THEIR_CUB),
            Err(Refusal::Illegal(Reason::NotFacedown))
        );
    }

    #[test]
    fn played_face_up_from_hand_it_pays_three_and_the_other_seat_cannot_play_it_on_this_turn() {
        let mut fixture = holding(fixtures::BF1);
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CUB)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let action = fixtures::move_action(CUB, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        legal::classify(&ctx, 0, &entry(&ctx, 0, CUB)).unwrap();
        play_engine::begin(&mut ctx, 0, CUB, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(CUB), Some(Location::Base(0)));
        assert!(ctx.card(CUB).unwrap().exhausted);
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "three energy exhausts every ready rune"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
    }
}
