use super::daisy::{is_bird, is_cat, is_dog, tags_among_your_units};
use super::poro_herder::is_poro;
use super::prelude::{a_unit, card_target, done, friendly_units, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub type TagReader = fn(&Ctx, u32) -> bool;

pub const TAGS: [(&str, TagReader); 4] = [
    ("Bird", is_bird),
    ("Cat", is_cat),
    ("Dog", is_dog),
    ("Poro", is_poro),
];

pub fn tag_names_among_your_units(ctx: &Ctx, seat: u8) -> Vec<&'static str> {
    let units = friendly_units(ctx, seat);
    TAGS.into_iter()
        .filter(|(_, has_tag)| units.iter().any(|unit| has_tag(ctx, *unit)))
        .map(|(tag, _)| tag)
        .collect()
}

pub fn friendship_might(ctx: &Ctx, seat: u8) -> i16 {
    i16::try_from(tags_among_your_units(ctx, seat)).unwrap_or(i16::MAX)
}

fn befriend(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let tags = tag_names_among_your_units(ctx, item.controller);
    let delta = friendship_might(ctx, item.controller);
    if delta == 0 {
        ctx.narrate(format!(
            "{{card {unit}}} gets nothing · no Bird, Cat, Dog or Poro among {{seat {}}}'s units",
            item.controller
        ));
        return done();
    }
    might_this_turn(ctx, item, unit, delta, None);
    ctx.narrate(format!(
        "{{card {unit}}} gets +{delta} might this turn · {}",
        tags.join(", ")
    ));
    done()
}

pub static CARD: Card = spell(
    "Friendship",
    &[Keyword::Reaction],
    &[play(&[a_unit("a unit")], befriend)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::priority;
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const FRIENDSHIP: u32 = 90;
    const THEIR_FRIENDSHIP: u32 = 91;
    const PIGEONS: u32 = 92;
    const HUNTER: u32 = 93;
    const PUP: u32 = 94;
    const PORO: u32 = 95;
    const SECOND_PORO: u32 = 96;
    const THEIR_DOG: u32 = 97;
    const CALM_RUNE: u32 = 46;

    fn friendship(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Friendship", 1, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn menagerie() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(friendship(FRIENDSHIP, 0));
        fixture.table.cards.push(friendship(THEIR_FRIENDSHIP, 1));
        fixture.table.cards.push(fixtures::unit(
            PIGEONS,
            fixtures::BASE,
            0,
            "Crimson Pigeons",
            2,
        ));
        fixture.table.cards.push(fixtures::unit(
            HUNTER,
            fixtures::BF1,
            0,
            "Frisky Hunter (Alternate Art)",
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(PUP, fixtures::BASE, 0, "Loyal Pup", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(PORO, fixtures::BASE, 0, "Pouty Poro", 1));
        fixture.table.cards.push(fixtures::unit(
            SECOND_PORO,
            fixtures::BASE,
            0,
            "Daring Poro",
            2,
        ));
        fixture.table.cards.push(fixtures::unit(
            THEIR_DOG,
            fixtures::BASE,
            1,
            "Scorchclaw",
            3,
        ));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 1, "Calm", false));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_reaction_over_one_unit_that_reads_the_four_tags_through_daisys_seam() {
        assert!(std::ptr::eq(script_of("Friendship").unwrap(), &CARD));
        assert_eq!(CARD.name, "Friendship");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(CARD.abilities[0].targets[0].filter, UNIT);
        assert_eq!(
            TAGS.map(|(tag, _)| tag),
            ["Bird", "Cat", "Dog", "Poro"],
            "the four tags in the printed order, read through Daisy!'s seam"
        );
    }

    #[test]
    fn each_tag_is_read_off_the_printed_name_and_counted_once_among_your_units() {
        let mut fixture = menagerie();
        let ctx = fixture.ctx();
        assert!(is_bird(&ctx, PIGEONS));
        assert!(
            is_cat(&ctx, HUNTER),
            "an alternate art print is the same cat"
        );
        assert!(is_dog(&ctx, PUP));
        assert!(is_poro(&ctx, PORO));
        assert!(!is_bird(&ctx, PUP) && !is_cat(&ctx, PORO) && !is_dog(&ctx, fixtures::VI));
        assert!(is_dog(&ctx, THEIR_DOG));
        assert!(!is_cat(&ctx, THEIR_DOG));
        assert!(!is_dog(&ctx, fixtures::HAND_UNIT), "not on the board");
        assert_eq!(
            tag_names_among_your_units(&ctx, 0),
            ["Bird", "Cat", "Dog", "Poro"],
            "two Poros count once"
        );
        assert_eq!(friendship_might(&ctx, 0), 4);
        assert_eq!(
            tag_names_among_your_units(&ctx, 1),
            ["Dog"],
            "each seat reads its own units"
        );
        assert_eq!(friendship_might(&ctx, 1), 1);
    }

    #[test]
    fn a_full_menagerie_gives_the_chosen_unit_four_might_until_the_turn_ends() {
        let mut fixture = menagerie();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FRIENDSHIP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert!(
            fixtures::labels(&ctx).contains(&"{card 81}".to_string()),
            "any unit, an enemy too"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 6, "2 + 4");
        assert_eq!(
            ctx.state_of(fixtures::THEIR_UNIT).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} gets +4 might this turn · Bird, Cat, Dog, Poro".to_string()));
        assert_eq!(ctx.card(FRIENDSHIP).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_tags_are_counted_as_the_spell_resolves_and_without_any_nothing_is_layered() {
        let mut fixture = menagerie();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FRIENDSHIP).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        for gone in [PIGEONS, HUNTER] {
            ctx.table
                .apply_entry(&fixtures::move_action(gone, fixtures::TRASH, 0), 0)
                .unwrap();
        }
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "3 + 2 · the Bird and the Cat left before it resolved"
        );
        drop(ctx);

        let mut fixture = menagerie();
        fixture
            .table
            .cards
            .retain(|card| ![PIGEONS, HUNTER, PUP, PORO, SECOND_PORO].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FRIENDSHIP).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.state_of(fixtures::VI).is_none(), "+0 layers nothing");
        assert!(ctx.blob.log.contains(
            &"{card 50} gets nothing · no Bird, Cat, Dog or Poro among {seat 0}'s units"
                .to_string()
        ));
    }

    #[test]
    fn the_other_seat_reacts_on_my_turn_and_counts_its_own_dog_only() {
        let mut fixture = menagerie();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_FRIENDSHIP).unwrap();
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            3,
            "2 + 1 · my four tags are not theirs"
        );
    }

    #[test]
    fn a_gear_or_a_hand_card_is_refused_and_a_cancelled_friendship_goes_home() {
        let mut fixture = menagerie();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FRIENDSHIP).unwrap();
        for wrong in [fixtures::HAND_GEAR, fixtures::HAND_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(FRIENDSHIP).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    #[ignore = "engine gap · tags on the face (CardInfo carries no tags, the M10/M11 row): daisy::{BIRDS, CATS, DOGS} and poro_herder::is_poro read printed names, so a unit tagged under a name the lists do not know, or one granted a tag as it is played (Ivern - Friend to All), is invisible to friendship_might"]
    fn a_unit_the_lists_do_not_know_still_counts_by_its_tag() {
        let mut fixture = menagerie();
        fixture.table.card_mut(PUP).unwrap().name = "Unlisted Hound".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_dog(&ctx, PUP));
        assert_eq!(friendship_might(&ctx, 0), 4);
    }
}
