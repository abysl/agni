use super::poro_snax::ready_to_exhaust;
use super::prelude::{
    activated, asking, done, draw, gain_xp, gear, named, paying_with, remember_card,
    remembered_cards, usable_if, with_candidates, with_statics, ONE_ENERGY,
};
use super::{Card, Flow, Item, SelfCost, Stage, Static, Timing};
use crate::engine::ctx::Ctx;
use crate::engine::hide;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const PREDICTS: usize = 2;
pub const DRAWS: usize = 1;
pub const XP: u8 = 1;
pub const RECYCLE: u8 = 1;
pub const ORDER: u8 = 2;
pub const QUESTION: &str =
    "the predicted cards to recycle, then the one to put on top (skip keeps their order)";

fn deck_top(ctx: &Ctx, seat: u8, count: usize) -> Vec<u32> {
    let Some(deck) = ctx.zones.main_deck else {
        return Vec::new();
    };
    ctx.top_of(deck, seat, count)
}

fn in_deck(ctx: &Ctx, seat: u8, card: u32) -> bool {
    ctx.zones
        .main_deck
        .is_some_and(|deck| ctx.table.held(deck, seat).any(|held| held.id == card))
}

pub fn predicted(ctx: &Ctx, item: &Item) -> Vec<u32> {
    let remembered = remembered_cards(item);
    let mut looked: Vec<u32> = remembered.clone();
    looked.dedup();
    looked
        .into_iter()
        .filter(|card| in_deck(ctx, item.controller, *card))
        .filter(|card| remembered.iter().filter(|held| *held == card).count() == 1)
        .collect()
}

fn mark_recycled(ctx: &mut Ctx, card: u32) {
    remember_card(ctx, card);
}

pub fn predicted_cards(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != RECYCLE && stage.0 != ORDER {
        return Vec::new();
    }
    predicted(ctx, item)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub type Finish = fn(&mut Ctx, &Item) -> Flow;

pub fn put_on_top(ctx: &mut Ctx, seat: u8, card: u32) {
    let Some(deck) = ctx.zones.main_deck else {
        return;
    };
    hide::reveal_before(ctx, card, deck);
    ctx.emit(Effect::Move {
        card,
        zone: deck,
        seat,
        index: TOP,
    });
}

pub fn look(ctx: &mut Ctx, item: &Item, count: usize, finish: Finish) -> Flow {
    let seat = item.controller;
    let top = deck_top(ctx, seat, count);
    if top.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no card to predict"));
        return finish(ctx, item);
    }
    for card in &top {
        ctx.peek(*card, seat);
        remember_card(ctx, *card);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} predicts {count} · looks at the top {} of their deck",
        if top.len() == 1 {
            "card".to_string()
        } else {
            format!("{} cards", top.len())
        }
    ));
    let max = top.len() as u8;
    Flow::Ask(ctx.ask_resume(item, RECYCLE, 0, max))
}

pub fn recycle(ctx: &mut Ctx, item: &Item, order_up_to: u8, finish: Finish) -> Flow {
    let seat = item.controller;
    let looked = predicted(ctx, item);
    let picked: Vec<u32> = ctx
        .picks()
        .iter()
        .copied()
        .filter(|card| looked.contains(card))
        .collect();
    for card in &picked {
        ctx.recycle_to_bottom(*card);
        mark_recycled(ctx, *card);
        ctx.narrate(format!("{{seat {seat}}} recycles {{card {card}}}"));
    }
    if picked.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles nothing"));
    }
    let kept = looked.len() - picked.len();
    if kept > 1 {
        let max = u8::try_from(kept).unwrap_or(u8::MAX).min(order_up_to);
        return Flow::Ask(ctx.ask_resume(item, ORDER, 0, max));
    }
    finish(ctx, item)
}

pub fn order(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let kept = predicted(ctx, item);
    match ctx.picks().first().copied() {
        Some(card) if kept.contains(&card) => {
            put_on_top(ctx, seat, card);
            ctx.narrate(format!(
                "{{seat {seat}}} puts {{card {card}}} on top of their deck"
            ));
        }
        _ => ctx.narrate(format!(
            "{{seat {seat}}} keeps the predicted cards in their order"
        )),
    }
    finish(ctx, item)
}

fn finish(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    gain_xp(ctx, seat, XP);
    done()
}

fn scry(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        RECYCLE => recycle(ctx, item, 1, finish),
        ORDER => order(ctx, item),
        _ => look(ctx, item, PREDICTS, finish),
    }
}

