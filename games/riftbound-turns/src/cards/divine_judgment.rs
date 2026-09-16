use super::prelude::{
    asking, detach_all, detach_gear, done, friendly_gear, friendly_units, is_attached, play, spell,
    with_candidates,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::Effect;

pub const QUESTION: &str = "two to keep · the rest are recycled";
pub const KEEP: u8 = 2;
const STEPS: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Units,
    Gear,
    Runes,
    Hand,
}

impl Step {
    fn of(index: u8) -> Option<Step> {
        match index {
            0 => Some(Step::Units),
            1 => Some(Step::Gear),
            2 => Some(Step::Runes),
            3 => Some(Step::Hand),
            _ => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Step::Units => "units",
            Step::Gear => "gear",
            Step::Runes => "runes",
            Step::Hand => "cards in hand",
        }
    }
}

fn seats_in_turn_order(ctx: &Ctx) -> Vec<u8> {
    let order = ctx.blob.order();
    let mut seats = Vec::new();
    let mut seat = ctx.turn_player();
    for _ in 0..ctx.players() {
        seats.push(seat);
        seat = order.next_seat(seat);
    }
    seats
}

pub fn held(ctx: &Ctx, seat: u8, step: Step) -> Vec<u32> {
    match step {
        Step::Units => friendly_units(ctx, seat)
            .into_iter()
            .filter(|unit| !ctx.is_facedown(*unit))
            .collect(),
        Step::Gear => friendly_gear(ctx, seat)
            .into_iter()
            .filter(|gear| !ctx.is_facedown(*gear))
            .collect(),
        Step::Runes => ctx.runes_of(seat).into_iter().map(|rune| rune.id).collect(),
        Step::Hand => ctx.hand_of(seat),
    }
}

fn stage_of(seat_index: usize, step: u8) -> u8 {
    u8::try_from(seat_index)
        .ok()
        .and_then(|index| index.checked_mul(STEPS))
        .and_then(|base| base.checked_add(step))
        .and_then(|value| value.checked_add(1))
        .unwrap_or(u8::MAX)
}

fn decode(ctx: &Ctx, stage: Stage) -> Option<(u8, Step)> {
    let index = stage.0.checked_sub(1)?;
    let seat = seats_in_turn_order(ctx)
        .get(usize::from(index / STEPS))
        .copied()?;
    Some((seat, Step::of(index % STEPS)?))
}

fn to_keep(ctx: &Ctx, _: &Item, stage: Stage) -> Vec<TargetRef> {
    decode(ctx, stage)
        .map(|(seat, step)| held(ctx, seat, step))
        .unwrap_or_default()
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub fn recycle(ctx: &mut Ctx, card: u32) {
    if ctx.is_token(card) {
        ctx.emit(Effect::Despawn { card });
        ctx.blob.drop_card_state(card);
        return;
    }
    if ctx.on_board(card) {
        detach_all(ctx, card);
        if is_attached(ctx, card) {
            detach_gear(ctx, card);
        }
        ctx.clear_designation(card);
    }
    ctx.recycle_to_bottom(card);
    ctx.blob.drop_card_state(card);
}

fn judge(ctx: &mut Ctx, seat: u8, step: Step, kept: &[u32]) {
    let all = held(ctx, seat, step);
    let recycled: Vec<u32> = all
        .iter()
        .copied()
        .filter(|card| !kept.contains(card))
        .collect();
    for card in &recycled {
        recycle(ctx, *card);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} keeps {} of their {} · recycles {}",
        all.len() - recycled.len(),
        step.label(),
        recycled.len()
    ));
}

fn each_player(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seats = seats_in_turn_order(ctx);
    let mut position = 0usize;
    if let Some((seat, step)) = decode(ctx, stage) {
        let kept: Vec<u32> = ctx
            .picks()
            .iter()
            .copied()
            .filter(|card| held(ctx, seat, step).contains(card))
            .collect();
        judge(ctx, seat, step, &kept);
        position = usize::from(stage.0);
    }
    while let Some(seat) = seats.get(position / usize::from(STEPS)).copied() {
        let step_index = u8::try_from(position % usize::from(STEPS)).unwrap_or(u8::MAX);
        let Some(step) = Step::of(step_index) else {
            break;
        };
        position += 1;
        let cards = held(ctx, seat, step);
        if cards.len() <= usize::from(KEEP) {
            judge(ctx, seat, step, &cards);
            continue;
        }
        let stage = stage_of((position - 1) / usize::from(STEPS), step_index);
        return Flow::Ask(ctx.ask_seat_resume(item, seat, stage, KEEP, KEEP));
    }
    done()
}

