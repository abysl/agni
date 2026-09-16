use super::prelude::{asking, battlefield, done, draw, on_conquer, with_candidates};
use super::sett_brawler::{buffed_friendly_units, spend_buff};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const PICK: u8 = 1;
pub const DRAWS: usize = 1;

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    buffed_friendly_units(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn meditate(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == PICK {
        let offered = buffed_friendly_units(ctx, seat);
        let Some(unit) = ctx
            .picks()
            .first()
            .copied()
            .filter(|picked| offered.contains(picked))
        else {
            return done();
        };
        if spend_buff(ctx, unit) {
            draw(ctx, seat, DRAWS);
        }
        return done();
    }
    if buffed_friendly_units(ctx, seat).is_empty() {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, PICK, 0, 1))
}

pub static CARD: Card = battlefield(
    "Monastery of Hirana",
    &[],
    &[asking(
        with_candidates(on_conquer(&[], meditate), candidates),
        "a buff to spend to draw 1",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::ctx::{Event, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::{CounterInfo, Target};

    const MONASTERY: u32 = fixtures::GROUNDS;
    const MONK: u32 = 90;

    fn buffed(fixture: &mut Fixture, unit: u32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(unit),
            counter: COUNTER_BUFFED,
            value: 1,
        });
        fixture.table.counters.sort();
    }

    fn monastery() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(MONASTERY).unwrap().name = "Monastery of Hirana".into();
        fixture
            .table
            .cards
            .push(fixtures::unit(MONK, fixtures::BF1, 0, "Kinkou Monk", 2));
        buffed(&mut fixture, fixtures::VI);
        buffed(&mut fixture, fixtures::THEIR_UNIT);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MONASTERY).unwrap(),
            &CARD
        ));
        fixture
    }

    fn conquer(ctx: &mut Ctx, seat: u8) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(seat)
        );
        settle(ctx).unwrap();
    }

    fn monastery_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == MONASTERY => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_conquer_here_trigger_that_asks_at_resolution() {
        assert!(std::ptr::eq(
            script_of("Monastery of Hirana").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(
            ability.targets.is_empty(),
            "355.10.c · the buff is not chosen"
        );
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some("a buff to spend to draw 1"));
        assert!(ability.timing().is_none());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn conquering_here_may_spend_a_buff_anywhere_you_control_one_to_draw_one() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx, 0);
        assert_eq!(monastery_items(&ctx), [0]);
        assert!(ctx.blob.prompt.is_none(), "the choice waits for resolution");
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume { stage: 1, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "skip".to_string()],
            "Vi's buff in the base counts; the unbuffed Monk and the enemy's buff do not"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {MONASTERY}}}: choose a buff to spend to draw 1 (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        assert!(ctx.is_buffed(fixtures::THEIR_UNIT));
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_draws_nothing_and_keeps_the_buff() {
        let mut fixture = monastery();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx, 0);
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { .. })));
    }

    #[test]
    fn the_conqueror_without_a_buff_of_their_own_is_asked_nothing_and_draws_nothing() {
        let mut fixture = monastery();
        fixture
            .table
            .counters
            .retain(|counter| counter.target != Target::Card(fixtures::VI));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        conquer(&mut ctx, 0);
        assert_eq!(monastery_items(&ctx), [0], "the trigger still fires");
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "702.2.b.2 · the enemy's buff is not the conqueror's to spend"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.is_buffed(fixtures::THEIR_UNIT));
    }

    #[test]
    fn the_enemy_conquering_here_gets_their_own_trigger_with_their_own_buffs() {
        let mut fixture = monastery();
        fixture.table.card_mut(MONK).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(1).len();
        conquer(&mut ctx, 1);
        assert_eq!(monastery_items(&ctx), [1]);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                "skip".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(!ctx.is_buffed(fixtures::THEIR_UNIT));
        assert!(
            ctx.is_buffed(fixtures::VI),
            "seat 0's buff was never on offer"
        );
        assert_eq!(ctx.hand_of(1).len(), hand + 1);
    }
}
