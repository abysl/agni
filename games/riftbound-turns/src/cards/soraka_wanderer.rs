use super::prelude::{exhaust, heal, recall, replaces, unit, with_replacement};
use super::{Card, Keyword, Source, WouldDie};
use crate::engine::ctx::Ctx;

pub fn a_weaker_friendly_unit_here_would_die(ctx: &Ctx, would: &WouldDie, source: Source) -> bool {
    let me = source.card;
    let unit = would.unit;
    unit != me
        && ctx.is_unit(unit)
        && ctx.on_board(unit)
        && ctx.on_board(me)
        && ctx.controller(unit) == ctx.controller(me)
        && ctx.location(unit).is_some()
        && ctx.location(unit) == ctx.location(me)
        && ctx.current_might(unit) < ctx.current_might(me)
}

pub fn heal_exhaust_and_recall_instead(ctx: &mut Ctx, would: &WouldDie, source: Source) {
    let unit = would.unit;
    heal(ctx, unit);
    exhaust(ctx, unit);
    recall(ctx, unit, true);
    ctx.narrate(format!(
        "{{card {}}} sends {{card {unit}}} home healed and exhausted instead of dying",
        source.card
    ));
}

pub static CARD: Card = with_replacement(
    unit("Soraka - Wanderer", &[Keyword::Backline], &[]),
    replaces(
        a_weaker_friendly_unit_here_would_die,
        heal_exhaust_and_recall_instead,
    ),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{deathknell, done, draw, Location};
    use crate::cards::{script_of, Flow, Item, Stage};
    use crate::engine::ctx::{Cause, Event, Killed, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{combat, kill, settle};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const SORAKA: u32 = 90;
    const WARD: u32 = 91;
    const BRUTE: u32 = 92;
    const FAR: u32 = 93;

    fn wail(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
        draw(ctx, item.controller, 1);
        done()
    }

    static WAILER: Card = unit("Wailer", &[Keyword::Deathknell], &[deathknell(&[], wail)]);

    fn soraka(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(SORAKA, zone, 0, "Soraka - Wanderer", 4)
        }
    }

    fn grove() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(soraka(fixtures::BF1));
        fixture
            .table
            .cards
            .push(fixtures::unit(WARD, fixtures::BF1, 0, "Wailer", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 0, "Brute", 6));
        fixture
            .table
            .cards
            .push(fixtures::unit(FAR, fixtures::BASE, 0, "Wailer", 1));
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(WARD),
            counter: COUNTER_DAMAGE,
            value: 2,
        });
        fixture.table.counters.sort();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(WARD, &WAILER)
            .with_script(FAR, &WAILER);
        fixture
    }

    fn would(unit: u32) -> WouldDie {
        WouldDie {
            unit,
            cause: Cause::Rule,
        }
    }

    fn source() -> Source {
        Source {
            card: SORAKA,
            ability: 0,
        }
    }

    fn died(ctx: &Ctx, card: u32) -> bool {
        ctx.events
            .iter()
            .any(|event| matches!(event, Event::Died { card: dead, .. } if *dead == card))
    }

    #[test]
    fn the_script_is_backline_with_a_replacement_and_no_abilities() {
        assert!(std::ptr::eq(script_of("Soraka - Wanderer").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Backline]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_some());
        let mut fixture = grove();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(SORAKA, Keyword::Backline));
        assert_eq!(
            combat::ordered(&ctx, &[SORAKA, WARD], false),
            [WARD],
            "815 · she is assigned combat damage last"
        );
    }

    #[test]
    fn the_replacement_applies_to_a_weaker_friendly_unit_at_her_location_alone() {
        let mut fixture = grove();
        let ctx = fixture.ctx();
        assert!(a_weaker_friendly_unit_here_would_die(
            &ctx,
            &would(WARD),
            source()
        ));
        assert!(
            !a_weaker_friendly_unit_here_would_die(&ctx, &would(BRUTE), source()),
            "6 Might is not less than her 4"
        );
        assert!(
            !a_weaker_friendly_unit_here_would_die(&ctx, &would(FAR), source()),
            "the base is not here"
        );
        assert!(
            !a_weaker_friendly_unit_here_would_die(&ctx, &would(SORAKA), source()),
            "another unit, not herself"
        );
        assert!(
            !a_weaker_friendly_unit_here_would_die(&ctx, &would(fixtures::THEIR_UNIT), source()),
            "not a unit you control"
        );
        assert_eq!(
            kill::applicable(&ctx, &would(WARD)),
            [source()],
            "kill::applicable finds her"
        );
        assert!(kill::applicable(&ctx, &would(BRUTE)).is_empty());
    }

    #[test]
    fn a_weaker_ally_dying_beside_her_is_healed_exhausted_and_recalled_and_no_deathknell_fires() {
        let mut fixture = grove();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.damage_on(WARD), 2);
        assert_eq!(ctx.kill(WARD, Cause::Rule), Killed::Replaced);
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(WARD));
        assert_eq!(ctx.location(WARD), Some(Location::Base(0)));
        assert!(ctx.card(WARD).unwrap().exhausted);
        assert_eq!(ctx.damage_on(WARD), 0, "healed");
        assert!(!died(&ctx, WARD));
        assert!(ctx.blob.chain.is_empty(), "no death, no Deathknell");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SORAKA}}} replaces the death of {{card {WARD}}}"
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SORAKA}}} sends {{card {WARD}}} home healed and exhausted instead of dying"
        )));
        assert!(ctx.on_board(SORAKA), "she pays nothing for it");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_mightier_ally_a_far_ally_and_she_herself_die_as_usual() {
        let mut fixture = grove();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(BRUTE, Cause::Rule), Killed::Yes);
        assert!(died(&ctx, BRUTE));
        assert_eq!(ctx.kill(FAR, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(died(&ctx, FAR));
        assert_eq!(ctx.blob.chain.len(), 1, "the far Wailer's Deathknell");
        assert_eq!(ctx.kill(SORAKA, Cause::Rule), Killed::Yes);
        assert!(died(&ctx, SORAKA));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn once_she_is_gone_or_elsewhere_the_ward_dies() {
        let mut fixture = grove();
        fixture.table.card_mut(SORAKA).unwrap().zone = Some(fixtures::BF2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(WARD, Cause::Rule), Killed::Yes);
        assert!(died(&ctx, WARD));
        drop(ctx);

        let mut fixture = grove();
        fixture.table.cards.retain(|card| card.id != SORAKA);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(WARD, Cause::Rule), Killed::Yes);
        assert!(died(&ctx, WARD));
    }

    #[test]
    fn she_and_a_weaker_ally_dying_together_in_combat_still_send_the_ally_home() {
        let mut fixture = grove();
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(SORAKA),
            counter: COUNTER_DAMAGE,
            value: 4,
        });
        fixture.table.counters.sort();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.damage_on(SORAKA), 4);
        assert_eq!(ctx.damage_on(WARD), 2);
        let batch = crate::engine::cleanup::dying(&ctx);
        assert_eq!(batch, [SORAKA, WARD]);
        let dead = kill::batch(&mut ctx, &batch, Cause::Cleanup { last_item: None });
        assert_eq!(
            dead,
            [SORAKA],
            "she dies; the ward she stood beside is replaced"
        );
        assert!(ctx.on_board(WARD));
        assert_eq!(ctx.location(WARD), Some(Location::Base(0)));
        let at = |line: &str| ctx.blob.log.iter().position(|held| held == line);
        let replaced = at(&format!(
            "{{card {SORAKA}}} replaces the death of {{card {WARD}}}"
        ));
        assert!(replaced.is_some(), "{:?}", ctx.blob.log);
        let died =
            ctx.blob.log.iter().position(|line| {
                line.contains(&format!("{{card {SORAKA}}}")) && line.contains("dies")
            });
        assert!(
            died.is_none_or(|died| replaced.unwrap() < died),
            "373.1.a · the replacement runs before the unmodified death: {:?}",
            ctx.blob.log
        );
    }
}
