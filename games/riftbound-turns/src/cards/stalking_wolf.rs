use super::daisy::{is_bird, is_cat, is_dog};
use super::poro_herder::is_poro;
use super::prelude::{friendly_units, unit, Location};
use super::{Card, Keyword};
use crate::engine::ctx::{Cause, Ctx, Killed};
use crate::engine::legal;

pub const KILLS: usize = 1;

pub fn is_pack_animal(ctx: &Ctx, card: u32) -> bool {
    is_poro(ctx, card) || is_bird(ctx, card) || is_cat(ctx, card) || is_dog(ctx, card)
}

pub fn kill_candidates(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut pack: Vec<u32> = friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| is_pack_animal(ctx, *unit))
        .collect();
    pack.sort_unstable();
    pack
}

pub fn kill_cost_payable(ctx: &Ctx, seat: u8) -> bool {
    kill_candidates(ctx, seat).len() >= KILLS
}

pub fn pay_kill_cost(ctx: &mut Ctx, unit: u32) -> Option<Location> {
    let stood_at = ctx.location(unit)?;
    if ctx.kill(unit, Cause::Cost) == Killed::NotOnBoard {
        return None;
    }
    ctx.narrate(format!("{{card {unit}}} is killed as an additional cost"));
    Some(stood_at)
}

pub fn play_locations_with_the_kill(
    ctx: &Ctx,
    seat: u8,
    wolf: u32,
    killed_at: Option<Location>,
) -> Vec<Location> {
    let mut locations = legal::ambush_locations(ctx, seat, wolf);
    if let Some(Location::Battlefield(zone)) = killed_at {
        let there = Location::Battlefield(zone);
        if ctx.units_played_here(zone) && !locations.contains(&there) {
            locations.push(there);
        }
    }
    locations
}

