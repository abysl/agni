use super::prelude::{legion, unit};
use super::{Card, Cost, Domain, Keyword, Power};
use crate::engine::cost;
use crate::engine::ctx::Ctx;

pub const FROM_TRASH: Cost = Cost {
    energy: 3,
    power: &[Power::Domain(Domain::Fury)],
};

pub fn from_trash_cost(ctx: &Ctx, me: u32) -> cost::Cost {
    cost::of_script(&FROM_TRASH, &ctx.domains_of(me))
}

pub fn playable_from_trash(ctx: &Ctx, seat: u8, me: u32) -> bool {
    ctx.in_trash(me) && ctx.controller(me) == seat && legion(ctx, seat)
}

pub static CARD: Card = unit("Undying Legion", &[Keyword::Legion], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, IMPLICIT_FLOW};
    use crate::engine::cost::Need;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, settle};
    use crate::state::{Leave, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const LEGION: u32 = 90;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 3;
    const FURY_RUNE: u32 = 46;

    fn undying(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Fury".into()],
            ..fixtures::unit(LEGION, zone, seat, "Undying Legion", MIGHT)
        }
    }

    fn barrow(zone: u16, played: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(undying(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(FURY_RUNE, 0, "Fury", false));
        fixture.blob.seat_mut(0).played_main = played;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LEGION).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_prints_legion_and_names_the_trash_play_seam() {
        assert!(std::ptr::eq(script_of("Undying Legion").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Legion]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert!(
            CARD.flow_cost().is_none(),
            "the trash play is printed under Legion, not as Flow"
        );
        assert_eq!(FROM_TRASH.energy, 3);
        assert_eq!(FROM_TRASH.power, [Power::Domain(Domain::Fury)]);
        let mut fixture = barrow(fixtures::TRASH, true);
        let ctx = fixture.ctx();
        let price = from_trash_cost(&ctx, LEGION);
        assert_eq!(price.energy, 3);
        assert_eq!(price.power, [Need::Domain(Domain::Fury)]);
    }

    #[test]
    fn the_seam_reads_the_trash_the_controller_and_legion() {
        let mut fixture = barrow(fixtures::TRASH, true);
        let ctx = fixture.ctx();
        assert!(legion(&ctx, 0));
        assert!(playable_from_trash(&ctx, 0, LEGION));
        assert!(!playable_from_trash(&ctx, 1, LEGION), "not their card");
        drop(ctx);
        let mut quiet = barrow(fixtures::TRASH, false);
        let ctx = quiet.ctx();
        assert!(
            !playable_from_trash(&ctx, 0, LEGION),
            "812 · Legion is off until another card was played this turn"
        );
        drop(ctx);
        let mut held = barrow(fixtures::HAND, true);
        let ctx = held.ctx();
        assert!(
            !playable_from_trash(&ctx, 0, LEGION),
            "in hand, not in the trash"
        );
        drop(ctx);
        let mut fielded = barrow(fixtures::BASE, true);
        let ctx = fielded.ctx();
        assert!(!playable_from_trash(&ctx, 0, LEGION));
    }

    #[test]
    fn from_hand_it_plays_at_its_printed_three_and_the_trash_offers_nothing_today() {
        let mut fixture = barrow(fixtures::HAND, true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LEGION).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(LEGION), Some(Location::Base(0)));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "three energy off four runes"
        );
        assert_eq!(
            ctx.card(FURY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL),
            "no Fury power from the hand"
        );
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut fixture = barrow(fixtures::TRASH, true);
        let mut ctx = fixture.ctx();
        assert!(
            !activate::flow_offers(&ctx, 0)
                .iter()
                .any(|offer| offer.source == LEGION),
            "the Flow path lists spells only"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, LEGION, IMPLICIT_FLOW),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert_eq!(ctx.card(LEGION).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    #[ignore = "engine gap · a unit's own play from the trash for a printed cost under Legion: activate::flow_playable reads Keyword::Flow on spells only and cost::origin_cost prices a Banish-origin unit at its printed cost, so the offer never appears and the Fury is never charged"]
    fn under_legion_the_trash_offers_him_for_three_and_a_fury_and_he_enters_the_base() {
        let mut fixture = barrow(fixtures::TRASH, true);
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == LEGION)
            .expect("the trash offers the Legion play");
        assert!(offer.enabled);
        assert!(offer.label.contains("play from your trash"));
        activate::activate(&mut ctx, 0, LEGION, offer.index).unwrap();
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(LEGION), Some(Location::Base(0)));
        assert_eq!(
            ctx.card(FURY_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Fury rune recycles for the power"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "three energy off four runes"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            crate::engine::ctx::Event::Played { card, origin: Origin::Trash { leave: Leave::Banish }, .. } if *card == LEGION
        )));
    }
}
