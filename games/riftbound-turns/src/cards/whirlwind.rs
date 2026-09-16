use super::prelude::{asking, bounce, done, play, spell, units_on_board, with_candidates};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const QUESTION: &str = "a unit to return to its owner's hand";

pub fn seats_from_the_next_player(ctx: &Ctx) -> Vec<u8> {
    let order = ctx.blob.order();
    let mut seats = Vec::new();
    let mut seat = order.next_seat(ctx.turn_player());
    for _ in 0..ctx.players() {
        seats.push(seat);
        seat = order.next_seat(seat);
    }
    seats
}

pub fn seat_at(ctx: &Ctx, stage: Stage) -> Option<u8> {
    let index = usize::from(stage.0).checked_sub(1)?;
    seats_from_the_next_player(ctx).get(index).copied()
}

fn units(ctx: &Ctx) -> Vec<u32> {
    units_on_board(ctx)
        .into_iter()
        .filter(|unit| !ctx.is_facedown(*unit))
        .collect()
}

fn any_unit(ctx: &Ctx, _: &Item, stage: Stage) -> Vec<TargetRef> {
    if seat_at(ctx, stage).is_none() {
        return Vec::new();
    }
    units(ctx).into_iter().map(TargetRef::Card).collect()
}

fn each_player(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seats = seats_from_the_next_player(ctx);
    let mut next = usize::from(stage.0);
    if let Some(chooser) = seat_at(ctx, stage) {
        match ctx.picks().first().copied() {
            Some(unit) if units(ctx).contains(&unit) => {
                ctx.narrate(format!("{{seat {chooser}}} returns {{card {unit}}}"));
                bounce(ctx, unit);
            }
            _ => ctx.narrate(format!("{{seat {chooser}}} returns nothing")),
        }
    }
    while let Some(seat) = seats.get(next).copied() {
        next += 1;
        if units(ctx).is_empty() {
            ctx.narrate(format!("{{seat {seat}}} has no unit to return"));
            continue;
        }
        let stage = u8::try_from(next).unwrap_or(u8::MAX);
        return Flow::Ask(ctx.ask_seat_resume(item, seat, stage, 0, 1));
    }
    done()
}

pub static CARD: Card = spell(
    "Whirlwind",
    &[],
    &[asking(
        with_candidates(play(&[], each_player), any_unit),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const WHIRLWIND: u32 = 90;
    const THEIR_SECOND: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut whirlwind = fixtures::spell(WHIRLWIND, fixtures::HAND, 0, "Whirlwind", 0, 0);
        whirlwind.domain = vec!["Chaos".into()];
        fixture.table.cards.push(whirlwind);
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SECOND, fixtures::BASE, 1, "Nobody", 1));
        fixture.resolve();
        fixture
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, WHIRLWIND).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.e · nothing is targeted");
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_targetless_sorcery_that_walks_the_seats_from_the_next_player() {
        assert!(std::ptr::eq(script_of("Whirlwind").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(seats_from_the_next_player(&ctx), [1, 0]);
        assert_eq!(seat_at(&ctx, Stage(0)), None);
        assert_eq!(seat_at(&ctx, Stage(1)), Some(1));
        assert_eq!(seat_at(&ctx, Stage(2)), Some(0));
        assert_eq!(seat_at(&ctx, Stage(3)), None);
    }

    #[test]
    fn the_next_player_chooses_first_from_every_unit_and_each_may_skip() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (1, 0, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_SECOND}}}"),
                "skip".to_string()
            ],
            "every unit on the board, mine included"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 1 }),
            format!("{{seat 1}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::HAND),
            "returned before the next seat chooses"
        );
        assert_eq!(ctx.card(fixtures::VI).unwrap().seat, 0);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 2 }));
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_SECOND}}}"),
                "skip".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.on_board(THEIR_SECOND));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 1}} returns {{card {}}}", fixtures::VI)));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} returns nothing".to_string()));
        assert_eq!(ctx.card(WHIRLWIND).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_returned_token_vanishes_and_an_empty_board_asks_nobody() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "the token ceases to exist"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SECOND}}}")).unwrap();
        assert_eq!(ctx.card(THEIR_SECOND).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(THEIR_SECOND).unwrap().seat, 1);
        let mut fixture = armed();
        fixture.table.cards.retain(|card| {
            ![
                fixtures::VI,
                fixtures::SPRITE,
                fixtures::THEIR_UNIT,
                THEIR_SECOND,
            ]
            .contains(&card.id)
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no unit to return".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_turn_player_cannot_answer_the_next_players_prompt() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 }))
        );
        assert_eq!(priority::pass(&mut ctx, 0), Err(Refusal::PromptOpen));
        assert!(ctx.on_board(fixtures::VI));
    }
}
