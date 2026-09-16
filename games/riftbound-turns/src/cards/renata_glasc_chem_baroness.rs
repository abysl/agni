use super::aspirants_climb::victory_score;
use super::prelude::{done, exhausting_self, legend, optional, spawn_gold, triggered, ONE_ENERGY};
use super::{Card, Cost, Flow, Item, Stage, Trigger, Who, TOKEN_GOLD};
use crate::engine::ctx::Ctx;

const GOLD_ARRIVES_READY: bool = false;
pub const WITHIN: i32 = 3;
pub const EXTRA: Cost = ONE_ENERGY;

fn bribe(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    spawn_gold(ctx, item.controller, GOLD_ARRIVES_READY);
    done()
}

pub fn within_three_of_the_victory_score(ctx: &Ctx, seat: u8) -> bool {
    victory_score(ctx) - ctx.points(seat) <= WITHIN
}

pub fn baroness_of(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.table
        .cards
        .iter()
        .filter(|held| ctx.face_in_play(held) && !held.is_hidden())
        .filter(|held| ctx.is_legend(held.id) && ctx.controller(held.id) == seat)
        .find(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .map(|held| held.id)
}

pub fn gold_adds_while_paying(ctx: &Ctx, seat: u8, gold: u32) -> Option<Cost> {
    let ready = ctx
        .card(gold)
        .is_some_and(|held| held.name == TOKEN_GOLD && !held.exhausted);
    let mine = ctx.is_token(gold) && ctx.on_board(gold) && ctx.controller(gold) == seat;
    (ready
        && mine
        && baroness_of(ctx, seat).is_some()
        && within_three_of_the_victory_score(ctx, seat))
    .then_some(EXTRA)
}

pub static CARD: Card = legend(
    "Renata Glasc - Chem-Baroness",
    &[],
    &[optional(exhausting_self(triggered(
        Trigger::Hold(Who::You),
        &[],
        bribe,
    )))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost};
    use crate::engine::cost::Cost as Total;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, cleanup, pay, priority, prompts, settle};
    use crate::rules::DEFAULT_VICTORY_SCORE;
    use crate::state::{ItemKind, PromptWhy};

    const RENATA: u32 = fixtures::LEGEND_CARD;
    const GOLD: u32 = 85;

    fn boardroom() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(RENATA).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RENATA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn with_gold(mut fixture: Fixture, exhausted: bool) -> Fixture {
        fixture.table.cards.push(fixtures::gold(GOLD, 0, exhausted));
        fixture.table.tokens.push(GOLD);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        fixture
    }

    fn golds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    fn hold(ctx: &mut Ctx, seat: u8) {
        cleanup::score_holds(ctx, seat);
        settle(ctx).unwrap();
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn energy(amount: u8) -> Total {
        Total {
            energy: amount,
            power: Vec::new(),
            ..Total::default()
        }
    }

    #[test]
    fn the_legend_has_one_may_hold_trigger_that_exhausts_her_and_the_gold_boost_is_a_seam() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.targets.is_empty());
        assert_eq!(EXTRA, ONE_ENERGY);
        assert_eq!(WITHIN, 3);
        let mut fixture = boardroom();
        let ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0).is_empty());
    }

    #[test]
    fn holding_asks_to_exhaust_her_and_yes_plays_an_exhausted_gold_when_the_trigger_resolves() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, .. } if *zone == fixtures::BF1
        )));
        assert_eq!(ctx.points(0), 1, "the hold itself");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 }),
            "392.2 · the may is the cost confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "exhaust {{card {RENATA}}} for the {{card {RENATA}}} trigger · {{zone {}}}?",
                fixtures::BF1
            )
        );
        assert!(!ctx.card(RENATA).unwrap().exhausted);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(RENATA).unwrap().exhausted,
            "exhausting her is the cost"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == RENATA
        ));
        assert!(golds_of(&ctx, 0).is_empty(), "nothing until it resolves");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let gold = *golds_of(&ctx, 0).first().expect("one Gold");
        assert!(ctx.is_token(gold));
        assert!(ctx.is_gear(gold));
        assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        assert!(
            ctx.card(gold).unwrap().exhausted,
            "played exhausted, so it pays on a later turn"
        );
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_or_an_exhausted_renata_plays_no_gold_and_the_opponents_hold_is_not_hers() {
        let mut fixture = boardroom();
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(RENATA).unwrap().exhausted);
        assert!(golds_of(&ctx, 0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {RENATA}}} trigger is removed · its cost is declined"
        )));
        drop(ctx);
        let mut spent = boardroom();
        spent.table.card_mut(RENATA).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        hold(&mut ctx, 0);
        assert!(
            ctx.blob.prompt.is_none(),
            "no question for a cost she cannot pay"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {RENATA}}} trigger is removed · its source is exhausted"
        )));
        drop(ctx);
        let mut theirs = boardroom();
        let mut ctx = theirs.ctx();
        hold(&mut ctx, 1);
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.chain.is_empty(),
            "seat 1 holding is not her controller holding"
        );
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    fn within_three_points_reads_the_victory_score_less_the_seats_score() {
        let mut far = boardroom();
        let ctx = far.ctx();
        assert_eq!(ctx.points(0), 0);
        assert!(
            !within_three_of_the_victory_score(&ctx, 0),
            "{DEFAULT_VICTORY_SCORE} away is not within 3"
        );
        drop(far);
        let mut close = boardroom();
        close.set_points(0, DEFAULT_VICTORY_SCORE - WITHIN);
        let ctx = close.ctx();
        assert!(within_three_of_the_victory_score(&ctx, 0), "exactly 3 away");
        assert!(!within_three_of_the_victory_score(&ctx, 1));
        drop(close);
        let mut closer = boardroom();
        closer.set_points(0, DEFAULT_VICTORY_SCORE - WITHIN - 1);
        let ctx = closer.ctx();
        assert!(!within_three_of_the_victory_score(&ctx, 0), "4 away");
    }

    #[test]
    fn her_gold_adds_one_energy_more_only_for_her_controller_within_reach_of_victory() {
        let mut fixture = with_gold(boardroom(), false);
        fixture.set_points(0, DEFAULT_VICTORY_SCORE - 1);
        let mut ctx = fixture.ctx();
        assert_eq!(baroness_of(&ctx, 0), Some(RENATA));
        assert_eq!(baroness_of(&ctx, 1), None);
        assert_eq!(gold_adds_while_paying(&ctx, 0, GOLD), Some(EXTRA));
        assert_eq!(
            gold_adds_while_paying(&ctx, 1, GOLD),
            None,
            "the Gold is seat 0's"
        );
        assert_eq!(
            gold_adds_while_paying(&ctx, 0, fixtures::HAND_GEAR),
            None,
            "a gear that is no Gold"
        );
        assert!(ctx.exhaust(GOLD));
        assert_eq!(
            gold_adds_while_paying(&ctx, 0, GOLD),
            None,
            "an exhausted Gold adds nothing"
        );
        drop(ctx);
        let mut far = with_gold(boardroom(), false);
        let ctx = far.ctx();
        assert_eq!(
            gold_adds_while_paying(&ctx, 0, GOLD),
            None,
            "eight points away is not within three"
        );
        drop(ctx);
        let mut spent = with_gold(boardroom(), false);
        spent.set_points(0, DEFAULT_VICTORY_SCORE - 1);
        spent.table.card_mut(RENATA).unwrap().exhausted = true;
        let ctx = spent.ctx();
        assert_eq!(
            gold_adds_while_paying(&ctx, 0, GOLD),
            Some(EXTRA),
            "a static reads nothing from her exhaustion"
        );
        drop(ctx);
        let mut nobody = Fixture::enforced();
        nobody.table.cards.push(fixtures::gold(GOLD, 0, false));
        nobody.table.tokens.push(GOLD);
        nobody.set_points(0, DEFAULT_VICTORY_SCORE - 1);
        nobody.resolve();
        let ctx = nobody.ctx();
        assert_eq!(baroness_of(&ctx, 0), None, "Lillia is no Baroness");
        assert_eq!(gold_adds_while_paying(&ctx, 0, GOLD), None);
    }

    #[test]
    fn within_reach_of_victory_a_gold_pays_an_energy_the_rune_pool_is_short_of() {
        let mut fixture = with_gold(boardroom(), false);
        fixture.set_points(0, DEFAULT_VICTORY_SCORE - 1);
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert!(ctx.ready_runes_of(0).is_empty());
        let planned = pay::plan(&ctx, 0, &energy(1)).expect("the Gold adds the energy");
        pay::pay(&mut ctx, 0, &planned);
        assert!(
            !ctx.on_board(GOLD),
            "the Gold is killed to add, as its own text says"
        );
        drop(ctx);
        let mut far = with_gold(boardroom(), false);
        for rune in [41, 42, 43] {
            far.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = far.ctx();
        assert!(
            pay::plan(&ctx, 0, &energy(1)).is_err(),
            "eight points from victory the Gold is a rainbow alone"
        );
    }
}
