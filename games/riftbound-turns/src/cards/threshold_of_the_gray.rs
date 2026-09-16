use super::prelude::{battlefield, done, Location, ONE_ENERGY};
use super::{Card, Cost, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const ADDS: Cost = ONE_ENERGY;

pub static CARD: Card = battlefield("Threshold of the Gray", &[], &[]);

pub fn combatants_here(ctx: &Ctx, threshold: u32) -> Option<(u8, u8)> {
    let showdown = ctx.blob.showdown.as_ref()?;
    if !showdown.combat || ctx.location(threshold) != Some(Location::Battlefield(showdown.zone)) {
        return None;
    }
    Some((showdown.attacker, showdown.defender))
}

pub fn each_side_adds_one(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let threshold = item.kind.source();
    let Some((attacker, defender)) = combatants_here(ctx, threshold) else {
        ctx.narrate(format!("no combat is open at {{card {threshold}}}"));
        return done();
    };
    ctx.add_to_pool(attacker, threshold, &ADDS);
    ctx.add_to_pool(defender, threshold, &ADDS);
    done()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::triggered;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::cost;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, settle};
    use crate::state::{ItemKind, Pool};

    const THRESHOLD: u32 = fixtures::GROUNDS;
    const RAIDER: u32 = 90;

    static WIRED_TO_THE_FIRST_DEFENDER_FOR_THE_TEST: Card = battlefield(
        "Threshold of the Gray",
        &[],
        &[triggered(
            Trigger::Defends(Who::You),
            &[],
            each_side_adds_one,
        )],
    );

    fn threshold() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(THRESHOLD).unwrap().name = "Threshold of the Gray".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(THRESHOLD).unwrap(),
            &CARD
        ));
        fixture
    }

    fn wired() -> Fixture {
        let mut fixture = threshold();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(THRESHOLD, &WIRED_TO_THE_FIRST_DEFENDER_FOR_THE_TEST);
        fixture
    }

    fn open_combat(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        settle(ctx).unwrap();
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
    }

    fn threshold_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == THRESHOLD => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_stub_is_the_pool_name_and_the_combat_start_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(
            script_of("Threshold of the Gray").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(ADDS, ONE_ENERGY);
        let wired = &WIRED_TO_THE_FIRST_DEFENDER_FOR_THE_TEST.abilities[0];
        assert_eq!(wired.trigger, Trigger::Defends(Who::You));
        assert!(!wired.optional);
        assert!(wired.cost.is_none());
    }

    #[test]
    fn combatants_are_read_off_an_open_combat_here_and_nowhere_else() {
        let mut fixture = threshold();
        let mut ctx = fixture.ctx();
        assert_eq!(combatants_here(&ctx, THRESHOLD), None, "nothing is open");
        open_combat(&mut ctx);
        assert_eq!(combatants_here(&ctx, THRESHOLD), Some((1, 0)));
        assert_eq!(
            combatants_here(&ctx, fixtures::ROCKFALL),
            None,
            "the other battlefield hosts no combat"
        );
        assert!(
            threshold_items(&ctx).is_empty(),
            "the stub carries no trigger"
        );
        assert!(ctx.blob.seat(0).pool.is_empty());
        assert!(ctx.blob.seat(1).pool.is_empty());
        drop(ctx);
        let mut fixture = threshold();
        fixture.table.card_mut(RAIDER).unwrap().zone = Some(fixtures::BASE);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF2);
        fixture.blob.set_contested(fixtures::BF2, Some(1));
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(ctx
            .blob
            .showdown
            .as_ref()
            .is_some_and(|held| !held.combat && held.zone == fixtures::BF2));
        assert_eq!(
            combatants_here(&ctx, THRESHOLD),
            None,
            "a showdown without combat, elsewhere"
        );
    }

    #[test]
    fn wired_to_the_defender_the_run_banks_one_energy_for_the_attacker_and_the_defender() {
        let mut fixture = wired();
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert_eq!(threshold_items(&ctx), [0], "the holder's unit defends once");
        assert!(ctx.blob.seat(0).pool.is_empty(), "the add waits");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.blob.showdown.is_some(),
            "the combat is still open: the add lands during the showdown"
        );
        let one_energy = Pool {
            energy: 1,
            power: Vec::new(),
        };
        assert_eq!(ctx.blob.seat(1).pool, one_energy, "the attacker");
        assert_eq!(ctx.blob.seat(0).pool, one_energy, "the defender");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} adds 1 energy to their rune pool".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} adds 1 energy to their rune pool".to_string()));
        let spark = cost::total(&ctx, fixtures::HAND_SPELL, false);
        assert_eq!(spark.energy, 1, "Spark costs 2: the add pays one");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_run_adds_nothing_when_no_combat_is_open_here() {
        let mut fixture = wired();
        let mut ctx = fixture.ctx();
        let item = crate::state::ChainItem::new(
            7,
            ItemKind::Trigger {
                source: THRESHOLD,
                index: 0,
            },
            0,
            crate::state::Origin::Board,
        );
        assert_eq!(each_side_adds_one(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(ctx.blob.seat(0).pool.is_empty());
        assert!(ctx.blob.seat(1).pool.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("no combat is open at {{card {THRESHOLD}}}")));
    }

    #[test]
    #[ignore = "engine gap · no trigger subject for combat starting: Trigger has no ShowdownBegins arm and showdown::open raises no event as a combat opens (464.2.b), so the threshold's trigger never fires; the engine owes Trigger::ShowdownBegins(Where::Here) raised with the zone and both seats as the combat opens (the Diana - Lunari row), wired as triggered(ShowdownBegins(Here), &[], each_side_adds_one), and each_side_adds_one then adds one energy to each side's rune pool (167) through Ctx::add_to_pool"]
    fn combat_starting_here_adds_one_energy_to_each_side() {
        let mut fixture = threshold();
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert_eq!(threshold_items(&ctx), [0]);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.seat(0).pool.energy, 1);
        assert_eq!(ctx.blob.seat(1).pool.energy, 1);
    }
}