pub static CARD: Card = spell(
    "Divine Judgment",
    &[],
    &[asking(
        with_candidates(play(&[], each_player), to_keep),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::Location;
    use crate::cards::script_of;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::prompts;
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const JUDGMENT: u32 = 90;
    const MY_SECOND: u32 = 91;
    const MY_THIRD: u32 = 92;
    const MY_GEAR: u32 = 93;
    const THEIR_SECOND: u32 = 94;
    const THEIR_THIRD: u32 = 95;
    const THEIR_GEAR_A: u32 = 96;
    const THEIR_GEAR_B: u32 = 97;
    const THEIR_GEAR_C: u32 = 98;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut judgment = fixtures::spell(JUDGMENT, fixtures::HAND, 0, "Divine Judgment", 0, 0);
        judgment.domain = vec!["Order".into()];
        fixture.table.cards.push(judgment);
        fixture
            .table
            .cards
            .push(fixtures::unit(MY_SECOND, fixtures::BF1, 0, "Second", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(MY_THIRD, fixtures::BASE, 0, "Third", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Boots", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SECOND, fixtures::BASE, 1, "Nobody", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_THIRD, fixtures::BASE, 1, "Anybody", 1));
        for (id, name) in [
            (THEIR_GEAR_A, "Hat"),
            (THEIR_GEAR_B, "Coat"),
            (THEIR_GEAR_C, "Sword"),
        ] {
            fixture
                .table
                .cards
                .push(fixtures::gear(id, fixtures::BASE, 1, name, 1));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, JUDGMENT).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.e · nothing is targeted");
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_a_targetless_sorcery_whose_stages_encode_seat_and_step() {
        assert!(std::ptr::eq(script_of("Divine Judgment").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(CARD.abilities[0].question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(decode(&ctx, Stage(0)), None);
        assert_eq!(decode(&ctx, Stage(1)), Some((0, Step::Units)));
        assert_eq!(decode(&ctx, Stage(4)), Some((0, Step::Hand)));
        assert_eq!(decode(&ctx, Stage(5)), Some((1, Step::Units)));
        assert_eq!(decode(&ctx, Stage(8)), Some((1, Step::Hand)));
        assert_eq!(decode(&ctx, Stage(9)), None);
        assert_eq!(stage_of(1, 2), 7);
        assert_eq!(
            held(&ctx, 0, Step::Units),
            [fixtures::VI, MY_SECOND, MY_THIRD]
        );
        assert_eq!(held(&ctx, 0, Step::Gear), [MY_GEAR]);
        assert_eq!(held(&ctx, 0, Step::Runes), [40, 41, 42, 43]);
        assert_eq!(held(&ctx, 1, Step::Hand), [fixtures::THEIR_HAND_CARD]);
    }

    #[test]
    fn each_seat_keeps_two_of_each_kind_in_turn_order_and_the_rest_is_recycled() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 2, 2));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {MY_SECOND}}}"),
                format!("{{card {MY_THIRD}}}")
            ],
            "three units, two to keep, nothing to skip"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_SECOND}}}")).unwrap();
        assert_eq!(
            ctx.card(MY_THIRD).unwrap().zone,
            Some(fixtures::MAIN_DECK),
            "the second pick fills the prompt, the lone done answers itself, the third unit is recycled under the deck"
        );
        assert!(ctx.on_board(fixtures::VI) && ctx.on_board(MY_SECOND));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume { item, stage: 3 }),
            "one gear and the hand of four: only the runes ask"
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 40}", "{card 41}", "{card 42}", "{card 43}"]
        );
        fixtures::choose(&mut ctx, 0, "{card 42}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 43}").unwrap();
        assert_eq!(ctx.runes_of(0).len(), 2);
        assert_eq!(ctx.card(40).unwrap().zone, Some(fixtures::RUNE_DECK));
        assert_eq!(ctx.card(41).unwrap().zone, Some(fixtures::RUNE_DECK));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume { item, stage: 4 }),
            "then my hand of four"
        );
        let hand = ctx.hand_of(0);
        assert_eq!(hand.len(), 4);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", hand[0])).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", hand[1])).unwrap();
        assert_eq!(ctx.hand_of(0), [hand[0], hand[1]]);
        assert_eq!(ctx.card(hand[2]).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume { item, stage: 5 }),
            "the other seat's three units"
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 1);
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 5 }),
            format!("{{seat 1}}: choose {QUESTION} (0 of 2)")
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_THIRD}}}")).unwrap();
        assert_eq!(
            ctx.card(THEIR_SECOND).unwrap().zone,
            Some(fixtures::MAIN_DECK)
        );
        assert_eq!(ctx.card(THEIR_SECOND).unwrap().seat, 1);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 6 }));
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_GEAR_A}}}")).unwrap();
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_GEAR_C}}}")).unwrap();
        assert_eq!(
            ctx.card(THEIR_GEAR_B).unwrap().zone,
            Some(fixtures::MAIN_DECK)
        );
        assert!(
            ctx.blob.prompt.is_none(),
            "two runes and one card in hand: nothing more to choose"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.runes_of(1).len(), 2);
        assert_eq!(ctx.hand_of(1), [fixtures::THEIR_HAND_CARD]);
        assert_eq!(ctx.card(JUDGMENT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} keeps 2 of their units · recycles 1".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} keeps 1 of their cards in hand · recycles 0".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_recycled_token_vanishes_and_gear_worn_by_a_recycled_unit_comes_off() {
        let mut fixture = armed();
        fixture.table.card_mut(MY_GEAR).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.attach(MY_GEAR, MY_SECOND),
            crate::cards::prelude::Attached::Yes
        );
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_THIRD}}}")).unwrap();
        assert_eq!(ctx.card(MY_SECOND).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert!(!is_attached(&ctx, MY_GEAR), "the gear is detached");
        assert!(ctx.on_board(MY_GEAR));
        fixtures::choose(&mut ctx, 0, "{card 42}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 43}").unwrap();
        let hand = ctx.hand_of(0);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", hand[0])).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", hand[1])).unwrap();
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_SECOND}}}")).unwrap();
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_THIRD}}}")).unwrap();
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "the token ceases to exist"
        );
        assert_eq!(ctx.location(THEIR_SECOND), Some(Location::Base(1)));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_seat_with_two_or_fewer_of_everything_is_never_asked_and_a_stranger_cannot_answer() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![MY_THIRD, 40, 41].contains(&card.id));
        fixture.table.cards.retain(|card| {
            card.zone != Some(fixtures::HAND) || card.seat != 0 || card.id == JUDGMENT
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume { item, stage: 5 }),
            "seat 0 keeps everything unasked; seat 1's units ask"
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 }))
        );
        assert!(ctx.on_board(fixtures::VI) && ctx.on_board(MY_SECOND));
        assert_eq!(ctx.runes_of(0).len(), 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} keeps 2 of their units · recycles 0".to_string()));
    }
}
