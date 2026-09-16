use super::prelude::{
    at_battlefield, bounce, card_target, done, location_of, move_unit, on_hold_me, optional, play,
    target, unit, when,
};
use super::{Card, Event, Filter, Flow, Item, Keyword, Source, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ENEMY_UNIT_ELSEWHERE: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::Not(&Filter::Here)]);

pub const GRAB: TargetSpec = target(
    ENEMY_UNIT_ELSEWHERE,
    0,
    1,
    TargetKind::Card,
    "an enemy unit to pull here",
);

fn played_to_a_battlefield(ctx: &Ctx, _: &Event, source: Source) -> bool {
    at_battlefield(ctx, source.card)
}

fn rocket_grab(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let Some(here) = location_of(ctx, me).filter(|at| at.battlefield().is_some()) else {
        ctx.narrate(format!(
            "{{card {me}}} is no longer at a battlefield · nothing is pulled"
        ));
        return done();
    };
    move_unit(ctx, item, unit, here);
    done()
}

fn power_fist_home(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if bounce(ctx, me) {
        ctx.narrate(format!("{{card {me}}} returns to his owner's hand"));
    }
    done()
}

pub static CARD: Card = unit(
    "Blitzcrank - Impassive",
    &[Keyword::Tank],
    &[
        when(
            optional(play(&[GRAB], rocket_grab)),
            played_to_a_battlefield,
        ),
        on_hold_me(&[], power_fist_home),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::{Event, Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cleanup, play as play_engine, priority, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BLITZ: u32 = 90;
    const CALM_RUNE: u32 = 46;

    fn blitz(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(BLITZ, zone, seat, "Blitzcrank - Impassive", 5)
        }
    }

    fn in_hand() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(blitz(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn deploy(ctx: &mut Ctx, to: Location) {
        play_engine::begin(ctx, 0, BLITZ, Origin::Hand, Some(to)).unwrap();
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn target_item(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play asks for an enemy unit, not {other:?}"),
        }
    }

    #[test]
    fn the_script_prints_tank_a_conditional_optional_play_pull_and_a_hold_return() {
        assert!(std::ptr::eq(
            script_of("Blitzcrank - Impassive").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Blitzcrank - Impassive");
        assert_eq!(CARD.keywords, [Keyword::Tank]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let pull = &CARD.abilities[0];
        assert_eq!(pull.trigger, Trigger::Play);
        assert!(pull.optional, "you may move");
        assert!(
            pull.condition.is_some(),
            "only when played to a battlefield"
        );
        assert!(pull.cost.is_none());
        assert_eq!(pull.targets.len(), 1);
        let spec = &pull.targets[0];
        assert_eq!(spec.filter, ENEMY_UNIT_ELSEWHERE);
        assert_eq!((spec.min, spec.max), (0, 1), "the may is a 0-of-1 target");
        assert_eq!(spec.kind, TargetKind::Card);
        let hold = &CARD.abilities[1];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(!hold.optional);
        assert!(hold.targets.is_empty());
        assert!(hold.condition.is_none());
    }

    #[test]
    fn played_to_a_battlefield_he_offers_enemy_units_elsewhere_and_pulls_the_pick_here() {
        let mut fixture = in_hand();
        let mut ctx = fixture.ctx();
        deploy(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert_eq!(
            ctx.location(BLITZ),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let item = target_item(&ctx);
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                "skip".to_string()
            ],
            "the Sprite at {{zone 10}} and Jinx in her base; Vi is friendly"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {BLITZ}}}: choose an enemy unit to pull here (0 of 1)")
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is refused"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BLITZ
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Base(1)),
            "the pull waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, from: Some(Location::Base(1)), to: Location::Battlefield(zone), cause: MoveCause::Effect, .. }
                if *card == fixtures::THEIR_UNIT && *zone == fixtures::BF1
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} moves to {{zone {}}}",
            fixtures::THEIR_UNIT,
            fixtures::BF1
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_the_pull_moves_nothing() {
        let mut fixture = in_hand();
        let mut ctx = fixture.ctx();
        deploy(&mut ctx, Location::Battlefield(fixtures::BF1));
        target_item(&ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the trigger still resolves, with no target"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
    }

    #[test]
    fn played_to_the_base_he_asks_nothing_and_pulls_nothing() {
        let mut fixture = in_hand();
        let mut ctx = fixture.ctx();
        deploy(&mut ctx, Location::Base(0));
        assert_eq!(ctx.location(BLITZ), Some(Location::Base(0)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == BLITZ
        )));
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(ctx.blob.queue.is_empty());
        assert!(
            ctx.blob.chain.is_empty(),
            "the condition refuses the trigger"
        );
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
    }

    #[test]
    fn holding_with_him_returns_him_to_his_owners_hand_when_the_trigger_resolves() {
        let mut fixture = in_hand();
        fixture.table.card_mut(BLITZ).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == BLITZ
        ));
        assert!(ctx.blob.prompt.is_none(), "the hold asks nothing");
        assert_eq!(
            ctx.location(BLITZ),
            Some(Location::Battlefield(fixtures::BF1)),
            "the return waits for the chain"
        );
        assert_eq!(ctx.points(0), 1);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BLITZ).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(BLITZ).unwrap().seat, 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BLITZ}}} returns to his owner's hand")));
        assert_eq!(ctx.points(0), 1, "the hold point stays");
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "cleanup finds the battlefield empty and control lapses"
        );
    }

    #[test]
    fn a_conquer_with_him_is_not_a_hold() {
        let mut fixture = in_hand();
        fixture.table.card_mut(BLITZ).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(BLITZ),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }
}
