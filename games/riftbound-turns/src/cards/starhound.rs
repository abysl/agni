use super::prelude::{a_card, card_target, done, play, unit};
use super::{base_name, Card, Filter, Flow, Item, Stage, TargetSpec, KIND_UNIT};
use crate::engine::ctx::Ctx;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const COMPANIONS: [&str; 53] = [
    "Affectionate Poro",
    "Alpha Wildclaw",
    "Anivia - Primal",
    "Astral Heron",
    "Azir - Ascendant",
    "Azir - Sovereign",
    "Bird",
    "Cemetery Attendant",
    "Crimson Pigeons",
    "Daring Poro",
    "Eager Drakehound",
    "Eclipse Herald",
    "Fallen Feline",
    "Fretful Feline",
    "Frisky Hunter",
    "Frostcoat Cub",
    "Frostcoat Mother",
    "Gustwalker",
    "Hungry Wolf",
    "Lonely Poro",
    "Loyal Poro",
    "Loyal Pup",
    "Mosstomper",
    "Mutated Mouser",
    "Mystic Poro",
    "Nasus, Ascended",
    "Nasus, Guardian of Knowledge",
    "Nidalee - Cat Form",
    "Ol' Poro",
    "Pakaa Cub",
    "Pakaa Protector",
    "Patched Porobot",
    "Plundering Poro",
    "Pouty Poro",
    "Punching Poro",
    "Rengar - Pouncing",
    "Rengar - Trophy Hunter",
    "Rengar - Unseen",
    "Rengar, Trophy Hunter",
    "Scorchclaw",
    "Sinister Poro",
    "Soaring Scout",
    "Solari Sunhawk",
    "Stalking Wolf",
    "Stalwart Poro",
    "Starhound",
    "Steel Paws",
    "Trusty Ramhound",
    "Ultrasoft Poro",
    "Veteran Poro",
    "Warwick - Hunter",
    "Yuumi - Magical Cat",
    "Zephyr Sage",
];

pub const COMPANION: Filter = Filter::Or(&[
    Filter::Named(COMPANIONS[0]),
    Filter::Named(COMPANIONS[1]),
    Filter::Named(COMPANIONS[2]),
    Filter::Named(COMPANIONS[3]),
    Filter::Named(COMPANIONS[4]),
    Filter::Named(COMPANIONS[5]),
    Filter::Named(COMPANIONS[6]),
    Filter::Named(COMPANIONS[7]),
    Filter::Named(COMPANIONS[8]),
    Filter::Named(COMPANIONS[9]),
    Filter::Named(COMPANIONS[10]),
    Filter::Named(COMPANIONS[11]),
    Filter::Named(COMPANIONS[12]),
    Filter::Named(COMPANIONS[13]),
    Filter::Named(COMPANIONS[14]),
    Filter::Named(COMPANIONS[15]),
    Filter::Named(COMPANIONS[16]),
    Filter::Named(COMPANIONS[17]),
    Filter::Named(COMPANIONS[18]),
    Filter::Named(COMPANIONS[19]),
    Filter::Named(COMPANIONS[20]),
    Filter::Named(COMPANIONS[21]),
    Filter::Named(COMPANIONS[22]),
    Filter::Named(COMPANIONS[23]),
    Filter::Named(COMPANIONS[24]),
    Filter::Named(COMPANIONS[25]),
    Filter::Named(COMPANIONS[26]),
    Filter::Named(COMPANIONS[27]),
    Filter::Named(COMPANIONS[28]),
    Filter::Named(COMPANIONS[29]),
    Filter::Named(COMPANIONS[30]),
    Filter::Named(COMPANIONS[31]),
    Filter::Named(COMPANIONS[32]),
    Filter::Named(COMPANIONS[33]),
    Filter::Named(COMPANIONS[34]),
    Filter::Named(COMPANIONS[35]),
    Filter::Named(COMPANIONS[36]),
    Filter::Named(COMPANIONS[37]),
    Filter::Named(COMPANIONS[38]),
    Filter::Named(COMPANIONS[39]),
    Filter::Named(COMPANIONS[40]),
    Filter::Named(COMPANIONS[41]),
    Filter::Named(COMPANIONS[42]),
    Filter::Named(COMPANIONS[43]),
    Filter::Named(COMPANIONS[44]),
    Filter::Named(COMPANIONS[45]),
    Filter::Named(COMPANIONS[46]),
    Filter::Named(COMPANIONS[47]),
    Filter::Named(COMPANIONS[48]),
    Filter::Named(COMPANIONS[49]),
    Filter::Named(COMPANIONS[50]),
    Filter::Named(COMPANIONS[51]),
    Filter::Named(COMPANIONS[52]),
]);

