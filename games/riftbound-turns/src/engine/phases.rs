use crate::engine::ctx::{Ctx, Event};
use crate::engine::legal::Reason;
use crate::engine::{chain, cleanup, expiry, roll, triggers};
use crate::rules;
use crate::state::{Phase, PromptWhy, SetupStage, When};
use crate::Refusal;
use agni_plugin_sdk::decide::Effect;

pub const OPENING_HAND: usize = 4;
pub const MULLIGAN_MAX: u8 = 2;

pub fn setup(ctx: &mut Ctx) {
    let players = ctx.players();
    ctx.blob.set_phase(Phase::Setup);
    ctx.record_chosen_champions();
    for seat in 0..players {
        zero_counters(ctx, seat);
        let short = OPENING_HAND.saturating_sub(ctx.hand_of(seat).len());
        if short > 0 {
            ctx.deal(seat, short);
        }
        let row = ctx.blob.seat_mut(seat);
        row.setup = SetupStage::Drawn;
        row.draws = 0;
    }
    ctx.narrate("setup · mulligans in turn order");
    let first = ctx.blob.core().map(|core| core.first).unwrap_or(0);
    open_mulligan(ctx, first);
}

fn zero_counters(ctx: &mut Ctx, seat: u8) {
    for (counter, held) in [
        (rules::COUNTER_POINTS, ctx.points(seat)),
        (rules::COUNTER_XP, ctx.xp(seat)),
    ] {
        if held != 0 {
            ctx.emit(Effect::score(seat, counter, -held));
            ctx.narrate(format!("{{seat {seat}}} starts the game at 0"));
        }
    }
}

fn open_mulligan(ctx: &mut Ctx, seat: u8) {
    ctx.ask(seat, 0, MULLIGAN_MAX, false, PromptWhy::Mulligan);
}

pub fn mulligan_gesture(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
    let Some(prompt) = ctx.blob.prompt.as_mut() else {
        return Err(Refusal::NoPrompt);
    };
    if !prompt.record(agni_plugin_sdk::prompt::Answer::Card(card)) {
        return Err(Refusal::AlreadyPicked);
    }
    let full = prompt.is_full();
    let picked = prompt.picked.clone();
    let bottom = ctx
        .zones
        .main_deck
        .and_then(|deck| ctx.table.held(deck, seat).next())
        .map(|held| held.id);
    if bottom != Some(card) {
        ctx.recycle_to_bottom(card);
    }
    if full {
        ctx.blob.close_prompt();
        finish_mulligan(ctx, seat, &picked);
    }
    Ok(())
}

pub fn finish_mulligan(ctx: &mut Ctx, seat: u8, set_aside: &[u32]) {
    let hand = ctx.zones.hand;
    let still_in_hand: Vec<u32> = set_aside
        .iter()
        .copied()
        .filter(|card| {
            ctx.card(*card)
                .is_some_and(|held| held.zone == hand && held.owner == seat)
        })
        .collect();
    let count = set_aside.len();
    if count > 0 {
        ctx.deal(seat, count);
        for card in still_in_hand {
            ctx.recycle_to_bottom(card);
        }
        ctx.narrate(format!("{{seat {seat}}} sets aside {count} and redraws"));
    } else {
        ctx.narrate(format!("{{seat {seat}}} keeps their hand"));
    }
    {
        let row = ctx.blob.seat_mut(seat);
        row.setup = SetupStage::Done;
        row.draws = 0;
    }
    let deck = ctx.zones.main_deck;
    let recycled: Vec<u32> = set_aside
        .iter()
        .copied()
        .filter(|card| {
            ctx.card(*card)
                .is_some_and(|held| held.zone == deck && held.owner == seat)
        })
        .collect();
    if roll::open(ctx, seat, recycled, roll::WHY_MULLIGAN) {
        return;
    }
    after_mulligan(ctx, seat);
}

