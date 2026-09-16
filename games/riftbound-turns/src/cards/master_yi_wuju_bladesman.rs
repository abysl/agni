use super::prelude::{legend, with_statics};
use super::{Card, Grant, Scope, Static};
use crate::engine::statics::defends_alone;

pub const BONUS: i16 = 2;

pub static CARD: Card = with_statics(
    legend("Master Yi - Wuju Bladesman", &[], &[]),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: defends_alone,
        grants: &[Grant::Might(BONUS)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, Location, MoveCause, Moved, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, showdown, statics};
    use crate::state::{GameBlob, Mode};
    use agni_plugin_sdk::table::Target;

    const RAIDER: u32 = 90;
    const ALLY: u32 = 91;

    fn dojo() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.card_mut(fixtures::LEGEND_CARD).unwrap().name =
            "Master Yi - Wuju Bladesman".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(fixtures::LEGEND_CARD).unwrap(),
            &CARD
        ));
        fixture
    }

    fn raided(might: u8) -> Fixture {
        let mut fixture = dojo();
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", might));
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn damage_of(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn fight(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        for _ in 0..4 {
            let Some(held) = ctx.blob.showdown.clone() else {
                break;
            };
            showdown::pass(ctx, held.focus()).unwrap();
        }
        assert!(ctx.blob.showdown.is_none());
    }

    #[test]
    fn the_script_is_a_legend_whose_aura_reaches_friendly_units_defending_alone() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Master Yi - Wuju Bladesman").unwrap(),
            &CARD
        ));
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Might(2)],
                ..
            }]
        ));
    }

    #[test]
    fn a_lone_friendly_defender_reads_plus_two_and_loses_it_when_a_second_friendly_unit_arrives() {
        let mut fixture = dojo();
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 3, "not defending");
        assert!(ctx.mark_defender(fixtures::VI));
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::Might(2)]
        ));
        assert_eq!(ctx.current_might(fixtures::VI), 5, "defending alone");
        assert_eq!(
            ctx.move_unit(
                ALLY,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "466.7.a · a second friendly unit here ends alone"
        );
        assert!(ctx.mark_defender(ALLY));
        assert_eq!(ctx.current_might(ALLY), 2);
        ctx.recall(ALLY, false);
        assert_eq!(ctx.current_might(fixtures::VI), 5, "alone again");
        ctx.clear_designation(fixtures::VI);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the designation clearing ends it"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_bonus_is_read_by_the_damage_step_so_a_lone_defender_kills_a_bigger_raider() {
        let mut fixture = raided(4);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 4 might vs defenders 5 might"));
        assert!(
            !ctx.on_board(RAIDER),
            "4 damage is lethal to a 4-Might raider"
        );
        assert!(
            ctx.on_board(fixtures::VI),
            "465.2 · 4 damage is not lethal at 5 Might"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(damage_of(&ctx, fixtures::VI), 0, "healed after the combat");
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the designation is gone with the combat"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_aura_reaches_neither_a_unit_at_base_nor_an_attacker_nor_the_other_seats_units() {
        let mut fixture = raided(3);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(ctx.is_defender(fixtures::VI));
        assert!(ctx.is_attacker(RAIDER));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(
            ctx.current_might(RAIDER),
            3,
            "an attacker, and not friendly"
        );
        assert_eq!(
            ctx.current_might(ALLY),
            2,
            "a unit at base is not defending"
        );
        assert!(statics::grants_on(&ctx, ALLY).is_empty());
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "an ally at base does not end alone"
        );
        drop(ctx);

        let mut fixture = dojo();
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF2, 1, "Raider", 3));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(ctx.is_attacker(fixtures::VI));
        assert!(ctx.is_defender(RAIDER));
        assert_eq!(
            ctx.current_might(RAIDER),
            3,
            "the other seat's lone defender gets nothing from our legend"
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "our attacker gets nothing either"
        );
    }
}
