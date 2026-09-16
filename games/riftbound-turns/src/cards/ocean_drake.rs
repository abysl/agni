use super::herald_of_scales::{is_dragon, DRAGONS};
use super::prelude::{
    bounce, card_target, done, open_battlefields, optional, play, target, unit, with_statics,
    Location,
};
use super::{Card, Filter, Flow, Item, Stage, Static, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

const fn dragon_names() -> [Filter; DRAGONS.len()] {
    let mut names = [Filter::Any; DRAGONS.len()];
    let mut index = 0;
    while index < DRAGONS.len() {
        names[index] = Filter::Named(DRAGONS[index]);
        index += 1;
    }
    names
}

const DRAGON_NAMES: [Filter; DRAGONS.len()] = dragon_names();

pub const NON_DRAGON_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Not(&Filter::Or(&DRAGON_NAMES))]);

pub const TIDE: TargetSpec = target(
    NON_DRAGON_UNIT,
    0,
    1,
    TargetKind::Card,
    "a non-Dragon unit to return to its owner's hand",
);

pub fn open_play_locations(ctx: &Ctx, _: u8, card: u32) -> Vec<Location> {
    if ctx
        .script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
    {
        open_battlefields(ctx)
    } else {
        Vec::new()
    }
}

fn tidal_wave(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if is_dragon(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is a Dragon · it stays"));
        return done();
    }
    let owner = ctx.owner(unit);
    if bounce(ctx, unit) {
        ctx.narrate(format!(
            "{{card {}}} returns {{card {unit}}} to {{seat {owner}}}'s hand",
            item.kind.source()
        ));
    }
    done()
}

pub static CARD: Card = with_statics(
    unit("Ocean Drake", &[], &[optional(play(&[TIDE], tidal_wave))]),
    &[Static::PlayLocations(open_play_locations)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::engine::{play as play_engine, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const DRAKE: u32 = 90;
    const THEIR_DRAGON: u32 = 91;
    const THEIR_BRUTE: u32 = 92;

    fn drake(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(DRAKE, zone, 0, "Ocean Drake", 7)
        }
    }

    fn reef() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(drake(fixtures::HAND));
        fixture.table.cards.push(fixtures::unit(
            THEIR_DRAGON,
            fixtures::BF1,
            1,
            "Dune Drake",
            4,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: DRAKE,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn enter(ctx: &mut Ctx) -> u16 {
        play_engine::begin(ctx, 0, DRAKE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(ctx).unwrap();
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play asks for a non-Dragon unit, not {other:?}"),
        }
    }

    #[test]
    fn the_script_is_one_optional_play_trigger_over_a_non_dragon_unit() {
        assert!(std::ptr::eq(script_of("Ocean Drake").unwrap(), &CARD));
        assert_eq!(CARD.name, "Ocean Drake");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.grants_play_locations());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.optional, "you may");
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets, &[TIDE]);
        assert_eq!((TIDE.min, TIDE.max), (0, 1));
        assert_eq!(TIDE.kind, TargetKind::Card);
        assert_eq!(TIDE.filter, NON_DRAGON_UNIT);
        assert_eq!(DRAGON_NAMES.len(), DRAGONS.len());
        for name in DRAGONS {
            assert!(
                DRAGON_NAMES.contains(&Filter::Named(name)),
                "{name} is excluded by name"
            );
        }
    }

    #[test]
    fn the_dragons_are_the_heralds_list_with_the_vendetta_four() {
        let mut fixture = reef();
        let ctx = fixture.ctx();
        assert!(is_dragon(&ctx, DRAKE), "Ocean Drake is a Dragon");
        assert!(
            is_dragon(&ctx, THEIR_DRAGON),
            "Dune Drake through the herald's list"
        );
        assert!(!is_dragon(&ctx, THEIR_BRUTE));
        assert!(!is_dragon(&ctx, fixtures::VI));
        assert!(!is_dragon(&ctx, 999));
        for name in [
            "Cloud Drake",
            "Corrupted Dragon",
            "Eclipse Dragon",
            "Ocean Drake",
        ] {
            assert!(DRAGONS.contains(&name), "{name} is on the herald's list");
        }
    }

    #[test]
    fn played_it_offers_the_non_dragon_units_and_returns_the_pick_to_its_owners_hand() {
        let mut fixture = reef();
        let mut ctx = fixture.ctx();
        let item = enter(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_BRUTE}}}"),
                "skip".to_string()
            ],
            "neither the Dune Drake nor the Ocean Drake itself"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[THEIR_DRAGON]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a Dragon is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[DRAKE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "it is a Dragon too"
        );
        let hand = ctx.hand_of(1).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DRAKE
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_BRUTE)]);
        assert!(
            ctx.on_board(THEIR_BRUTE),
            "the bounce waits for the trigger"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(THEIR_BRUTE));
        assert_eq!(ctx.hand_of(1).len(), hand + 1);
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_BRUTE,
            zone: fixtures::HAND,
            seat: 1,
            index: TOP
        }));
        assert!(ctx.on_board(THEIR_DRAGON));
        assert!(ctx.on_board(DRAKE));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DRAKE}}} returns {{card {THEIR_BRUTE}}} to {{seat 1}}'s hand"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_returns_nobody() {
        let mut fixture = reef();
        let mut ctx = fixture.ctx();
        enter(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(THEIR_BRUTE));
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.on_board(DRAKE));
    }

    #[test]
    fn the_open_battlefields_are_its_own_play_locations_beside_the_base() {
        let mut fixture = reef();
        fixture.table.card_mut(THEIR_DRAGON).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(THEIR_BRUTE).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            open_play_locations(&ctx, 0, DRAKE),
            [Location::Battlefield(fixtures::BF1)],
            "the one open battlefield; the enemy's held one is not"
        );
        assert!(open_play_locations(&ctx, 0, fixtures::HAND_UNIT).is_empty());
        assert_eq!(
            legal::locations_for(&ctx, 0, DRAKE),
            [Location::Base(0), Location::Battlefield(fixtures::BF1)]
        );
        assert_eq!(
            legal::locations_for(&ctx, 0, fixtures::HAND_UNIT),
            [Location::Base(0)],
            "the grant is its own"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BASE)),
            Ok(Intent::Play {
                card: DRAKE,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false,
            })
        );
    }

    #[test]
    fn it_may_be_played_to_an_open_battlefield_and_contests_it() {
        let mut fixture = reef();
        fixture.table.card_mut(THEIR_DRAGON).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(THEIR_BRUTE).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Ok(Intent::Play {
                card: DRAKE,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF2)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "the enemy's held battlefield is not open"
        );
        ctx.table
            .apply_entry(&fixtures::move_action(DRAKE, fixtures::BF1, 0), 0)
            .unwrap();
        crate::engine::act(
            &mut ctx,
            0,
            Intent::Play {
                card: DRAKE,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(DRAKE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.contester(fixtures::BF1), Some(0));
    }
}
