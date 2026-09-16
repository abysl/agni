use super::poro_herder::is_poro;
use super::prelude::{
    a_card, card_target, done, friendly_units, on_attack, stun, unit, when, with_statics,
    ENEMY_UNIT_HERE,
};
use super::{base_name, Card, Cost, Flow, Item, Source, Stage, Static, TargetSpec};
use crate::engine::ctx::{Ctx, Event};

pub const TAGS: usize = 4;

pub const BIRDS: [&str; 10] = [
    "Anivia - Primal",
    "Astral Heron",
    "Azir - Ascendant",
    "Azir - Sovereign",
    "Bird",
    "Crimson Pigeons",
    "Eclipse Herald",
    "Soaring Scout",
    "Solari Sunhawk",
    "Zephyr Sage",
];

pub const CATS: [&str; 14] = [
    "Alpha Wildclaw",
    "Fallen Feline",
    "Fretful Feline",
    "Frisky Hunter",
    "Mutated Mouser",
    "Nidalee - Cat Form",
    "Pakaa Cub",
    "Pakaa Protector",
    "Rengar - Pouncing",
    "Rengar - Trophy Hunter",
    "Rengar - Unseen",
    "Rengar, Trophy Hunter",
    "Steel Paws",
    "Yuumi - Magical Cat",
];

pub const DOGS: [&str; 15] = [
    "Cemetery Attendant",
    "Eager Drakehound",
    "Frostcoat Cub",
    "Frostcoat Mother",
    "Gustwalker",
    "Hungry Wolf",
    "Loyal Pup",
    "Mosstomper",
    "Nasus, Ascended",
    "Nasus, Guardian of Knowledge",
    "Scorchclaw",
    "Stalking Wolf",
    "Starhound",
    "Trusty Ramhound",
    "Warwick - Hunter",
];

fn named_among(ctx: &Ctx, card: u32, names: &[&str]) -> bool {
    ctx.is_unit(card)
        && ctx
            .card(card)
            .is_some_and(|held| names.contains(&base_name(&held.name)))
}

pub fn is_bird(ctx: &Ctx, card: u32) -> bool {
    named_among(ctx, card, &BIRDS)
}

pub fn is_cat(ctx: &Ctx, card: u32) -> bool {
    named_among(ctx, card, &CATS)
}

pub fn is_dog(ctx: &Ctx, card: u32) -> bool {
    named_among(ctx, card, &DOGS)
}

pub const TAG_READERS: [fn(&Ctx, u32) -> bool; TAGS] = [is_bird, is_cat, is_dog, is_poro];

pub fn tags_among_your_units(ctx: &Ctx, seat: u8) -> usize {
    let units = friendly_units(ctx, seat);
    TAG_READERS
        .iter()
        .filter(|has_tag| units.iter().any(|unit| has_tag(ctx, *unit)))
        .count()
}

pub fn your_units_have_all_four_tags(ctx: &Ctx, seat: u8) -> bool {
    tags_among_your_units(ctx, seat) == TAGS
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    Cost {
        energy: u8::try_from(tags_among_your_units(ctx, seat)).unwrap_or(u8::MAX),
        power: &[],
    }
}

pub fn enters_ready(_: &Ctx, _: u32) -> bool {
    true
}

fn all_four_on_the_attack(ctx: &Ctx, _: &Event, source: Source) -> bool {
    your_units_have_all_four_tags(ctx, ctx.controller(source.card))
}

pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to stun");

