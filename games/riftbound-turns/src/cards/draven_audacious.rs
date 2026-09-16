use super::prelude::{
    deathknell, done, in_combat, once_each_turn, score_point, seat_target, target, triggered, unit,
    when,
};
use super::{
    Card, Event, Filter, Flow, Item, Keyword, Source, Stage, TargetKind, TargetSpec, Trigger, Who,
};
use crate::engine::ctx::Ctx;

pub const AN_OPPONENT: TargetSpec = target(
    Filter::Enemy,
    1,
    1,
    TargetKind::Seat,
    "an opponent who scores 1 point",
);

fn dying_in_combat(ctx: &Ctx, _: &Event, source: Source) -> bool {
    in_combat(ctx, source.card)
}

fn showboat(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    score_point(ctx, item.controller);
    done()
}

fn spite(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(seat) = seat_target(item, 0) {
        score_point(ctx, seat);
    }
    done()
}

pub static CARD: Card = unit(
    "Draven - Audacious",
    &[Keyword::Deflect(1)],
    &[
        once_each_turn(triggered(Trigger::CombatWon(Who::Me), &[], showboat)),
        when(deathknell(&[AN_OPPONENT], spite), dying_in_combat),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Once;
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle, showdown};
    use crate::state::{ItemKind, TargetRef, FLAG_ONCE_USED};
    use agni_plugin_sdk::table::CardInfo;

    const DRAVEN: u32 = 90;
    const DEFENDER: u32 = 91;

    fn draven(zone: u16, might: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(DRAVEN, zone, 0, "Draven - Audacious", might)
        }
    }

    fn against(defender_might: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(draven(fixtures::BF1, 6));
        fixture.table.cards.push(fixtures::unit(
            DEFENDER,
            fixtures::BF1,
            1,
            "Defender",
            defender_might,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn fight(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        let open = ctx.blob.showdown.clone().expect("a combat opens");
        assert!(open.combat);
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_deflect_a_once_a_turn_combat_win_and_a_conditional_deathknell() {
        assert!(std::ptr::eq(
            script_of("Draven - Audacious").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Draven - Audacious");
        assert_eq!(CARD.keywords, [Keyword::Deflect(1)]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let win = &CARD.abilities[0];
        assert_eq!(win.trigger, Trigger::CombatWon(Who::Me));
        assert_eq!(win.once, Once::PerTurn, "the first time each turn");
        assert!(win.targets.is_empty());
        assert!(win.condition.is_none());
        let death = &CARD.abilities[1];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.condition.is_some(), "in combat");
        assert_eq!(death.targets, [AN_OPPONENT]);
        assert_eq!(AN_OPPONENT.kind, TargetKind::Seat);
        assert_eq!((AN_OPPONENT.min, AN_OPPONENT.max), (1, 1));
    }

    #[test]
    fn winning_a_combat_scores_a_point_beside_the_conquer_when_the_trigger_resolves() {
        let mut fixture = against(2);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(ctx.card(DEFENDER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::CombatWon { zone, seat: 0 } if *zone == fixtures::BF1
        )));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.points(0), 1, "the conquer; the trigger waits");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DRAVEN
        ));
        assert!(ctx.blob.prompt.is_none(), "the win asks nothing");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 2);
        assert_eq!(ctx.points(1), 0);
        assert!(ctx.has_flag(DRAVEN, FLAG_ONCE_USED));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_second_win_in_the_same_turn_scores_only_its_conquer() {
        let mut fixture = against(2);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        resolve_chain(&mut ctx);
        assert_eq!(ctx.points(0), 2);
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat: 0,
        });
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the first time each turn has passed"
        );
        assert_eq!(ctx.points(0), 2);
    }

    #[test]
    fn dying_in_combat_hands_the_opponent_a_point_when_the_deathknell_resolves() {
        let mut fixture = against(7);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            ctx.card(DRAVEN).unwrap().zone,
            Some(fixtures::TRASH),
            "seven on six is lethal"
        );
        assert_eq!(ctx.card(DEFENDER).unwrap().zone, Some(fixtures::BF1));
        assert_eq!(ctx.points(0), 0);
        assert_eq!(ctx.blob.chain.len(), 1, "the deathknell alone: he lost");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == DRAVEN
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Seat(1)],
            "the lone opponent is chosen without a click"
        );
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.points(1), 0, "the point waits for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(1), 1);
        assert_eq!(ctx.points(0), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} scores 1 point".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_death_outside_combat_scores_nothing_for_anyone() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(draven(fixtures::BF1, 6));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!in_combat(&ctx, DRAVEN));
        ctx.kill(DRAVEN, Cause::Item(9));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(DRAVEN).unwrap().zone, Some(fixtures::TRASH));
        assert!(
            ctx.blob.chain.is_empty(),
            "a kill outside combat is not a death in combat"
        );
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.points(1), 0);
        assert_eq!(ctx.points(0), 0);
    }
}
