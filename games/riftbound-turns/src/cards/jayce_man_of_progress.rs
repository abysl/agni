use super::prelude::{
    card_target, done, kill, optional, play, promise_this_turn, target, this_turn, unit, Promise,
    PromiseEffect, PromiseKind,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::cost;
use crate::engine::ctx::{Ctx, Killed};
use crate::state::Origin;

pub const MAX_ENERGY: u8 = 7;
pub const FRIENDLY_GEAR: Filter = Filter::And(&[Filter::Gear, Filter::Friendly]);
pub const GEAR_TO_KILL: TargetSpec = target(
    FRIENDLY_GEAR,
    0,
    1,
    TargetKind::Card,
    "a friendly gear to kill",
);

pub fn progress_promise(ctx: &Ctx) -> Promise {
    Promise {
        kind: PromiseKind::Gear,
        effect: PromiseEffect::FreeForPower {
            max_energy: MAX_ENERGY,
        },
        until: this_turn(ctx),
    }
}

pub fn covered_by_the_promise(ctx: &Ctx, seat: u8, card: u32) -> bool {
    ctx.in_hand(card)
        && ctx.controller(card) == seat
        && cost::promise_covers(
            ctx,
            &progress_promise(ctx),
            &cost::play_item(ctx, seat, card, Origin::Hand),
        )
}

pub fn promise_a_gear_for_free_this_turn(ctx: &mut Ctx, seat: u8) {
    promise_this_turn(
        ctx,
        seat,
        PromiseKind::Gear,
        PromiseEffect::FreeForPower {
            max_energy: MAX_ENERGY,
        },
    );
    ctx.narrate(format!(
        "{{seat {seat}}} may play a gear costing {MAX_ENERGY} or less from hand this turn for its Power alone"
    ));
}

fn progress(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(gear) = card_target(ctx, item, 0) else {
        return done();
    };
    if kill(ctx, item, gear) != Killed::Yes {
        return done();
    }
    ctx.narrate(format!(
        "{{card {}}} kills {{card {gear}}}",
        item.kind.source()
    ));
    promise_a_gear_for_free_this_turn(ctx, item.controller);
    done()
}

pub static CARD: Card = unit(
    "Jayce - Man of Progress",
    &[],
    &[optional(play(&[GEAR_TO_KILL], progress))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const JAYCE: u32 = 90;
    const HAMMER: u32 = 91;
    const THEIR_HAMMER: u32 = 92;
    const BIG_GEAR: u32 = 93;
    const SMALL_GEAR: u32 = 94;

    fn jayce() -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Mind".into()],
            ..fixtures::unit(JAYCE, fixtures::HAND, 0, "Jayce - Man of Progress", 4)
        }
    }

    fn laboratory() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jayce());
        fixture
            .table
            .cards
            .push(fixtures::gear(HAMMER, fixtures::BASE, 0, "Hammer", 3));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_HAMMER, fixtures::BASE, 1, "Hammer", 3));
        fixture.table.cards.push(fixtures::gear(
            BIG_GEAR,
            fixtures::HAND,
            0,
            "Colossus Frame",
            8,
        ));
        fixture.table.cards.push(fixtures::gear(
            SMALL_GEAR,
            fixtures::HAND,
            0,
            "Mercury Hammer",
            7,
        ));
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_an_optional_play_trigger_over_a_friendly_gear() {
        assert!(std::ptr::eq(
            script_of("Jayce - Man of Progress").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.optional);
        assert_eq!(ability.targets, &[GEAR_TO_KILL]);
        assert_eq!((GEAR_TO_KILL.min, GEAR_TO_KILL.max), (0, 1));
        assert_eq!(MAX_ENERGY, 7);
    }

    #[test]
    fn the_promise_covers_a_gear_in_your_hand_costing_seven_or_less() {
        let mut fixture = laboratory();
        let ctx = fixture.ctx();
        assert!(covered_by_the_promise(&ctx, 0, SMALL_GEAR));
        assert!(covered_by_the_promise(&ctx, 0, fixtures::HAND_GEAR));
        assert!(
            !covered_by_the_promise(&ctx, 0, BIG_GEAR),
            "eight energy is over the line"
        );
        assert!(
            !covered_by_the_promise(&ctx, 0, HAMMER),
            "on the board, not in hand"
        );
        assert!(
            !covered_by_the_promise(&ctx, 0, fixtures::HAND_UNIT),
            "a unit is not a gear"
        );
        assert!(
            !covered_by_the_promise(&ctx, 1, SMALL_GEAR),
            "not the opponent's promise"
        );
    }

    #[test]
    fn playing_him_offers_a_friendly_gear_to_kill_and_the_kill_lands_when_the_trigger_resolves() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JAYCE).unwrap();
        assert!(ctx.on_board(JAYCE));
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {HAMMER}}}"), "skip".to_string()],
            "your gear on the board, not the opponent's nor the ones in hand"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {HAMMER}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == JAYCE
        ));
        assert!(ctx.on_board(HAMMER), "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(HAMMER));
        assert_eq!(ctx.card(HAMMER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: false, .. } if *card == HAMMER
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {JAYCE}}} kills {{card {HAMMER}}}")));
        assert!(ctx.blob.log.contains(
            &"{seat 0} may play a gear costing 7 or less from hand this turn for its Power alone"
                .to_string()
        ));
        assert_eq!(ctx.blob.seat(0).promises, [progress_promise(&ctx)]);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_the_kill_promises_nothing_and_without_a_gear_he_asks_nothing() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JAYCE).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(HAMMER));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("may play a gear")));
        assert!(ctx.blob.seat(0).promises.is_empty());
        drop(ctx);

        let mut fixture = laboratory();
        fixture.table.cards.retain(|card| card.id != HAMMER);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JAYCE).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", fixtures::labels(&ctx));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(THEIR_HAMMER));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn after_the_kill_a_seven_energy_gear_is_playable_for_its_power_alone_and_an_eight_is_not() {
        let mut fixture = laboratory();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JAYCE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {HAMMER}}}")).unwrap();
        resolve_chain(&mut ctx);
        let ready = ctx.ready_runes_of(0).len();
        assert!(fixtures::play_from_hand(&mut ctx, 0, BIG_GEAR).is_err());
        fixtures::play_from_hand(&mut ctx, 0, SMALL_GEAR).unwrap();
        assert!(ctx.on_board(SMALL_GEAR));
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "no energy paid");
    }
}
