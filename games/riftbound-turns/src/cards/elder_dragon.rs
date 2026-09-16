use super::prelude::{card_targets, deal, done, play, target, unit, with_statics, ENEMY_UNIT};
use super::{Card, Filter, Flow, Item, Stage, Static, TargetKind, TargetSpec};
use crate::engine::ctx::{Ctx, Location};

pub const DAMAGE: u8 = 1;
pub const LOCATIONS_AT_MOST: u8 = 7;

pub const ONE_AT_EACH_LOCATION: TargetSpec = target(
    Filter::And(&[ENEMY_UNIT, Filter::DifferentLocationFromPicks]),
    0,
    LOCATIONS_AT_MOST,
    TargetKind::Card,
    "up to one enemy unit at each location",
);

pub fn is_elder_dragon(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn your_damage_is_lethal_to(ctx: &Ctx, source: u32, unit: u32) -> bool {
    ctx.is_unit(unit) && ctx.controller(unit) != ctx.controller(source)
}

pub fn any_of_your_damage_is_lethal_to(ctx: &Ctx, seat: u8, unit: u32) -> bool {
    ctx.any_damage_is_lethal(seat, unit)
}

pub fn lethal_damage_for(ctx: &Ctx, unit: u32, marked_by: u8) -> i32 {
    if any_of_your_damage_is_lethal_to(ctx, marked_by, unit) {
        1
    } else {
        ctx.current_might(unit).max(1)
    }
}

pub fn one_per_location(ctx: &Ctx, units: &[u32]) -> Vec<u32> {
    let mut seen: Vec<Location> = Vec::new();
    let mut kept = Vec::new();
    for unit in units {
        let Some(at) = ctx.location(*unit) else {
            continue;
        };
        if seen.contains(&at) {
            continue;
        }
        seen.push(at);
        kept.push(*unit);
    }
    kept
}

fn breathe(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let chosen = card_targets(ctx, item);
    let victims = one_per_location(ctx, &chosen);
    if victims.len() < chosen.len() {
        ctx.narrate(format!(
            "{{card {}}} · only the first pick at each location is dealt to",
            item.kind.source()
        ));
    }
    for unit in victims {
        deal(ctx, item, unit, DAMAGE);
    }
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Elder Dragon",
        &[],
        &[play(&[ONE_AT_EACH_LOCATION], breathe)],
    ),
    &[Static::LethalDamage(your_damage_is_lethal_to)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::herald_of_scales::is_dragon;
    use crate::cards::prelude::move_unit;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cleanup, play as play_engine, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const DRAGON: u32 = 90;
    const THEIR_BRUTE: u32 = 91;
    const THEIR_SCOUT: u32 = 92;
    const THEIR_GUARD: u32 = 93;
    const ALLY: u32 = 94;
    const PLAIN: u32 = 54;

    fn dragon(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Body".into()],
            ..fixtures::unit(DRAGON, zone, seat, "Elder Dragon", 10)
        }
    }

    fn roost(dragon_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dragon(dragon_zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SCOUT, fixtures::BF1, 1, "Scout", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_GUARD, fixtures::BF3, 1, "Guard", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_holder(fixtures::BF3, Some(1));
        fixture.resolve();
        fixture
    }

    fn descend(ctx: &mut Ctx) -> u16 {
        play_engine::begin(ctx, 0, DRAGON, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(ctx).unwrap();
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play asks for enemy units, not {other:?}"),
        }
    }

    #[test]
    fn the_script_is_a_dragon_whose_play_trigger_picks_up_to_one_enemy_unit_per_location() {
        assert!(std::ptr::eq(script_of("Elder Dragon").unwrap(), &CARD));
        assert_eq!(CARD.name, "Elder Dragon");
        assert!(CARD.keywords.is_empty());
        assert!(matches!(CARD.statics, [Static::LethalDamage(_)]));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert_eq!(ability.targets, [ONE_AT_EACH_LOCATION]);
        assert_eq!(
            (ONE_AT_EACH_LOCATION.min, ONE_AT_EACH_LOCATION.max),
            (0, LOCATIONS_AT_MOST)
        );
        assert_eq!(
            ONE_AT_EACH_LOCATION.filter,
            Filter::And(&[ENEMY_UNIT, Filter::DifferentLocationFromPicks])
        );
        assert_eq!(
            (DAMAGE, LOCATIONS_AT_MOST),
            (1, 7),
            "four bases and three battlefields"
        );
        let mut fixture = roost(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(is_dragon(&ctx, DRAGON), "a Dragon to the Gemdragon");
        assert!(is_elder_dragon(&ctx, DRAGON));
        assert!(!is_elder_dragon(&ctx, fixtures::VI));
    }

    #[test]
    fn the_lethal_seam_reads_one_for_enemy_units_of_a_dragon_in_play_and_might_otherwise() {
        let mut fixture = roost(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(any_of_your_damage_is_lethal_to(&ctx, 0, THEIR_BRUTE));
        assert_eq!(lethal_damage_for(&ctx, THEIR_BRUTE, 0), 1);
        assert_eq!(
            lethal_damage_for(&ctx, THEIR_BRUTE, 1),
            4,
            "their own damage on their own unit reads its Might"
        );
        assert!(
            !any_of_your_damage_is_lethal_to(&ctx, 0, ALLY),
            "friendly units are not enemy units"
        );
        assert_eq!(lethal_damage_for(&ctx, ALLY, 0), 2);
        assert!(
            !any_of_your_damage_is_lethal_to(&ctx, 1, fixtures::VI),
            "the opponent has no dragon"
        );
        assert_eq!(lethal_damage_for(&ctx, fixtures::VI, 1), 3);
        drop(ctx);
        let mut grounded = roost(fixtures::HAND);
        let ctx = grounded.ctx();
        assert!(
            !any_of_your_damage_is_lethal_to(&ctx, 0, THEIR_BRUTE),
            "a dragon in hand is not in play"
        );
        assert_eq!(lethal_damage_for(&ctx, THEIR_BRUTE, 0), 4);
    }

    #[test]
    fn one_per_location_keeps_the_first_pick_at_each_location_in_pick_order() {
        let mut fixture = roost(fixtures::BASE);
        let ctx = fixture.ctx();
        assert_eq!(
            one_per_location(&ctx, &[THEIR_SCOUT, THEIR_BRUTE, THEIR_GUARD]),
            [THEIR_SCOUT, THEIR_GUARD]
        );
        assert_eq!(
            one_per_location(&ctx, &[THEIR_BRUTE, THEIR_GUARD, fixtures::THEIR_UNIT]),
            [THEIR_BRUTE, THEIR_GUARD, fixtures::THEIR_UNIT]
        );
        assert_eq!(one_per_location(&ctx, &[999, THEIR_GUARD]), [THEIR_GUARD]);
        assert_eq!(one_per_location(&ctx, &[]), []);
    }

    #[test]
    fn playing_it_offers_every_enemy_unit_and_deals_one_to_each_pick_at_a_distinct_location() {
        let mut fixture = roost(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let item = descend(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_BRUTE}}}"),
                format!("{{card {THEIR_SCOUT}}}"),
                format!("{{card {THEIR_GUARD}}}"),
                "done".to_string(),
                "skip".to_string()
            ],
            "every enemy unit anywhere, never a friendly one"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {DRAGON}}}: choose up to one enemy unit at each location (0 of 7)")
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[ALLY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[DRAGON]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the dragon itself is refused"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SCOUT}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_GUARD}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DRAGON
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(THEIR_SCOUT),
                TargetRef::Card(THEIR_GUARD),
                TargetRef::Card(fixtures::SPRITE)
            ]
        );
        let chain_item = ctx.blob.chain[0].id;
        assert_eq!(
            ctx.damage_on(THEIR_GUARD),
            0,
            "the damage waits for the trigger"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for unit in [THEIR_SCOUT, THEIR_GUARD, fixtures::SPRITE] {
            assert!(ctx.events.contains(&Event::DamageDealt {
                card: unit,
                n: DAMAGE,
                source: Cause::Item(chain_item)
            }));
        }
        assert!(!ctx.on_board(THEIR_SCOUT), "1 Might dies to 1");
        assert!(!ctx.on_board(THEIR_GUARD));
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 0, "not picked");
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0, "not picked");
        assert!(ctx.on_board(THEIR_BRUTE));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_picks_at_one_location_are_judged_again_at_resolution_and_only_the_first_is_dealt_to() {
        let mut fixture = roost(fixtures::HAND);
        let mut ctx = fixture.ctx();
        descend(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_GUARD}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        move_unit(
            &mut ctx,
            &fixtures::effect_of(0),
            THEIR_GUARD,
            Location::Battlefield(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(THEIR_BRUTE));
        assert_eq!(
            ctx.damage_on(THEIR_GUARD),
            0,
            "355.11.b · the second pick now at {{zone 9}} is dropped"
        );
        assert!(ctx.on_board(THEIR_GUARD));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DRAGON}}} · only the first pick at each location is dealt to"
        )));
    }

    #[test]
    fn skipping_every_pick_deals_nothing_and_with_no_enemy_unit_the_trigger_fizzles() {
        let mut fixture = roost(fixtures::HAND);
        let mut ctx = fixture.ctx();
        descend(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        drop(ctx);
        let mut empty = roost(fixtures::HAND);
        empty.table.cards.retain(|card| {
            ![
                THEIR_BRUTE,
                THEIR_SCOUT,
                THEIR_GUARD,
                fixtures::SPRITE,
                fixtures::THEIR_UNIT,
            ]
            .contains(&card.id)
        });
        empty.resolve();
        let mut ctx = empty.ctx();
        play_engine::begin(&mut ctx, 0, DRAGON, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to ask");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(DRAGON), Some(Location::Base(0)));
    }

    #[test]
    fn a_second_pick_at_an_already_picked_location_is_refused_at_the_prompt() {
        let mut fixture = roost(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let item = descend(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        assert!(!fixtures::labels(&ctx).contains(&format!("{{card {THEIR_SCOUT}}}")));
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[THEIR_BRUTE, THEIR_SCOUT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
    }

    #[test]
    #[ignore = "engine gap · damage modifiers (712–715) and 142.3: damage is one counter per unit with no record of who marked it, and cleanup::dying reads current_might as the lethal amount; the engine owes per-player damage marks and a lethal-damage consult of any_of_your_damage_is_lethal_to before the cleanup kills"]
    fn with_the_dragon_in_play_one_damage_from_you_kills_an_enemy_unit_at_the_cleanup() {
        let mut fixture = roost(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let item = Item::new(
            7,
            ItemKind::Trigger {
                source: DRAGON,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert!(deal(&mut ctx, &item, THEIR_BRUTE, 1));
        assert_eq!(cleanup::lethal_kills(&mut ctx), [THEIR_BRUTE]);
        assert!(!ctx.on_board(THEIR_BRUTE));
        assert!(ctx.on_board(ALLY));
    }
}
