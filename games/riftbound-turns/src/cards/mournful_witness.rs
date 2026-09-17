use super::prelude::{done, is_empowered, triggered, unit, with_statics};
use super::{Card, Flow, Grant, Item, Stage, Static, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 2;

pub fn empower_me_after_my_combat(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    if ctx.empower_by(me, item.controller) {
        ctx.narrate(format!("{{card {me}}} becomes Empowered"));
    } else {
        ctx.narrate(format!("{{card {me}}} is already Empowered"));
    }
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Mournful Witness",
        &[],
        &[triggered(
            Trigger::CombatEnded(Who::Me),
            &[],
            empower_me_after_my_combat,
        )],
    ),
    &[Static::While(is_empowered, &[Grant::Might(BONUS)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cleanup;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{ChainItem, ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const WITNESS: u32 = 90;
    const MIGHT: u8 = 2;

    fn witness() -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Calm".into()],
            ..fixtures::unit(WITNESS, fixtures::BF1, 0, "Mournful Witness", MIGHT)
        }
    }

    fn wake() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(witness());
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WITNESS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn combat_ended() -> ChainItem {
        ChainItem::new(
            1,
            ItemKind::Trigger {
                source: WITNESS,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_script_empowers_after_its_combat_and_carries_the_empowered_might() {
        assert!(std::ptr::eq(script_of("Mournful Witness").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::CombatEnded(Who::Me));
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(
            CARD.statics[0],
            Static::While(_, [Grant::Might(BONUS)])
        ));
        assert_eq!(BONUS, 2);
    }

    #[test]
    fn empowered_it_has_two_more_might_and_nothing_more_before() {
        let mut fixture = wake();
        let mut ctx = fixture.ctx();
        assert!(!ctx.is_empowered(WITNESS));
        assert_eq!(ctx.current_might(WITNESS), i32::from(MIGHT));
        assert!(ctx.empower(WITNESS));
        assert_eq!(
            ctx.current_might(WITNESS),
            i32::from(MIGHT) + 2,
            "828.1.c · the Empowered ability is active"
        );
        assert!(ctx.disempower(WITNESS));
        assert_eq!(ctx.current_might(WITNESS), i32::from(MIGHT));
    }

    #[test]
    fn the_run_empowers_the_witness_once_and_leaves_one_that_left_the_board_alone() {
        let mut fixture = wake();
        let mut ctx = fixture.ctx();
        assert_eq!(
            empower_me_after_my_combat(&mut ctx, &combat_ended(), Stage(0)),
            Flow::Done
        );
        assert!(ctx.is_empowered(WITNESS));
        assert_eq!(ctx.current_might(WITNESS), i32::from(MIGHT) + 2);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Empowered { card, .. } if *card == WITNESS))
                .count(),
            1
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {WITNESS}}} becomes Empowered")));
        empower_me_after_my_combat(&mut ctx, &combat_ended(), Stage(0));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Empowered { .. }))
                .count(),
            1,
            "I become Empowered if I'm not already"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {WITNESS}}} is already Empowered")));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut gone = wake();
        gone.table.card_mut(WITNESS).unwrap().zone = Some(fixtures::TRASH);
        gone.resolve();
        let mut ctx = gone.ctx();
        empower_me_after_my_combat(&mut ctx, &combat_ended(), Stage(0));
        assert!(!ctx.is_empowered(WITNESS));
        assert!(ctx.effects.is_empty());
    }

    #[test]
    fn a_combat_the_witness_was_in_ending_empowers_it_whatever_the_result() {
        let mut fixture = wake();
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(WITNESS));
        assert!(ctx.mark_defender(fixtures::THEIR_UNIT));
        cleanup::after_combat(&mut ctx, fixtures::BF1, 0);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the combat-ended trigger");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, .. } if source == WITNESS
        ));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_empowered(WITNESS));
        assert_eq!(ctx.current_might(WITNESS), i32::from(MIGHT) + 2);
    }
}
