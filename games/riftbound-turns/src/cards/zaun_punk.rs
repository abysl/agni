use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{a_card, card_target, done, friendly_gear, kill, play, unit, when, GEAR};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::{Cause, Ctx, Killed};

pub const KILLS: usize = 1;

pub const WRECKED: TargetSpec = a_card(GEAR, "a gear to kill");

pub fn kill_candidates(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut gear = friendly_gear(ctx, seat);
    gear.sort_unstable();
    gear
}

pub fn kill_cost_payable(ctx: &Ctx, seat: u8) -> bool {
    kill_candidates(ctx, seat).len() >= KILLS
}

pub fn pay_kill_cost(ctx: &mut Ctx, gear: u32) -> Killed {
    let killed = ctx.kill(gear, Cause::Cost);
    if killed != Killed::NotOnBoard {
        ctx.narrate(format!("{{card {gear}}} is killed as an additional cost"));
    }
    killed
}

fn wreck(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(gear) = card_target(ctx, item, 0) {
        kill(ctx, item, gear);
    }
    done()
}

pub static CARD: Card = unit(
    "Zaun Punk",
    &[],
    &[when(play(&[WRECKED], wreck), paid_additional_on_entry)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{chain, play as play_engine, priority, triggers};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const PUNK: u32 = 90;
    const MY_GEAR: u32 = 91;
    const THEIR_GEAR: u32 = 92;
    const SPARE_GEAR: u32 = 93;

    fn punk(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Order".into()],
            ..fixtures::unit(PUNK, zone, 0, "Zaun Punk", 3)
        }
    }

    fn alley(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(punk(zone));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Loot", 2));
        fixture.resolve();
        fixture
    }

    fn paid_and_landed(ctx: &mut Ctx) {
        ctx.raise(Event::Played {
            card: PUNK,
            controller: 0,
            kind: "Unit".into(),
            origin: Origin::Hand,
            paid_additional: true,
        });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_unit_whose_gated_play_trigger_kills_any_gear_and_prints_no_rune_cost() {
        assert!(std::ptr::eq(script_of("Zaun Punk").unwrap(), &CARD));
        assert_eq!(CARD.name, "Zaun Punk");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is an optional rune cost; his kills a friendly gear"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional, "the may is the cost, not the kill");
        assert!(ability.condition.is_some());
        assert_eq!(ability.targets, &[WRECKED]);
        assert_eq!((WRECKED.min, WRECKED.max), (1, 1));
        assert_eq!(WRECKED.kind, TargetKind::Card);
        assert_eq!(WRECKED.filter, GEAR);
        assert_eq!(KILLS, 1);
    }

    #[test]
    fn the_cost_candidates_are_the_controllers_gear_on_the_board_and_paying_kills_one() {
        let mut fixture = alley(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(kill_candidates(&ctx, 0), [MY_GEAR]);
        assert_eq!(
            kill_candidates(&ctx, 1),
            [THEIR_GEAR],
            "each seat reads its own"
        );
        assert!(kill_cost_payable(&ctx, 0));
        assert_eq!(pay_kill_cost(&mut ctx, MY_GEAR), Killed::Yes);
        assert!(ctx.in_trash(MY_GEAR));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: false, .. } if *card == MY_GEAR
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MY_GEAR}}} is killed as an additional cost"
        )));
        assert_eq!(
            pay_kill_cost(&mut ctx, MY_GEAR),
            Killed::NotOnBoard,
            "a dead gear cannot pay twice"
        );
        assert!(kill_candidates(&ctx, 0).is_empty());
        assert!(
            !kill_cost_payable(&ctx, 0),
            "356.2.b · without a friendly gear the optional cost cannot be offered"
        );
    }

    #[test]
    fn paid_the_trigger_offers_every_gear_on_the_board_and_the_pick_dies_when_it_resolves() {
        let mut fixture = alley(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::gear(SPARE_GEAR, fixtures::BASE, 0, "Spare", 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        paid_and_landed(&mut ctx);
        let item = 1;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {MY_GEAR}}}"),
                format!("{{card {THEIR_GEAR}}}"),
                format!("{{card {SPARE_GEAR}}}")
            ],
            "a gear · friendly or enemy"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit is not a gear"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_GEAR}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PUNK
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_GEAR)]);
        assert!(ctx.on_board(THEIR_GEAR), "the kill waits for the trigger");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_trash(THEIR_GEAR));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: false, .. } if *card == THEIR_GEAR
        )));
        assert!(ctx.on_board(MY_GEAR));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn unpaid_the_trigger_never_queues() {
        let mut fixture = alley(fixtures::BASE);
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Played {
            card: PUNK,
            controller: 0,
            kind: "Unit".into(),
            origin: Origin::Hand,
            paid_additional: false,
        });
        assert_eq!(triggers::collect(&mut ctx), 0);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · a non-resource optional additional cost (kill a friendly gear) at the pay stage: play::advance knows only the rune cost in Card.additional, so the play never asks which friendly gear dies, never records the kill as paid_additional, and the gated trigger cannot fire from a real play"]
    fn playing_him_asks_which_friendly_gear_dies_and_a_paid_play_fires_the_kill() {
        let mut fixture = alley(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PUNK).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "which friendly gear pays: {:?}",
            ctx.blob.why
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {MY_GEAR}}}"),
                "skip".to_string(),
                "cancel".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_GEAR}}}")).unwrap();
        assert!(
            ctx.in_trash(MY_GEAR),
            "357.2 · the kill is paid before he enters"
        );
        assert_eq!(ctx.location(PUNK), Some(Location::Base(0)));
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {THEIR_GEAR}}}")],
            "the trigger's own target · the paid gear is gone"
        );
    }
}
