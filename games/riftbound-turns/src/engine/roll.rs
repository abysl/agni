use crate::engine::ctx::{is_rune_face, Ctx};
use crate::engine::phases;
use crate::state::{Ask, InGameRoll, PromptWhy, DIE_SIDES};
use crate::Refusal;
use agni_plugin_sdk::decide::{Effect, BOTTOM};
use agni_plugin_sdk::dice::{Roll, SECRET_LEN};
use agni_plugin_sdk::prompt::Prompt;

pub const WHY_MULLIGAN: u8 = 0;
pub const WHY_BURN_OUT: u8 = 1;

pub const ROLL_ID_BASE: u32 = 0x1_0000;

pub fn open(ctx: &mut Ctx, seat: u8, cards: Vec<u32>, why: u8) -> bool {
    if cards.len() < 2 || ctx.blob.roll.is_some() || ctx.blob.prompt.is_some() {
        return false;
    }
    let count = u8::try_from(cards.len()).unwrap_or(u8::MAX);
    let id = ctx.blob.next_prompt_id();
    let mut prompt = Prompt::new(id, seat, count, count);
    prompt.picked = cards;
    ctx.blob.open_prompt(Ask {
        prompt,
        why: PromptWhy::Shuffle { why },
    });
    let players = ctx.players();
    ctx.blob.roll = Some(InGameRoll {
        id: ROLL_ID_BASE + u32::from(id),
        roll: Roll::new(players, DIE_SIDES),
        why,
    });
    ctx.narrate(format!(
        "{{seat {seat}}} recycles {count} cards · every seat rolls to order them"
    ));
    true
}

pub fn is_open(ctx: &Ctx) -> bool {
    ctx.blob.roll.is_some()
}

pub fn commit(ctx: &mut Ctx, seat: u8, commit: [u8; SECRET_LEN]) -> Result<(), Refusal> {
    let open = ctx.blob.roll.as_mut().ok_or(Refusal::NoRoll)?;
    open.roll.commit(seat, commit).map_err(Refusal::Dice)
}

pub fn reveal(ctx: &mut Ctx, seat: u8, secret: [u8; SECRET_LEN]) -> Result<(), Refusal> {
    let open = ctx.blob.roll.as_mut().ok_or(Refusal::NoRoll)?;
    open.roll.reveal(seat, secret).map_err(Refusal::Dice)?;
    if open.roll.all_revealed() {
        resolve(ctx);
    }
    Ok(())
}

pub fn abandon(ctx: &mut Ctx) {
    if ctx.blob.roll.take().is_none() {
        return;
    }
    let Some((prompt, PromptWhy::Shuffle { why })) = ctx.blob.close_prompt() else {
        return;
    };
    ctx.narrate("the shuffle roll is abandoned · the cards keep their order");
    continue_after(ctx, prompt.seat, why);
}

fn resolve(ctx: &mut Ctx) {
    let Some(open) = ctx.blob.roll.take() else {
        return;
    };
    let Some((prompt, PromptWhy::Shuffle { why })) = ctx.blob.close_prompt() else {
        return;
    };
    let seed = open.roll.pooled().unwrap_or(0) ^ u64::from(open.id);
    let order = permutation(seed, prompt.picked.len());
    let ordered: Vec<u32> = order.iter().map(|at| prompt.picked[*at]).collect();
    for card in &ordered {
        sink(ctx, *card);
    }
    let names: Vec<String> = ordered
        .iter()
        .rev()
        .map(|card| format!("{{card {card}}}"))
        .collect();
    ctx.narrate(format!(
        "the roll orders the deck bottom: {}",
        names.join(", ")
    ));
    continue_after(ctx, prompt.seat, why);
}

fn continue_after(ctx: &mut Ctx, seat: u8, why: u8) {
    if why == WHY_MULLIGAN {
        phases::after_mulligan(ctx, seat);
    }
}