pub static CARD: Card = with_statics(
    gear(
        "Scryer's Bloom",
        &[],
        &[usable_if(
            named(
                asking(
                    with_candidates(
                        paying_with(
                            activated(Timing::Sorcery, ONE_ENERGY, &[], scry),
                            SelfCost::KillSelf,
                        ),
                        predicted_cards,
                    ),
                    QUESTION,
                ),
                "exhaust and kill this: predict 2, draw 1, gain 1 XP",
            ),
            ready_to_exhaust,
        )],
    ),
    &[Static::EntersExhausted],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const BLOOM: u32 = 90;
    const MY_DECK: [u32; 4] = [20, 21, 22, 23];
    const TOP: u32 = 23;
    const SECOND: u32 = 22;

    fn bloom(zone: u16, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Chaos".into()],
            exhausted,
            ..fixtures::gear(BLOOM, zone, 0, CARD.name, 1)
        }
    }

    fn glade(zone: u16, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bloom(zone, exhausted));
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn peeks(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Peek { card, seat } => Some((*card, *seat)),
                _ => None,
            })
            .collect()
    }

    fn recycled(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card,
                    zone,
                    index: BOTTOM,
                    ..
                } if *zone == fixtures::MAIN_DECK => Some(*card),
                _ => None,
            })
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn activate_and_resolve(ctx: &mut Ctx) {
        activate::activate(ctx, 0, BLOOM, 0).unwrap();
        assert!(!ctx.on_board(BLOOM), "the kill is paid as the cost");
        assert!(ctx.in_trash(BLOOM));
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: RECYCLE
            })
        );
    }

    #[test]
    fn the_script_enters_exhausted_and_has_one_paid_kill_this_activation_gated_on_being_ready() {
        assert!(std::ptr::eq(script_of("Scryer's Bloom").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.has_static(Static::EntersExhausted));
        assert_eq!(CARD.abilities.len(), 1);
        let scry = &CARD.abilities[0];
        assert_eq!(scry.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(scry.cost, Some(ONE_ENERGY));
        assert_eq!(scry.self_cost, SelfCost::KillSelf);
        assert!(
            scry.usable.is_some(),
            "the exhaust is part of the cost: a spent Bloom can't be used"
        );
        assert!(scry.candidates.is_some());
        assert_eq!(scry.question, Some(QUESTION));
        assert!(scry.targets.is_empty());
        assert_eq!((PREDICTS, DRAWS, XP), (2, 1, 1));
    }

    #[test]
    fn playing_it_lands_it_exhausted_so_it_cannot_be_used_this_turn() {
        let mut fixture = glade(fixtures::HAND, false);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLOOM).unwrap();
        assert_eq!(ctx.card(BLOOM).unwrap().zone, Some(fixtures::BASE));
        assert!(ctx.card(BLOOM).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, BLOOM, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the usable gate refuses a Bloom that can't be exhausted"
        );
        assert!(ctx.on_board(BLOOM));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_predict_looks_at_the_top_two_recycles_the_picks_orders_the_rest_then_draws_and_gains_xp()
    {
        let mut fixture = glade(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(deck_of(&ctx, 0), MY_DECK.to_vec());
        activate_and_resolve(&mut ctx);
        assert_eq!(
            peeks(&ctx),
            [(TOP, 0), (SECOND, 0)],
            "436.1.a · the look reaches its controller only"
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 2));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {BLOOM}}}: choose {QUESTION} (0 of 2)")
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {TOP}}}"),
                format!("{{card {SECOND}}}"),
                "done".to_string(),
                "skip".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: ORDER
            }),
            "both stay · which goes on top"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {TOP}}}"),
                format!("{{card {SECOND}}}"),
                "skip".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(recycled(&ctx).is_empty());
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + DRAWS,
            "the draw follows the predict"
        );
        assert!(
            ctx.hand_of(0).contains(&SECOND),
            "the card put on top is the one drawn"
        );
        assert_eq!(deck_of(&ctx, 0), [20, 21, TOP]);
        assert_eq!(ctx.xp(0), i32::from(XP));
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn recycling_both_predicted_cards_skips_the_order_and_draws_the_third() {
        let mut fixture = glade(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        activate_and_resolve(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {TOP}}}")).unwrap();
        assert!(ctx.blob.prompt.is_some(), "one of two picked");
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing left to order");
        assert_eq!(recycled(&ctx), [TOP, SECOND]);
        assert!(ctx.hand_of(0).contains(&21));
        assert_eq!(
            deck_of(&ctx, 0),
            [SECOND, TOP, 20],
            "each recycle goes under the last"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn recycling_one_leaves_the_other_on_top_to_be_drawn_and_a_one_card_deck_predicts_one() {
        let mut fixture = glade(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        activate_and_resolve(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {TOP}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(recycled(&ctx), [TOP]);
        assert!(ctx.hand_of(0).contains(&SECOND));
        assert_eq!(deck_of(&ctx, 0), [TOP, 20, 21]);
        drop(ctx);
        let mut thin = glade(fixtures::BASE, false);
        thin.table
            .cards
            .retain(|card| !(MY_DECK.contains(&card.id) && card.id != TOP));
        thin.resolve();
        let mut ctx = thin.ctx();
        activate_and_resolve(&mut ctx);
        assert_eq!(peeks(&ctx), [(TOP, 0)], "436.4 · as many as possible");
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {TOP}}}"), "skip".to_string()]
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none(), "one card needs no order");
        assert!(ctx.hand_of(0).contains(&TOP));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spent_bloom_the_other_seat_and_a_pool_without_energy_are_refused() {
        let mut spent = glade(fixtures::BASE, true);
        let mut ctx = spent.ctx();
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == BLOOM));
        assert_eq!(
            activate::activate(&mut ctx, 0, BLOOM, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.on_board(BLOOM));
        drop(ctx);
        let mut fixture = glade(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, BLOOM, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        let mut dry = glade(fixtures::BASE, false);
        for rune in dry
            .table
            .cards
            .iter_mut()
            .filter(|card| card.is_kind("Rune") && card.owner == 0)
        {
            rune.exhausted = true;
        }
        dry.resolve();
        let mut ctx = dry.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, BLOOM, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            })
        );
        assert!(ctx.on_board(BLOOM));
        assert!(ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty());
    }
}
