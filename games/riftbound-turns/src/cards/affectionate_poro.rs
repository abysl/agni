use super::prelude::{done, draw, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const CARDS: usize = 1;

pub fn undamaged_this_turn(ctx: &Ctx, me: u32) -> bool {
    ctx.damage_on(me) == 0
}

pub fn draw_if_undamaged_after_my_combat(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    if !ctx.on_board(me) {
        return done();
    }
    if !undamaged_this_turn(ctx, me) {
        ctx.narrate(format!(
            "{{card {me}}} was dealt damage this turn · no card"
        ));
        return done();
    }
    if draw(ctx, seat, CARDS) > 0 {
        ctx.narrate(format!("{{card {me}}} lets {{seat {seat}}} draw {CARDS}"));
    }
    done()
}

pub static CARD: Card = unit("Affectionate Poro", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cleanup;
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{ChainItem, ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const PORO: u32 = 90;

    fn poro() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Calm".into()],
            ..fixtures::unit(PORO, fixtures::BF1, 0, "Affectionate Poro", 3)
        }
    }

    fn snowfield() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poro());
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PORO).unwrap(), &CARD));
        fixture
    }

    fn combat_ended() -> ChainItem {
        ChainItem::new(
            1,
            ItemKind::Trigger {
                source: PORO,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_script_stays_a_stub_until_the_engine_raises_a_combat_ended_event_with_its_units() {
        assert!(std::ptr::eq(script_of("Affectionate Poro").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARDS, 1);
    }

    #[test]
    fn the_run_draws_one_for_an_undamaged_poro_and_nothing_for_a_wounded_one() {
        let mut fixture = snowfield();
        let mut ctx = fixture.ctx();
        assert!(undamaged_this_turn(&ctx, PORO));
        let hand = ctx.hand_of(0).len();
        assert_eq!(
            draw_if_undamaged_after_my_combat(&mut ctx, &combat_ended(), Stage(0)),
            Flow::Done
        );
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PORO}}} lets {{seat 0}} draw 1")));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = snowfield();
        let mut ctx = fixture.ctx();
        assert!(ctx.damage(PORO, 1, Cause::Rule));
        assert!(!undamaged_this_turn(&ctx, PORO));
        let hand = ctx.hand_of(0).len();
        draw_if_undamaged_after_my_combat(&mut ctx, &combat_ended(), Stage(0));
        assert_eq!(ctx.hand_of(0).len(), hand, "wounded · no card");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {PORO}}} was dealt damage this turn · no card"
        )));
        drop(ctx);

        let mut gone = snowfield();
        gone.table.card_mut(PORO).unwrap().zone = Some(fixtures::TRASH);
        gone.resolve();
        let mut ctx = gone.ctx();
        let hand = ctx.hand_of(0).len();
        draw_if_undamaged_after_my_combat(&mut ctx, &combat_ended(), Stage(0));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "a Poro that died in the combat draws nothing"
        );
    }

    #[test]
    #[ignore = "engine gap · missing triggers: cleanup::after_combat raises CombatWon / CombatLost with the zone and the seat only and Trigger has no CombatEnded(Who); when a combat the Poro was in ends it must trigger as triggered(CombatEnded(Who::Me), &[], draw_if_undamaged_after_my_combat) with the Poro as the subject, and the damage it read must be a per-turn record: after_combat heals every unit before the trigger resolves, so damage_on reads 0 for a Poro that was hit"]
    fn a_combat_the_poro_was_in_ending_draws_a_card_unless_it_was_hit() {
        let mut fixture = snowfield();
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(PORO));
        assert!(ctx.mark_defender(fixtures::THEIR_UNIT));
        let hand = ctx.hand_of(0).len();
        cleanup::after_combat(&mut ctx, fixtures::BF1, 0);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the combat-ended trigger");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, .. } if source == PORO
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        drop(ctx);

        let mut hit = snowfield();
        let mut ctx = hit.ctx();
        assert!(ctx.mark_attacker(PORO));
        assert!(ctx.mark_defender(fixtures::THEIR_UNIT));
        assert!(ctx.damage(PORO, 1, Cause::Combat));
        let hand = ctx.hand_of(0).len();
        cleanup::after_combat(&mut ctx, fixtures::BF1, 0);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "dealt damage this turn · no card"
        );
    }
}