fn stomp(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        stun(ctx, unit);
    }
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Daisy!",
        &[],
        &[when(on_attack(&[TARGET], stomp), all_four_on_the_attack)],
    ),
    &[
        Static::SelfDiscount(discount),
        Static::EntersReady(enters_ready),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::cost;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, triggers};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const DAISY: u32 = 90;
    const BIRD: u32 = 91;
    const CAT: u32 = 92;
    const DOG: u32 = 93;
    const PORO: u32 = 94;
    const ENERGY: u8 = 9;
    const MIGHT: u8 = 8;
    const RUNES: [u32; 7] = [100, 101, 102, 103, 104, 105, 106];

    fn daisy(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(2),
            domain: vec!["Calm".into(), "Order".into()],
            ..fixtures::unit(DAISY, zone, seat, "Daisy!", MIGHT)
        }
    }

    fn friends(fixture: &mut Fixture, which: &[(u32, &str)]) {
        for (id, name) in which {
            fixture
                .table
                .cards
                .push(fixtures::unit(*id, fixtures::BASE, 0, name, 2));
        }
    }

    fn grove(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(daisy(zone, 0));
        for (index, rune) in RUNES.iter().enumerate() {
            let domain = if index % 2 == 0 { "Calm" } else { "Order" };
            fixture
                .table
                .cards
                .push(fixtures::rune(*rune, 0, domain, false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(DAISY).unwrap(), &CARD));
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: DAISY }, seat, Origin::Hand)
    }

    #[test]
    fn the_script_is_a_unit_with_a_self_discount_a_gated_attack_stun_and_the_enters_ready_seam() {
        assert!(std::ptr::eq(script_of("Daisy!").unwrap(), &CARD));
        assert_eq!(CARD.name, "Daisy!");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.additional.is_none());
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert_eq!(CARD.abilities.len(), 1);
        let stomp = &CARD.abilities[0];
        assert_eq!(stomp.trigger, Trigger::Attacks(Who::Me));
        assert!(stomp.condition.is_some());
        assert_eq!(stomp.targets, &[TARGET]);
        for list in [&BIRDS[..], &CATS[..], &DOGS[..]] {
            assert!(list.windows(2).all(|pair| pair[0] < pair[1]), "sorted");
        }
        let mut fixture = grove(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(enters_ready(&ctx, DAISY), "unconditional");
    }

    #[test]
    fn the_tag_readers_know_every_printed_bird_cat_dog_and_poro_by_name_and_not_a_gear() {
        let mut fixture = grove(fixtures::HAND);
        friends(
            &mut fixture,
            &[
                (BIRD, "Soaring Scout"),
                (CAT, "Pakaa Cub"),
                (DOG, "Loyal Pup"),
                (PORO, "Daring Poro"),
            ],
        );
        fixture
            .table
            .cards
            .push(fixtures::gear(95, fixtures::BASE, 0, "Eclipse Herald", 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_bird(&ctx, BIRD) && !is_cat(&ctx, BIRD) && !is_dog(&ctx, BIRD));
        assert!(is_cat(&ctx, CAT) && !is_bird(&ctx, CAT));
        assert!(is_dog(&ctx, DOG) && !is_cat(&ctx, DOG));
        assert!(is_poro(&ctx, PORO) && !is_dog(&ctx, PORO));
        assert!(!is_bird(&ctx, 95), "a gear named like a Bird is no unit");
        assert!(!is_bird(&ctx, DAISY) && !is_cat(&ctx, DAISY) && !is_dog(&ctx, DAISY));
        assert!(!is_poro(&ctx, DAISY), "Daisy is an Ivern, Ionia unit");
    }

    #[test]
    fn each_tag_among_your_units_takes_one_energy_off_and_the_same_tag_twice_counts_once() {
        let mut fixture = grove(fixtures::HAND);
        friends(
            &mut fixture,
            &[(BIRD, "Soaring Scout"), (97, "Astral Heron")],
        );
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(tags_among_your_units(&ctx, 0), 1);
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY - 1);
        assert_eq!(cost::total(&ctx, DAISY, false).energy, ENERGY - 1);
        assert_eq!(
            cost::of_item(&ctx, &item(1), None).energy,
            ENERGY,
            "the other seat's units have no Bird"
        );
        drop(ctx);
        let mut fixture = grove(fixtures::HAND);
        friends(
            &mut fixture,
            &[
                (BIRD, "Soaring Scout"),
                (CAT, "Pakaa Cub"),
                (DOG, "Loyal Pup"),
                (PORO, "Daring Poro"),
            ],
        );
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(tags_among_your_units(&ctx, 0), TAGS);
        assert!(your_units_have_all_four_tags(&ctx, 0));
        assert!(!your_units_have_all_four_tags(&ctx, 1));
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY - 4);
        drop(ctx);
        let mut fixture = grove(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(tags_among_your_units(&ctx, 0), 0);
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, ENERGY);
    }

    #[test]
    fn played_with_all_four_tags_she_costs_five_and_two_power_and_at_nine_she_is_refused() {
        let mut fixture = grove(fixtures::HAND);
        friends(
            &mut fixture,
            &[
                (BIRD, "Soaring Scout"),
                (CAT, "Pakaa Cub"),
                (DOG, "Loyal Pup"),
                (PORO, "Daring Poro"),
            ],
        );
        for rune in [41, 42, 43, 104, 105, 106] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 5);
        fixtures::play_from_hand(&mut ctx, 0, DAISY).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(DAISY), Some(Location::Base(0)));
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "five energy off five runes"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut short = grove(fixtures::HAND);
        for rune in [41, 42, 43, 104, 105, 106] {
            short.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = short.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, DAISY),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 5
            }),
            "no tags, no discount"
        );
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    fn attacking_with_all_four_tags_stuns_an_enemy_unit_here_and_with_three_it_does_not_trigger() {
        let mut fixture = grove(fixtures::BF1);
        friends(
            &mut fixture,
            &[
                (BIRD, "Soaring Scout"),
                (CAT, "Pakaa Cub"),
                (DOG, "Loyal Pup"),
                (PORO, "Daring Poro"),
            ],
        );
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: DAISY });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::THEIR_UNIT)],
            "the enemy unit here and not the Sprite elsewhere"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut three = grove(fixtures::BF1);
        friends(
            &mut three,
            &[
                (BIRD, "Soaring Scout"),
                (CAT, "Pakaa Cub"),
                (DOG, "Loyal Pup"),
            ],
        );
        three.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        three.blob.set_holder(fixtures::BF1, Some(1));
        three.resolve();
        let mut ctx = three.ctx();
        ctx.raise(Event::Attacks { card: DAISY });
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "three tags are not all four"
        );
        ctx.raise(Event::Defends { card: DAISY });
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "the stomp is on the attack only"
        );
    }

    #[test]
    fn played_from_hand_she_enters_ready() {
        let mut fixture = grove(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DAISY).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(DAISY));
        assert!(
            !ctx.card(DAISY).unwrap().exhausted,
            "369.3 · I enter ready replaces the exhausted entry"
        );
    }

    #[test]
    #[ignore = "engine gap · CardInfo carries no tags: a Bird, Cat, Dog or Poro tagged under a name the lists do not know (a token, a future print) is invisible to the discount and the stomp"]
    fn a_tagged_unit_the_name_lists_do_not_know_still_counts() {
        let mut fixture = grove(fixtures::HAND);
        friends(&mut fixture, &[(BIRD, "Unlisted Fledgling")]);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(tags_among_your_units(&ctx, 0), 1);
    }
}
