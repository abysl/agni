use super::prelude::{
    a_battlefield, a_card, card_target, deal, done, move_unit, play, spell, zone_target, Location,
};
use super::{Card, Filter, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const MOVABLE_FRIENDLY_UNIT_IN_BASE: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::InBase,
    Filter::Movable,
]);

const UNIT: usize = 0;
const BATTLEFIELD: usize = 1;

fn might_of(ctx: &Ctx, unit: u32) -> u8 {
    u8::try_from(ctx.current_might(unit).max(0)).unwrap_or(u8::MAX)
}

fn enemy_units_at(ctx: &Ctx, seat: u8, zone: u16) -> Vec<u32> {
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat)
        .collect()
}

fn storm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    let Some(zone) = zone_target(item, BATTLEFIELD) else {
        return done();
    };
    let amount = might_of(ctx, unit);
    let enemies = enemy_units_at(ctx, item.controller, zone);
    if enemies.is_empty() {
        ctx.narrate(format!("no enemy unit stands at {{zone {zone}}}"));
    }
    for enemy in enemies {
        if deal(ctx, item, enemy, amount) {
            ctx.narrate(format!("{{card {enemy}}} takes {amount} damage"));
        }
    }
    move_unit(ctx, item, unit, Location::Battlefield(zone));
    done()
}

pub static CARD: Card = spell(
    "Stormbringer",
    &[],
    &[play(
        &[
            a_card(
                MOVABLE_FRIENDLY_UNIT_IN_BASE,
                "a friendly unit in your base",
            ),
            a_battlefield("the battlefield it strikes"),
        ],
        storm,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Keyword, TargetKind, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{PromptWhy, ShowdownStage, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const STORM: u32 = 90;
    const THEIR_STORM: u32 = 91;
    const BRUTE: u32 = 92;
    const RUNT: u32 = 93;
    const ALLY: u32 = 94;
    const BODY_RUNE: u32 = 100;
    const FURY_D: u32 = 101;
    const FURY_E: u32 = 102;

    fn storm_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Stormbringer", 6, 2);
        card.domain = vec!["Fury".into(), "Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(storm_card(STORM, 0));
        fixture.table.cards.push(storm_card(THEIR_STORM, 1));
        for (rune, domain) in [(BODY_RUNE, "Body"), (FURY_D, "Fury"), (FURY_E, "Fury")] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, domain, false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF2, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(RUNT, fixtures::BF2, 1, "Runt", 1));
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

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    fn damage(ctx: &Ctx, unit: u32) -> i32 {
        ctx.table
            .counter(Target::Card(unit), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt { card, n, .. } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_plain_spell_over_a_friendly_unit_in_base_and_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Stormbringer").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Stormbringer");
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[UNIT].filter, MOVABLE_FRIENDLY_UNIT_IN_BASE);
        assert_eq!(ability.targets[BATTLEFIELD].kind, TargetKind::Zone);
        assert_eq!(ability.targets[BATTLEFIELD].filter, Filter::AtBattlefield);
        assert!(
            ability.candidates.is_none(),
            "355.10.b · the battlefield is the target, its units are not"
        );
    }

    #[test]
    fn every_enemy_at_the_battlefield_takes_the_units_might_then_the_unit_moves_there() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STORM).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "only the friendly unit in the base"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "every battlefield, occupied or not"
        );
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Zone(fixtures::BF2)
            ]
        );
        assert!(damage_events(&ctx).is_empty(), "nothing before it resolves");
        both_pass(&mut ctx);
        assert_eq!(
            damage_events(&ctx),
            [(fixtures::SPRITE, 3), (BRUTE, 3), (RUNT, 3)],
            "Vi's 3 Might to each enemy there"
        );
        assert!(ctx.card(fixtures::SPRITE).is_none(), "3 on 3 is lethal");
        assert_eq!(ctx.card(RUNT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(BRUTE));
        assert_eq!(damage(&ctx, BRUTE), 3);
        assert_eq!(
            damage(&ctx, fixtures::THEIR_UNIT),
            0,
            "the base is untouched"
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::VI,
            from: Some(Location::Base(0)),
            to: Location::Battlefield(fixtures::BF2),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        let last_damage = ctx
            .effects
            .iter()
            .rposition(|effect| {
                matches!(effect, Effect::Counter { counter, .. } if *counter == COUNTER_DAMAGE)
            })
            .expect("damage was dealt");
        let moved = ctx
            .effects
            .iter()
            .position(|effect| {
                matches!(effect, Effect::Move { card, zone, .. }
                    if *card == fixtures::VI && *zone == fixtures::BF2)
            })
            .expect("the unit moved");
        assert!(last_damage < moved, "the damage lands before the move");
        assert_eq!(
            ctx.blob.contester(fixtures::BF2),
            Some(0),
            "428 · arriving on the enemy's battlefield contests it"
        );
        let showdown = ctx.blob.showdown.as_ref().expect("a combat opened");
        assert_eq!(showdown.zone, fixtures::BF2);
        assert!(showdown.combat);
        assert!(matches!(showdown.stage, ShowdownStage::Open));
        assert!(ctx.is_attacker(fixtures::VI));
        assert!(
            ctx.is_defender(BRUTE),
            "the survivor defends; the dead were cleaned up first"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} takes 3 damage".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} moves to {zone 10}".to_string()));
        assert_eq!(ctx.card(STORM).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn an_empty_or_friendly_battlefield_takes_no_damage_and_the_unit_still_moves() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STORM).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        both_pass(&mut ctx);
        assert!(damage_events(&ctx).is_empty(), "the Ally is no enemy");
        assert_eq!(damage(&ctx, ALLY), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"no enemy unit stands at {zone 9}".to_string()));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.blob.staged.is_empty() && ctx.blob.showdown.is_none(),
            "seat 0 already holds it"
        );
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_unit_that_left_the_base_before_resolution_strikes_nothing_and_moves_nowhere() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STORM).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::TRASH, 0), 0)
            .unwrap();
        both_pass(&mut ctx);
        assert!(damage_events(&ctx).is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert!(ctx.on_board(fixtures::SPRITE) && ctx.on_board(BRUTE) && ctx.on_board(RUNT));
        assert_eq!(ctx.card(STORM).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn units_away_from_the_base_and_enemies_are_refused_and_the_spell_is_the_turn_players() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STORM)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, STORM).unwrap();
        for wrong in [fixtures::THEIR_UNIT, BRUTE, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit in the base"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[u32::from(fixtures::BASE)]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the base is no battlefield"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(STORM).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(damage_events(&ctx).is_empty());
        assert!(ctx.blob.is_neutral_open());
        drop(ctx);

        let mut poor = armed();
        poor.table
            .cards
            .retain(|card| ![FURY_D, FURY_E].contains(&card.id));
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, STORM)),
            Err(Refusal::NotEnoughRunes {
                needed: 6,
                ready: 5
            }),
            "five ready runes cannot pay six energy"
        );
    }
}