pub static CARD: Card = unit("Stalking Wolf", &[Keyword::Ambush], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::daisy::DOGS;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use agni_plugin_sdk::table::CardInfo;

    const WOLF: u32 = 90;
    const MY_PORO: u32 = 91;
    const MY_HOUND: u32 = 92;
    const MY_CAT: u32 = 93;
    const THEIR_PORO: u32 = 94;
    const MY_BIRD_ALT: u32 = 95;
    const PLAIN: u32 = 54;

    fn wolf(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(WOLF, zone, seat, "Stalking Wolf", 6)
        }
    }

    fn pack() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.table.cards.push(wolf(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(MY_PORO, fixtures::BF1, 0, "Pouty Poro", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(MY_HOUND, fixtures::BASE, 0, "Loyal Pup", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(MY_CAT, fixtures::BF2, 0, "Pakaa Cub", 2));
        fixture.table.cards.push(fixtures::unit(
            THEIR_PORO,
            fixtures::BASE,
            1,
            "Daring Poro",
            2,
        ));
        fixture.table.cards.push(fixtures::unit(
            MY_BIRD_ALT,
            fixtures::BASE,
            0,
            "Soaring Scout (Alternate Art)",
            2,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_prints_ambush_alone_and_the_kill_cost_is_a_named_seam() {
        assert!(std::ptr::eq(script_of("Stalking Wolf").unwrap(), &CARD));
        assert_eq!(CARD.name, "Stalking Wolf");
        assert_eq!(CARD.keywords, [Keyword::Ambush]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is an optional rune cost; his kills a friendly Bird, Cat, Dog or Poro"
        );
        assert_eq!(KILLS, 1);
        assert!(DOGS.contains(&"Stalking Wolf"), "he is a Dog himself");
        let mut fixture = pack();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(WOLF, Keyword::Ambush));
    }

    #[test]
    fn the_pack_is_read_by_base_name_since_the_table_carries_no_tags() {
        let mut fixture = pack();
        let ctx = fixture.ctx();
        assert!(is_pack_animal(&ctx, MY_PORO), "a Poro");
        assert!(is_pack_animal(&ctx, MY_HOUND), "a Dog");
        assert!(is_pack_animal(&ctx, MY_CAT), "a Cat");
        assert!(
            is_pack_animal(&ctx, MY_BIRD_ALT),
            "a Bird · a print name resolves to its base"
        );
        assert!(is_pack_animal(&ctx, THEIR_PORO), "the tag ignores control");
        assert!(!is_pack_animal(&ctx, fixtures::VI));
        assert!(!is_pack_animal(&ctx, fixtures::SPRITE));
        assert!(
            is_pack_animal(&ctx, WOLF),
            "a Dog himself · the tag reads the face, the candidates read the board"
        );
        assert!(!is_pack_animal(&ctx, 999));
    }

    #[test]
    fn the_cost_candidates_are_the_controllers_pack_on_the_board_and_paying_kills_one() {
        let mut fixture = pack();
        let mut ctx = fixture.ctx();
        assert_eq!(
            kill_candidates(&ctx, 0),
            [MY_PORO, MY_HOUND, MY_CAT, MY_BIRD_ALT]
        );
        assert_eq!(
            kill_candidates(&ctx, 1),
            [THEIR_PORO],
            "each seat reads its own"
        );
        assert!(kill_cost_payable(&ctx, 0));
        assert_eq!(
            pay_kill_cost(&mut ctx, MY_PORO),
            Some(Location::Battlefield(fixtures::BF1)),
            "the kill remembers where it stood"
        );
        assert!(ctx.in_trash(MY_PORO));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: true, .. } if *card == MY_PORO
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MY_PORO}}} is killed as an additional cost"
        )));
        assert_eq!(
            pay_kill_cost(&mut ctx, MY_PORO),
            None,
            "a dead Poro cannot pay twice"
        );
        assert_eq!(kill_candidates(&ctx, 0), [MY_HOUND, MY_CAT, MY_BIRD_ALT]);
        drop(ctx);

        let mut lone = Fixture::enforced();
        lone.table.cards.push(wolf(fixtures::HAND, 0));
        lone.resolve();
        let ctx = lone.ctx();
        assert!(kill_candidates(&ctx, 0).is_empty());
        assert!(
            !kill_cost_payable(&ctx, 0),
            "356.2 · without a Bird, Cat, Dog or Poro he cannot be played"
        );
    }

    #[test]
    fn the_play_locations_are_the_ambush_ones_plus_the_killed_units_battlefield() {
        let mut fixture = pack();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::ambush_locations(&ctx, 0, WOLF),
            [
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ],
            "Ambush reaches the battlefields where he has units"
        );
        let killed_at = pay_kill_cost(&mut ctx, MY_CAT);
        assert_eq!(killed_at, Some(Location::Battlefield(fixtures::BF2)));
        assert_eq!(
            legal::ambush_locations(&ctx, 0, WOLF),
            [Location::Battlefield(fixtures::BF1)],
            "the cat was his only unit there"
        );
        assert_eq!(
            play_locations_with_the_kill(&ctx, 0, WOLF, killed_at),
            [
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ],
            "even if you don't have other units there"
        );
        let killed_at = pay_kill_cost(&mut ctx, MY_HOUND);
        assert_eq!(killed_at, Some(Location::Base(0)));
        assert_eq!(
            play_locations_with_the_kill(&ctx, 0, WOLF, killed_at),
            [Location::Battlefield(fixtures::BF1)],
            "a kill in the base adds no battlefield"
        );
        assert_eq!(
            play_locations_with_the_kill(&ctx, 0, WOLF, None),
            [Location::Battlefield(fixtures::BF1)]
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_kill_at_rockfall_path_adds_nothing_since_units_cannot_be_played_there() {
        let mut fixture = pack();
        fixture.table.card_mut(MY_CAT).unwrap().zone = Some(fixtures::BF3);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!ctx.units_played_here(fixtures::BF3), "Rockfall Path");
        let killed_at = pay_kill_cost(&mut ctx, MY_CAT);
        assert_eq!(killed_at, Some(Location::Battlefield(fixtures::BF3)));
        assert_eq!(
            play_locations_with_the_kill(&ctx, 0, WOLF, killed_at),
            [Location::Battlefield(fixtures::BF1)]
        );
    }

    #[test]
    #[ignore = "engine gap · a non-resource mandatory additional cost (kill a friendly Bird, Cat, Dog or Poro) at the pay stage (the Zaun Punk row): play::advance knows only the rune cost in Card.additional, so the play never asks which pack unit dies, never refuses him without one, and never offers its battlefield as his play location; kill_candidates, pay_kill_cost and play_locations_with_the_kill are the seam"]
    fn playing_him_asks_which_pack_unit_dies_and_offers_its_battlefield() {
        let mut fixture = pack();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WOLF).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(crate::state::PromptWhy::Target { .. })),
            "which pack unit pays: {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_CAT}}}")).unwrap();
        assert!(
            ctx.in_trash(MY_CAT),
            "357.2 · the kill is paid before he enters"
        );
        assert!(matches!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::PlayLocation { .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF2)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(WOLF),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }
}