pub fn after_mulligan(ctx: &mut Ctx, seat: u8) {
    if ctx.blob.phase() != Some(Phase::Setup) {
        return;
    }
    let players = ctx.players();
    let order = ctx.blob.order();
    let mut next = order.next_seat(seat);
    for _ in 0..players {
        if ctx.blob.seat(next).setup != SetupStage::Done {
            open_mulligan(ctx, next);
            return;
        }
        next = order.next_seat(next);
    }
    start_turn(ctx);
}

pub fn start_turn(ctx: &mut Ctx) {
    let seat = ctx.turn_player();
    ctx.blob.set_phase(Phase::Awaken);
    let exhausted: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|card| card.exhausted && ctx.controller(card.id) == seat)
        .filter(|card| {
            ctx.on_board(card.id) || card.zone.is_some_and(|zone| ctx.zones.legend == Some(zone))
        })
        .map(|card| card.id)
        .collect();
    for card in exhausted {
        ctx.awaken(card, seat);
    }
    ctx.blob.set_phase(Phase::Beginning);
    ctx.raise(Event::BeginningPhase { seat });
    triggers::queue_delayed(ctx, When::BeginningOf(seat));
    chain::proceed(ctx);
}

pub fn continue_beginning(ctx: &mut Ctx) {
    if ctx.blob.phase() != Some(Phase::Beginning) || ctx.won.is_some() {
        return;
    }
    let seat = ctx.turn_player();
    let turn = ctx.turn();
    cleanup::score_holds(ctx, seat);
    if cleanup::win_check(ctx).is_some() {
        return;
    }
    ctx.blob.set_phase(Phase::Channel);
    let runes = ctx.channel_count(seat);
    ctx.channel(seat, runes);
    ctx.blob.set_phase(Phase::Draw);
    match ctx.skips_draw_phase(seat) {
        Some(by) => ctx.narrate(format!(
            "{{seat {seat}}} skips their Draw Phase · {{card {by}}}"
        )),
        None => {
            ctx.draw(seat, rules::CARDS_PER_TURN);
        }
    }
    ctx.blob.set_phase(Phase::Action);
    ctx.empty_pools();
    ctx.narrate(format!("turn {turn} · {{seat {seat}}}"));
    cleanup::run(ctx, None);
}

pub fn end_turn(ctx: &mut Ctx) -> Result<(), Refusal> {
    let blob = &*ctx.blob;
    if blob.showdown.is_some() || !blob.staged.is_empty() || blob.contested_zones().next().is_some()
    {
        return Err(Refusal::ShowdownOpen);
    }
    if blob.prompt.is_some() {
        return Err(Refusal::PromptOpen);
    }
    if blob.priority.is_some() || !blob.chain.is_empty() || !blob.queue.is_empty() {
        return Err(Refusal::Illegal(Reason::ChainClosed));
    }
    if blob.roll.is_some() {
        return Err(Refusal::RollFirst);
    }
    if blob.phase() != Some(Phase::Action) {
        return Err(Refusal::Illegal(Reason::NotActionPhase));
    }
    let seat = ctx.turn_player();
    let turn = ctx.turn();
    ctx.blob.set_phase(Phase::Ending);
    expiry::clear_stuns(ctx);
    ctx.raise(Event::EndingStep { seat });
    triggers::queue_delayed(ctx, When::EndOfTurn(turn));
    chain::proceed(ctx);
    Ok(())
}