pub const FRIENDLY_COMPANION_IN_TRASH: Filter = Filter::And(&[
    Filter::Kind(KIND_UNIT),
    Filter::InTrash,
    Filter::Friendly,
    COMPANION,
]);

pub const BURIED_COMPANION: TargetSpec = a_card(
    FRIENDLY_COMPANION_IN_TRASH,
    "a Bird, Cat, Dog or Poro in your trash to return to your hand",
);

pub fn is_companion(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card)
        .is_some_and(|held| COMPANIONS.contains(&base_name(&held.name)))
}

fn return_from_trash(ctx: &mut Ctx, card: u32) -> bool {
    if !ctx.in_trash(card) {
        return false;
    }
    let Some(hand) = ctx.zones.hand else {
        return false;
    };
    let owner = ctx.owner(card);
    ctx.emit(Effect::Move {
        card,
        zone: hand,
        seat: owner,
        index: TOP,
    });
    ctx.narrate(format!(
        "{{seat {owner}}} returns {{card {card}}} from the trash to their hand"
    ));
    true
}

fn fetch(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(card) = card_target(ctx, item, 0) {
        return_from_trash(ctx, card);
    }
    done()
}

pub static CARD: Card = unit("Starhound", &[], &[play(&[BURIED_COMPANION], fetch)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::daisy::{BIRDS, CATS, DOGS};
    use crate::cards::poro_herder::is_poro;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const HOUND: u32 = 90;
    const BURIED_PUP: u32 = 91;
    const BURIED_PORO: u32 = 92;
    const BURIED_KNIGHT: u32 = 93;
    const THEIR_BURIED_CAT: u32 = 94;
    const CAT_ON_BOARD: u32 = 95;
    const BURIED_SPELL: u32 = 96;
    const ORDER_RUNE: u32 = 46;

    fn hound(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(HOUND, zone, seat, "Starhound", 6)
        }
    }

    fn kennel() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hound(fixtures::HAND, 0));
        fixture.table.cards.push(fixtures::unit(
            BURIED_PUP,
            fixtures::TRASH,
            0,
            "Loyal Pup",
            1,
        ));
        fixture.table.cards.push(fixtures::unit(
            BURIED_PORO,
            fixtures::TRASH,
            0,
            "Daring Poro",
            1,
        ));
        fixture.table.cards.push(fixtures::unit(
            BURIED_KNIGHT,
            fixtures::TRASH,
            0,
            "Brute",
            3,
        ));
        fixture.table.cards.push(fixtures::unit(
            THEIR_BURIED_CAT,
            fixtures::TRASH,
            1,
            "Pakaa Cub",
            1,
        ));
        fixture.table.cards.push(fixtures::unit(
            CAT_ON_BOARD,
            fixtures::BASE,
            0,
            "Alpha Wildclaw",
            5,
        ));
        fixture.table.cards.push(fixtures::spell(
            BURIED_SPELL,
            fixtures::TRASH,
            0,
            "Poro Snax",
            1,
            0,
        ));
        for id in ORDER_RUNE..ORDER_RUNE + 3 {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Order", false));
        }
        fixture.resolve();
        fixture
    }

    fn whistle(ctx: &mut Ctx) -> u16 {
        play_engine::begin(ctx, 0, HOUND, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(ctx).unwrap();
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play asks for a companion in the trash, not {other:?}"),
        }
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_one_companion_in_your_trash() {
        assert!(std::ptr::eq(script_of("Starhound").unwrap(), &CARD));
        assert_eq!(CARD.name, "Starhound");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert_eq!(ability.targets, [BURIED_COMPANION]);
        assert_eq!((BURIED_COMPANION.min, BURIED_COMPANION.max), (1, 1));
        assert_eq!(BURIED_COMPANION.kind, TargetKind::Card);
        assert_eq!(BURIED_COMPANION.filter, FRIENDLY_COMPANION_IN_TRASH);
        let Filter::Or(names) = COMPANION else {
            panic!("the companion filter is a name list");
        };
        assert_eq!(names.len(), COMPANIONS.len());
        for (index, name) in COMPANIONS.iter().enumerate() {
            assert_eq!(names[index], Filter::Named(name));
        }
        let mut sorted = COMPANIONS.to_vec();
        sorted.sort_unstable();
        assert_eq!(&COMPANIONS[..], &sorted[..], "the names are sorted");
    }

    #[test]
    fn the_companions_are_the_daisy_lists_plus_every_poro_the_herder_reads() {
        let mut fixture = Fixture::enforced();
        let mut poros: Vec<&str> = Vec::new();
        for (index, name) in COMPANIONS.iter().enumerate() {
            let tagged = BIRDS.contains(name) || CATS.contains(name) || DOGS.contains(name);
            if !tagged {
                poros.push(name);
                fixture.table.cards.push(fixtures::unit(
                    300 + index as u32,
                    fixtures::BASE,
                    0,
                    name,
                    1,
                ));
            }
        }
        fixture.resolve();
        let ctx = fixture.ctx();
        for (index, name) in COMPANIONS.iter().enumerate() {
            if poros.contains(name) {
                assert!(
                    is_poro(&ctx, 300 + index as u32),
                    "{name} is a Poro to the herder"
                );
            }
        }
        let mut union: Vec<&str> = BIRDS
            .iter()
            .chain(CATS.iter())
            .chain(DOGS.iter())
            .copied()
            .chain(poros.iter().copied())
            .collect();
        union.sort_unstable();
        union.dedup();
        assert_eq!(&COMPANIONS[..], &union[..], "one list, one reading");
    }

    #[test]
    fn the_list_names_every_printed_bird_cat_dog_and_poro_and_agrees_with_the_poro_herder() {
        let mut fixture = kennel();
        let ctx = fixture.ctx();
        for card in [BURIED_PUP, BURIED_PORO, THEIR_BURIED_CAT, CAT_ON_BOARD] {
            assert!(is_companion(&ctx, card), "{card} is a companion");
        }
        assert!(is_companion(&ctx, HOUND), "the hound is a Dog");
        assert!(!is_companion(&ctx, BURIED_KNIGHT));
        assert!(!is_companion(&ctx, fixtures::VI));
        assert!(!is_companion(&ctx, 999));
        assert!(is_poro(&ctx, BURIED_PORO), "the herder reads the same Poro");
        assert!(!is_poro(&ctx, BURIED_PUP), "a Dog is no Poro to the herder");
    }

    #[test]
    fn playing_the_hound_offers_only_your_buried_companions_and_the_pick_returns_to_hand() {
        let mut fixture = kennel();
        let mut ctx = fixture.ctx();
        let item = whistle(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BURIED_PUP}}}"), format!("{{card {BURIED_PORO}}}")],
            "the knight is no companion, the cat is theirs, the other cat is on the board, the snax is a spell"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {HOUND}}}: choose a Bird, Cat, Dog or Poro in your trash to return to your hand (0 of 1)")
        );
        for refused in [BURIED_KNIGHT, THEIR_BURIED_CAT, CAT_ON_BOARD, BURIED_SPELL] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, item, 0, &[refused]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{refused} is refused"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BURIED_PUP}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == HOUND
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BURIED_PUP)]);
        assert!(ctx.in_trash(BURIED_PUP), "the return waits for the trigger");
        let hand = ctx.hand_of(0).len();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BURIED_PUP).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.effects.contains(&Effect::Move {
            card: BURIED_PUP,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.in_trash(BURIED_PORO), "only the pick returns");
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} returns {{card {BURIED_PUP}}} from the trash to their hand"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_companion_in_your_trash_the_trigger_fizzles_and_the_hound_still_lands() {
        let mut fixture = kennel();
        fixture
            .table
            .cards
            .retain(|card| ![BURIED_PUP, BURIED_PORO].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_engine::begin(&mut ctx, 0, HOUND, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to ask");
        assert!(ctx.blob.chain.is_empty(), "390.3 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {HOUND}}} trigger fizzles · no legal target"
        )));
        assert_eq!(ctx.location(HOUND), Some(Location::Base(0)));
        assert!(ctx.in_trash(THEIR_BURIED_CAT), "theirs stays theirs");
    }

    #[test]
    fn a_pick_that_left_the_trash_before_resolution_is_left_alone() {
        let mut fixture = kennel();
        let mut ctx = fixture.ctx();
        whistle(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BURIED_PORO}}}")).unwrap();
        ctx.table.card_mut(BURIED_PORO).unwrap().zone = Some(fixtures::BANISHMENT);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(BURIED_PORO).unwrap().zone,
            Some(fixtures::BANISHMENT)
        );
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Move { card, .. } if *card == BURIED_PORO)));
    }

    #[test]
    #[ignore = "CardInfo carries no tags: is_companion reads the printed name over the Bird, Cat, Dog and Poro lists of every set, so a unit tagged under a name the list does not know is invisible; the engine owes a tags row on the face and Filter::Tag"]
    fn a_dog_tagged_unit_under_a_name_the_list_does_not_know_still_counts() {
        let mut fixture = kennel();
        fixture
            .table
            .cards
            .push(fixtures::unit(200, fixtures::TRASH, 0, "Fluffy Friend", 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_companion(&ctx, 200));
    }
}