fn sink(ctx: &mut Ctx, card: u32) {
    let Some(held) = ctx.card(card) else {
        return;
    };
    let owner = held.owner;
    let deck = if is_rune_face(held) {
        ctx.zones.rune_deck
    } else {
        ctx.zones.main_deck
    };
    let Some(deck) = deck else {
        return;
    };
    if held.zone != Some(deck) {
        return;
    }
    ctx.emit(Effect::Move {
        card,
        zone: deck,
        seat: owner,
        index: BOTTOM,
    });
}

pub fn xorshift64(state: &mut u64) -> u64 {
    let mut x = *state;
    if x == 0 {
        x = 0x9e37_79b9_7f4a_7c15;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

pub fn permutation(seed: u64, len: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    let mut state = seed;
    for i in (1..len).rev() {
        let j = (xorshift64(&mut state) % (i as u64 + 1)) as usize;
        order.swap(i, j);
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::phases::{finish_mulligan, setup};
    use crate::state::{GameBlob, Mode, Phase, SetupStage};
    use agni_plugin_sdk::dice::{commitment, DiceRefusal};
    use agni_plugin_sdk::prompt::Pick;

    fn secret(seed: u8) -> [u8; SECRET_LEN] {
        [seed; SECRET_LEN]
    }

    fn fresh() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_permutation_is_a_pure_function_of_the_seed() {
        assert_eq!(permutation(0, 0), Vec::<usize>::new());
        assert_eq!(permutation(7, 1), [0]);
        for seed in [0u64, 1, 42, u64::MAX] {
            let a = permutation(seed, 9);
            let b = permutation(seed, 9);
            assert_eq!(a, b);
            let mut sorted = a.clone();
            sorted.sort_unstable();
            assert_eq!(sorted, (0..9).collect::<Vec<usize>>());
        }
        let orders: std::collections::BTreeSet<Vec<usize>> =
            (0..64u64).map(|seed| permutation(seed, 4)).collect();
        assert!(orders.len() > 8, "the seed moves the order");
    }

    #[test]
    fn a_two_card_mulligan_opens_a_roll_and_the_reveal_orders_the_deck_bottom() {
        let mut fixture = fresh();
        let mut ctx = fixture.ctx();
        setup(&mut ctx);
        ctx.blob.close_prompt();
        finish_mulligan(&mut ctx, 0, &[fixtures::HAND_UNIT, fixtures::HAND_SPELL]);
        assert_eq!(ctx.blob.seat(0).setup, SetupStage::Done);
        let open = ctx
            .blob
            .roll
            .clone()
            .expect("two set-aside cards need a roll");
        assert_eq!(open.why, WHY_MULLIGAN);
        assert_eq!(open.roll.players(), 2);
        assert!(open.id >= ROLL_ID_BASE);
        let prompt = ctx
            .blob
            .prompt
            .clone()
            .expect("the shuffle prompt holds the cards");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Shuffle { why: WHY_MULLIGAN }));
        assert_eq!(prompt.seat, 0);
        assert_eq!(prompt.picked, [fixtures::HAND_UNIT, fixtures::HAND_SPELL]);
        assert!(
            crate::engine::prompts::offered(&ctx).is_empty(),
            "a shuffle prompt has no pick options; the dice answer it"
        );
        assert_eq!(
            crate::engine::prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            )
            .unwrap_err(),
            Refusal::Pick(agni_plugin_sdk::prompt::PickRefusal::NoSuchOption {
                option: 0,
                count: 0
            })
        );
        assert_eq!(
            deck_of(&ctx, 0),
            [fixtures::HAND_SPELL, fixtures::HAND_UNIT, 20, 21]
        );
        assert_eq!(
            reveal(&mut ctx, 0, secret(1)),
            Err(Refusal::Dice(DiceRefusal::NotEveryoneCommitted))
        );
        commit(&mut ctx, 0, commitment(&secret(1))).unwrap();
        assert_eq!(
            commit(&mut ctx, 0, commitment(&secret(1))),
            Err(Refusal::Dice(DiceRefusal::AlreadyCommitted))
        );
        commit(&mut ctx, 1, commitment(&secret(2))).unwrap();
        assert_eq!(
            reveal(&mut ctx, 1, secret(9)),
            Err(Refusal::Dice(DiceRefusal::WrongSecret))
        );
        reveal(&mut ctx, 1, secret(2)).unwrap();
        assert!(ctx.blob.roll.is_some(), "one reveal is not the roll");
        let before = ctx.effects.len();
        reveal(&mut ctx, 0, secret(1)).unwrap();
        assert!(ctx.blob.roll.is_none());
        assert!(ctx
            .blob
            .prompt
            .as_ref()
            .is_some_and(|prompt| prompt.seat == 1));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Mulligan));
        let sunk: Vec<u32> = ctx.effects[before..]
            .iter()
            .map(|effect| match effect {
                Effect::Move {
                    card, zone, index, ..
                } => {
                    assert_eq!((*zone, *index), (fixtures::MAIN_DECK, BOTTOM));
                    *card
                }
                other => panic!("only sinks: {other:?}"),
            })
            .collect();
        let mut expected = vec![fixtures::HAND_UNIT, fixtures::HAND_SPELL];
        expected.sort_unstable();
        let mut seen = sunk.clone();
        seen.sort_unstable();
        assert_eq!(seen, expected);
        let deck = deck_of(&ctx, 0);
        assert_eq!(&deck[2..], [20, 21]);
        assert_eq!(deck[0], sunk[1]);
        assert_eq!(deck[1], sunk[0]);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("the roll orders the deck bottom")));
        assert_eq!(
            commit(&mut ctx, 0, commitment(&secret(1))),
            Err(Refusal::NoRoll)
        );
    }

    #[test]
    fn the_roll_orders_replicas_identically_and_differs_with_the_secrets() {
        let run = |secrets: [[u8; SECRET_LEN]; 2]| {
            let mut fixture = fresh();
            let mut ctx = fixture.ctx();
            setup(&mut ctx);
            ctx.blob.close_prompt();
            finish_mulligan(&mut ctx, 0, &[fixtures::HAND_UNIT, fixtures::HAND_SPELL]);
            commit(&mut ctx, 0, commitment(&secrets[0])).unwrap();
            commit(&mut ctx, 1, commitment(&secrets[1])).unwrap();
            reveal(&mut ctx, 0, secrets[0]).unwrap();
            reveal(&mut ctx, 1, secrets[1]).unwrap();
            (deck_of(&ctx, 0), ctx.effects.clone(), ctx.blob.clone())
        };
        let a = run([secret(3), secret(4)]);
        let b = run([secret(3), secret(4)]);
        assert_eq!(
            a, b,
            "two replicas fed the same entries agree byte for byte"
        );
        let flipped = (0..40u8)
            .map(|seed| run([secret(seed), secret(4)]))
            .find(|other| other.0 != a.0)
            .expect("some secret flips the order");
        assert_eq!(&flipped.0[2..], &a.0[2..]);
    }

    #[test]
    fn a_one_card_mulligan_needs_no_roll_and_a_leaving_seat_abandons_an_open_one() {
        let mut fixture = fresh();
        let mut ctx = fixture.ctx();
        setup(&mut ctx);
        ctx.blob.close_prompt();
        finish_mulligan(&mut ctx, 0, &[fixtures::HAND_UNIT]);
        assert!(ctx.blob.roll.is_none());
        assert_eq!(ctx.blob.why, Some(PromptWhy::Mulligan));
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        ctx.blob.close_prompt();
        finish_mulligan(
            &mut ctx,
            1,
            &[fixtures::THEIR_HAND_CARD, fixtures::THEIR_UNIT],
        );
        assert!(
            ctx.blob.roll.is_none(),
            "one of the two named cards is not in the hand"
        );
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        let mut fixture = fresh();
        let mut ctx = fixture.ctx();
        setup(&mut ctx);
        ctx.blob.close_prompt();
        finish_mulligan(&mut ctx, 0, &[fixtures::HAND_UNIT, fixtures::HAND_SPELL]);
        assert!(is_open(&ctx));
        assert_eq!(
            crate::engine::phases::end_turn(&mut ctx),
            Err(Refusal::PromptOpen)
        );
        abandon(&mut ctx);
        assert!(!is_open(&ctx));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Mulligan));
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        abandon(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Mulligan));
    }
}