pub fn finish_turn(ctx: &mut Ctx) {
    if ctx.blob.phase() != Some(Phase::Ending) {
        return;
    }
    ctx.blob.set_phase(Phase::Cleanup);
    ctx.heal_all();
    ctx.blob.set_phase(Phase::Expiration);
    expiry::at_expiration(ctx);
    if !ctx.blob.extra_turns.is_empty() {
        let seat = ctx.blob.extra_turns.remove(0);
        if let Some(core) = ctx.blob.core_mut() {
            core.turn = core.turn.saturating_add(1);
            core.player = seat;
        }
        ctx.narrate(format!("{{seat {seat}}} takes an extra turn"));
    } else if let Some(core) = ctx.blob.core_mut() {
        core.advance();
    }
    super::expire_free_table(ctx);
    start_turn(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::prompts;
    use crate::engine::roll;
    use crate::rules::{COUNTER_POINTS, COUNTER_XP};
    use crate::state::{
        Expiry, GameBlob, Mode, SeatState, Showdown, FLAG_ENTERED_THIS_TURN, FLAG_NO_MOVE_BY_OWNER,
        FLAG_STUNNED,
    };
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::dice::commitment;
    use agni_plugin_sdk::prompt::Prompt;
    use agni_plugin_sdk::table::Target;

    fn fresh() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture
    }

    #[test]
    fn setup_tops_hands_up_to_four_and_asks_the_first_player_to_mulligan() {
        let mut fixture = fresh();
        let mut ctx = fixture.ctx();
        setup(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Setup));
        assert_eq!(ctx.hand_of(0).len(), 4);
        assert_eq!(ctx.hand_of(1).len(), 3);
        assert_eq!(ctx.blob.seat(0).setup, SetupStage::Drawn);
        assert_eq!(ctx.blob.seat(1).setup, SetupStage::Drawn);
        assert_eq!(ctx.blob.seat(1).draws, 0);
        assert!(
            ctx.events.is_empty(),
            "the opening hand is dealt, not drawn: no Drew event wakes a Draw trigger"
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, MULLIGAN_MAX));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Mulligan));
        let labels: Vec<String> = prompts::offered(&ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect();
        assert_eq!(
            labels,
            [
                "set aside {card 70}",
                "set aside {card 71}",
                "set aside {card 72}",
                "set aside {card 73}",
                "keep"
            ]
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Mulligan),
            "set aside up to 2 cards to redraw"
        );
        assert_eq!(ctx.effects.len(), 2);
        let mut tampered = fresh();
        tampered.set_xp(0, 6);
        tampered.set_points(1, 7);
        let mut ctx = tampered.ctx();
        setup(&mut ctx);
        assert_eq!(ctx.xp(0), 0, "lobby counter edits do not survive the start");
        assert_eq!(ctx.points(1), 0);
        assert!(ctx.effects.contains(&Effect::score(0, COUNTER_XP, -6)));
        assert!(ctx.effects.contains(&Effect::score(1, COUNTER_POINTS, -7)));
    }

    #[test]
    fn mulligans_by_button_and_by_gesture_walk_both_seats_then_start_turn_one() {
        let mut fixture = fresh();
        let mut ctx = fixture.ctx();
        setup(&mut ctx);
        let effects_before = ctx.effects.len();
        ctx.blob.close_prompt();
        finish_mulligan(&mut ctx, 0, &[fixtures::HAND_UNIT, fixtures::HAND_SPELL]);
        assert_eq!(
            &ctx.effects[effects_before..],
            [
                Effect::Move {
                    card: 23,
                    zone: fixtures::HAND,
                    seat: 0,
                    index: TOP
                },
                Effect::Move {
                    card: 22,
                    zone: fixtures::HAND,
                    seat: 0,
                    index: TOP
                },
                Effect::Move {
                    card: fixtures::HAND_UNIT,
                    zone: fixtures::MAIN_DECK,
                    seat: 0,
                    index: BOTTOM
                },
                Effect::Move {
                    card: fixtures::HAND_SPELL,
                    zone: fixtures::MAIN_DECK,
                    seat: 0,
                    index: BOTTOM
                },
            ]
        );
        assert_eq!(ctx.blob.seat(0).setup, SetupStage::Done);
        assert_eq!(ctx.blob.seat(0).draws, 0);
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Drew { .. })),
            "a mulligan redraw is not a turn draw either"
        );
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Shuffle {
                why: roll::WHY_MULLIGAN
            }),
            "two recycled cards are ordered by a roll before the next seat mulligans"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} sets aside 2 and redraws".to_string()));
        for seat in 0..2u8 {
            roll::commit(&mut ctx, seat, commitment(&[seat + 1; 8])).unwrap();
        }
        for seat in 0..2u8 {
            roll::reveal(&mut ctx, seat, [seat + 1; 8]).unwrap();
        }
        assert!(ctx.blob.roll.is_none());
        assert_eq!(ctx.blob.why, Some(PromptWhy::Mulligan));
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        let gesture = fixtures::move_to_bottom(fixtures::THEIR_HAND_CARD, fixtures::MAIN_DECK, 1);
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = blob.clone();
        let mut ctx = fixture.ctx_for(1, &gesture);
        mulligan_gesture(&mut ctx, 1, fixtures::THEIR_HAND_CARD).unwrap();
        assert_eq!(
            ctx.blob.prompt.as_ref().unwrap().picked,
            [fixtures::THEIR_HAND_CARD]
        );
        assert_eq!(
            mulligan_gesture(&mut ctx, 1, fixtures::THEIR_HAND_CARD),
            Err(Refusal::AlreadyPicked)
        );
        assert!(ctx.effects.is_empty());
        let picked = ctx.blob.prompt.as_ref().unwrap().picked.clone();
        ctx.blob.close_prompt();
        finish_mulligan(&mut ctx, 1, &picked);
        assert_eq!(ctx.blob.seat(1).setup, SetupStage::Done);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!((ctx.turn(), ctx.turn_player()), (1, 0));
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.effects[0],
            Effect::Move {
                card: fixtures::THEIR_HAND_CARD,
                zone: fixtures::HAND,
                seat: 1,
                index: TOP
            }
        );
        assert!(ctx.effects.contains(&Effect::ready(fixtures::RUNE_A)));
        assert_eq!(ctx.blob.log.last().unwrap(), "turn 1 · {seat 0}");
        blob.prompt = None;
        blob.why = None;
        let mut second = fresh();
        second.blob = blob;
        let mut ctx = second.ctx();
        mulligan_gesture(&mut ctx, 1, 5).unwrap_err();
    }

    #[test]
    fn a_set_aside_dropped_anywhere_on_the_deck_sinks_to_the_bottom() {
        let mut fixture = fresh();
        fixture.blob.set_phase(Phase::Setup);
        fixture.blob.open_prompt(crate::state::Ask {
            prompt: Prompt::new(1, 0, 0, MULLIGAN_MAX),
            why: PromptWhy::Mulligan,
        });
        let on_top = fixtures::move_action(fixtures::HAND_UNIT, fixtures::MAIN_DECK, 0);
        let mut ctx = fixture.ctx_for(0, &on_top);
        mulligan_gesture(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert_eq!(
            ctx.effects,
            [Effect::Move {
                card: fixtures::HAND_UNIT,
                zone: fixtures::MAIN_DECK,
                seat: 0,
                index: BOTTOM
            }]
        );
        assert_eq!(
            ctx.table
                .held(fixtures::MAIN_DECK, 0)
                .next()
                .map(|card| card.id),
            Some(fixtures::HAND_UNIT)
        );
        assert_eq!(
            ctx.blob.prompt.as_ref().unwrap().picked,
            [fixtures::HAND_UNIT]
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_hold_that_wins_ends_the_beginning_phase_before_the_channel_and_the_draw() {
        let mut fixture = fresh();
        fixture.set_points(0, crate::rules::DEFAULT_VICTORY_SCORE - 1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        start_turn(&mut ctx);
        assert_eq!(
            ctx.effects,
            [
                Effect::ready(fixtures::RUNE_A),
                Effect::score(0, COUNTER_POINTS, 1)
            ]
        );
        assert_eq!(ctx.won, Some(0));
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(ctx.hand_of(0).len(), 4);
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(
            ctx.blob.log,
            ["{seat 0} holds {zone 9}", "{seat 0} wins with 8 points"]
        );
    }

    #[test]
    fn the_start_of_turn_awakens_kills_temporaries_scores_holds_channels_and_draws_in_order() {
        let mut fixture = fresh();
        fixture.blob.core_mut().unwrap().turn = 2;
        fixture.blob.core_mut().unwrap().player = 1;
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.table.card_mut(44).unwrap().exhausted = true;
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF2, 1, "Jinx", 2));
        fixture
            .table
            .counters
            .push(agni_plugin_sdk::table::CounterInfo {
                target: Target::Card(fixtures::SPRITE),
                counter: crate::rules::COUNTER_TEMPORARY,
                value: 1,
            });
        fixture.table.counters.sort();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        start_turn(&mut ctx);
        assert_eq!(
            ctx.effects,
            [Effect::ready(44), Effect::ready(fixtures::THEIR_UNIT)],
            "the awaken step runs, then the Temporary trigger waits on the chain"
        );
        assert_eq!(ctx.blob.chain.len(), 1, "Temporary is a trigger of its own");
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert!(
            ctx.blob.holder(fixtures::BF2) == Some(1) && !ctx.blob.scored(fixtures::BF2, 1),
            "the Temporary kill lands before the Hold is scored"
        );
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            ctx.effects,
            [
                Effect::ready(44),
                Effect::ready(fixtures::THEIR_UNIT),
                Effect::Despawn {
                    card: fixtures::SPRITE
                },
                Effect::score(1, COUNTER_POINTS, 1),
                Effect::Move {
                    card: 35,
                    zone: fixtures::RUNE_POOL,
                    seat: 1,
                    index: TOP
                },
                Effect::Move {
                    card: 34,
                    zone: fixtures::RUNE_POOL,
                    seat: 1,
                    index: TOP
                },
                Effect::Move {
                    card: 33,
                    zone: fixtures::RUNE_POOL,
                    seat: 1,
                    index: TOP
                },
                Effect::Move {
                    card: 25,
                    zone: fixtures::HAND,
                    seat: 1,
                    index: TOP
                },
            ]
        );
        assert!(ctx.blob.scored(fixtures::BF2, 1));
        assert_eq!(ctx.blob.holder(fixtures::BF1), None);
        assert_eq!(ctx.blob.seat(1).draws, 1);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(ctx.events.contains(&Event::Held {
            zone: fixtures::BF2,
            seat: 1,
            units: vec![90]
        }));
        assert!(ctx.events.contains(&Event::BeginningPhase { seat: 1 }));
        assert_eq!(
            &ctx.blob.log[ctx.blob.log.len() - 2..],
            ["{seat 1} holds {zone 10}", "turn 2 · {seat 1}"]
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 60} is Temporary and dies"));
        let mut lost = fresh();
        lost.blob.set_holder(fixtures::BF2, Some(1));
        lost.table.cards.retain(|card| card.id != fixtures::SPRITE);
        lost.resolve();
        let mut ctx = lost.ctx();
        start_turn(&mut ctx);
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Counter { .. })));
    }

    #[test]
    fn a_temporary_kill_and_a_battlefield_beginning_trigger_are_one_batch_the_seat_orders() {
        use crate::cards::prelude::{battlefield, triggered};
        use crate::cards::{Card, Flow, Trigger};
        static LAB: Card = battlefield(
            "Lab",
            &[],
            &[triggered(Trigger::BeginningPhase, &[], |ctx, item, _| {
                crate::cards::prelude::draw(ctx, item.controller, 1);
                Flow::Done
            })],
        );
        let mut fixture = fresh();
        fixture.blob.core_mut().unwrap().turn = 2;
        fixture.blob.core_mut().unwrap().player = 1;
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::ROCKFALL, &LAB);
        let mut ctx = fixture.ctx();
        start_turn(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 1 }),
            "376.3.b · one seat, two simultaneous triggers, one order"
        );
        let labels: Vec<String> = prompts::offered(&ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect();
        assert_eq!(
            labels,
            [
                format!("{{card {}}} trigger", fixtures::ROCKFALL),
                format!("{{card {}}} is Temporary", fixtures::SPRITE)
            ]
        );
        assert!(ctx.blob.queue.len() == 2 && ctx.blob.chain.is_empty());
        let prompt = ctx.blob.prompt.clone().unwrap();
        for option in [0, 0] {
            let answered = prompts::answer(
                &mut ctx,
                1,
                agni_plugin_sdk::prompt::Pick {
                    prompt: prompt.id,
                    option,
                },
            )
            .unwrap();
            if let Some(answered) = answered {
                crate::engine::resume(&mut ctx, &answered).unwrap();
            }
        }
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2, "both triggers went on the chain");
        let hand = ctx.hand_of(1).len();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "the Temporary kill was placed last, so it resolved first"
        );
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.hand_of(1).len(), hand + 1 + rules::CARDS_PER_TURN);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(
            ctx.blob.holder(fixtures::BF2),
            None,
            "the Sprite was the only unit there: the hold is lost before it scores"
        );
        assert!(!ctx.blob.scored(fixtures::BF2, 1));
    }

    #[test]
    fn ending_the_turn_heals_unstuns_expires_resets_and_starts_the_next_turn_inside_one_entry() {
        let mut fixture = fresh();
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.mark_scored(fixtures::BF2, 1);
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF2, 1, "Jinx", 2));
        fixture.blob.seat_mut(0).played_main = true;
        fixture.blob.seat_mut(0).draws = 3;
        fixture
            .blob
            .card_state_mut(fixtures::VI)
            .set(FLAG_ENTERED_THIS_TURN, true);
        fixture
            .blob
            .card_state_mut(fixtures::VI)
            .set(FLAG_NO_MOVE_BY_OWNER, true);
        fixture
            .blob
            .card_state_mut(fixtures::SPRITE)
            .set(FLAG_STUNNED, true);
        fixture
            .table
            .card_mut(fixtures::SPRITE)
            .unwrap()
            .annotations = vec![("stunned".into(), vec![1])];
        fixture
            .table
            .counters
            .push(agni_plugin_sdk::table::CounterInfo {
                target: Target::Card(fixtures::VI),
                counter: crate::engine::ctx::COUNTER_DAMAGE,
                value: 2,
            });
        fixture.table.counters.sort();
        let mut ctx = fixture.ctx();
        ctx.might(fixtures::VI, 2, Expiry::EndOfTurn(1), None, 1);
        ctx.effects.clear();
        end_turn(&mut ctx).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            &ctx.effects[..3],
            [
                Effect::Annotate {
                    card: fixtures::SPRITE,
                    key: "stunned".into(),
                    value: None
                },
                Effect::Counter {
                    target: Target::Card(fixtures::VI),
                    counter: crate::engine::ctx::COUNTER_DAMAGE,
                    delta: -2
                },
                Effect::Counter {
                    target: Target::Card(fixtures::VI),
                    counter: crate::engine::ctx::COUNTER_MIGHT,
                    delta: -2
                },
            ],
            "410.1.a.2 · the stun lifts at the Ending Step, before 317.2's heal"
        );
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(ctx.blob.scored(fixtures::BF2, 1));
        assert_eq!(*ctx.blob.seat(0), SeatState::default());
        assert!(!ctx.has_flag(fixtures::VI, FLAG_ENTERED_THIS_TURN));
        assert!(!ctx.has_flag(fixtures::VI, FLAG_NO_MOVE_BY_OWNER));
        assert!(!ctx.has_flag(fixtures::SPRITE, FLAG_STUNNED));
        assert!(ctx.events.contains(&Event::EndingStep { seat: 0 }));
        assert!(ctx.effects.contains(&Effect::score(1, COUNTER_POINTS, 1)));
        assert_eq!(ctx.blob.log.last().unwrap(), "turn 2 · {seat 1}");
        let mut open = fresh();
        open.blob.showdown = Some(Showdown::open(fixtures::BF1, 0, 1));
        assert_eq!(end_turn(&mut open.ctx()), Err(Refusal::ShowdownOpen));
        let mut asked = fresh();
        asked.blob.prompt = Some(Prompt::new(1, 0, 0, 1));
        assert_eq!(end_turn(&mut asked.ctx()), Err(Refusal::PromptOpen));
        let mut queued = fresh();
        queued.blob.queue.push(crate::state::Pending {
            item: crate::state::ChainItem::new(
                1,
                crate::state::ItemKind::Spell { card: 1 },
                0,
                crate::state::Origin::Hand,
            ),
            needs: crate::state::Needs::Choices,
        });
        assert_eq!(
            end_turn(&mut queued.ctx()),
            Err(Refusal::Illegal(Reason::ChainClosed))
        );
        let mut early = fresh();
        early.blob.set_phase(Phase::Setup);
        assert_eq!(
            end_turn(&mut early.ctx()),
            Err(Refusal::Illegal(Reason::NotActionPhase))
        );
    }

    fn end_turn_through(ctx: &mut Ctx) {
        end_turn(ctx).unwrap();
        fixtures::pass_until_open(ctx);
        crate::engine::settle(ctx).unwrap();
    }

    #[test]
    fn a_queued_extra_turn_is_taken_after_this_one_and_the_rotation_resumes_afterwards() {
        let mut fixture = fresh();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.mark_scored(fixtures::BF1, 0);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.extra_turns = vec![0];
        let mut ctx = fixture.ctx();
        ctx.might(fixtures::VI, 2, Expiry::EndOfTurn(1), None, 1);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        ctx.effects.clear();
        let hand = ctx.hand_of(0).len();
        end_turn_through(&mut ctx);
        assert_eq!(
            (ctx.turn(), ctx.turn_player()),
            (2, 0),
            "735 · the queued seat takes the next turn with a fresh turn counter"
        );
        assert!(ctx.blob.extra_turns.is_empty());
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(
            ctx.effects.contains(&Effect::score(0, COUNTER_POINTS, 1)),
            "the held battlefield scores again on the extra turn: {:?}",
            ctx.effects
        );
        assert!(ctx.blob.scored(fixtures::BF1, 0));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + rules::CARDS_PER_TURN,
            "a whole turn: the draw step runs"
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "this-turn effects expired at the real turn's Expiration do not survive"
        );
        assert!(
            ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move { zone, seat: 0, .. } if Some(*zone) == ctx.zones.rune_pool
            )),
            "the Channel step runs"
        );
        assert_eq!(
            ctx.blob.seat(0).draws,
            1,
            "the seat counters were reset at Expiration and count the extra turn's draw"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} takes an extra turn".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "turn 2 · {seat 0}");
        end_turn_through(&mut ctx);
        assert_eq!(
            (ctx.turn(), ctx.turn_player()),
            (3, 1),
            "737 · the next advance yields the other seat"
        );
        assert!(
            ctx.effects.contains(&Effect::score(0, COUNTER_POINTS, 1)),
            "the hold scored on both turns"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_queued_turns_run_newest_first_and_the_first_turn_rune_adjustment_never_applies() {
        let mut fixture = fresh();
        {
            let mut ctx = fixture.ctx();
            ctx.queue_turn(0);
            ctx.queue_turn(1);
        }
        assert_eq!(
            fixture.blob.extra_turns,
            [1, 0],
            "738 · the turn queued last is taken first"
        );
        let mut ctx = fixture.ctx();
        end_turn_through(&mut ctx);
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        assert_eq!(ctx.blob.extra_turns, [0]);
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| matches!(
                    effect,
                    Effect::Move { zone, seat: 1, .. } if Some(*zone) == ctx.zones.rune_pool
                ))
                .count(),
            3,
            "turn two of a two-player table channels the extra rune whoever takes it (recorded)"
        );
        ctx.effects.clear();
        end_turn_through(&mut ctx);
        assert_eq!((ctx.turn(), ctx.turn_player()), (3, 0));
        assert!(ctx.blob.extra_turns.is_empty());
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| matches!(
                    effect,
                    Effect::Move { zone, seat: 0, .. } if Some(*zone) == ctx.zones.rune_pool
                ))
                .count(),
            2
        );
        end_turn_through(&mut ctx);
        assert_eq!((ctx.turn(), ctx.turn_player()), (4, 1));
    }

    #[test]
    fn the_awaken_step_readies_what_the_turn_player_controls_not_what_they_own() {
        let mut fixture = fresh();
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture
            .table
            .cards
            .push(fixtures::gear(90, fixtures::BASE, 1, "Stolen Gear", 1));
        fixture.table.card_mut(90).unwrap().exhausted = true;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.set_controller(90, 0, fixtures::VI));
        ctx.effects.clear();
        start_turn(&mut ctx);
        assert!(
            ctx.effects.contains(&Effect::ready(90)),
            "the gear seat 0 controls readies on seat 0's Awaken: {:?}",
            ctx.effects
        );
        assert!(
            !ctx.effects.contains(&Effect::ready(fixtures::THEIR_UNIT)),
            "the other seat's own unit waits for its controller's turn"
        );
        assert!(ctx.effects.contains(&Effect::ready(fixtures::RUNE_A)));
    }

    #[test]
    fn a_free_table_proposal_is_withdrawn_by_its_proposer_or_expires_with_the_turn() {
        let mut fixture = fresh();
        let mut ctx = fixture.ctx();
        super::super::game(&mut ctx, 0, crate::TurnEvent::FreeTable).unwrap();
        assert_eq!(ctx.blob.free_table, Some(0));
        super::super::game(&mut ctx, 0, crate::TurnEvent::FreeTable).unwrap();
        assert_eq!(ctx.blob.free_table, None, "the proposer takes it back");
        assert_eq!(
            ctx.blob.log.last().map(String::as_str),
            Some("{seat 0} withdraws the free table proposal")
        );
        assert_eq!(ctx.blob.mode, Mode::Enforced);
        super::super::game(&mut ctx, 0, crate::TurnEvent::FreeTable).unwrap();
        end_turn_through(&mut ctx);
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        assert_eq!(
            ctx.blob.free_table, None,
            "the proposal does not outlive the turn"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s free table proposal expired with the turn".to_string()));
        assert_eq!(ctx.blob.mode, Mode::Enforced);
        assert_eq!(
            super::super::game(&mut ctx, 0, crate::TurnEvent::FreeTable),
            Err(Refusal::NotYourTurn),
            "a fresh proposal needs the new turn player"
        );
    }

    #[test]
    fn a_concession_ends_the_game_for_the_last_seat_standing() {
        let mut fixture = fresh();
        let mut ctx = fixture.ctx();
        super::super::game(&mut ctx, 1, crate::TurnEvent::Concede).unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.conceded, [1]);
        assert_eq!(ctx.winner(), Some(0));
        assert_eq!(ctx.won, Some(0));
        assert_eq!(ctx.points_winner(), None, "no points changed hands");
        assert!(ctx.blob.log.contains(&"{seat 1} concedes".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} wins by concession".to_string()));
        assert_eq!(
            super::super::game(&mut ctx, 1, crate::TurnEvent::Concede),
            Err(Refusal::AlreadyConceded)
        );
        assert_eq!(
            super::super::game(&mut ctx, 7, crate::TurnEvent::Concede),
            Err(Refusal::NoSuchSeat)
        );
    }
}
