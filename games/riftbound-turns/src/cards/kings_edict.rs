use super::prelude::{
    asking, done, play, remember_card, remembered_cards, spell, units_on_board, with_candidates,
};
use super::whirlwind::seats_from_the_next_player;
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::kill;
use crate::state::TargetRef;

pub const QUESTION: &str = "a unit the spell's controller doesn't control to kill";

fn other_seats(ctx: &Ctx, item: &Item) -> Vec<u8> {
    seats_from_the_next_player(ctx)
        .into_iter()
        .filter(|seat| *seat != item.controller)
        .collect()
}

fn seat_at(ctx: &Ctx, item: &Item, stage: Stage) -> Option<u8> {
    let index = usize::from(stage.0).checked_sub(1)?;
    other_seats(ctx, item).get(index).copied()
}

fn not_yet_chosen(ctx: &Ctx, item: &Item) -> Vec<u32> {
    let chosen = remembered_cards(item);
    units_on_board(ctx)
        .into_iter()
        .filter(|unit| {
            ctx.controller(*unit) != item.controller
                && !ctx.is_facedown(*unit)
                && !chosen.contains(unit)
        })
        .collect()
}

fn their_pick(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if seat_at(ctx, item, stage).is_none() {
        return Vec::new();
    }
    not_yet_chosen(ctx, item)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn each_other_player(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seats = other_seats(ctx, item);
    let mut next = usize::from(stage.0);
    let mut chosen = remembered_cards(item);
    if let Some(chooser) = seat_at(ctx, item, stage) {
        if let Some(unit) = ctx.picks().first().copied() {
            if not_yet_chosen(ctx, item).contains(&unit) {
                ctx.narrate(format!("{{seat {chooser}}} chooses {{card {unit}}}"));
                remember_card(ctx, unit);
                chosen.push(unit);
            }
        }
    }
    while let Some(seat) = seats.get(next).copied() {
        next += 1;
        let open: Vec<u32> = not_yet_chosen(ctx, item)
            .into_iter()
            .filter(|unit| !chosen.contains(unit))
            .collect();
        if open.is_empty() {
            ctx.narrate(format!("{{seat {seat}}} has no unit to choose"));
            continue;
        }
        let stage = u8::try_from(next).unwrap_or(u8::MAX);
        return Flow::Ask(ctx.ask_seat_resume(item, seat, stage, 1, 1));
    }
    if !chosen.is_empty() {
        ctx.narrate("the chosen units are killed".to_string());
        kill::batch(ctx, &chosen, Cause::Item(item.id));
    }
    done()
}

pub static CARD: Card = spell(
    "King's Edict",
    &[],
    &[asking(
        with_candidates(play(&[], each_other_player), their_pick),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{deathknell, draw, unit};
    use crate::cards::script_of;
    use crate::cards::{Keyword, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{GameBlob, ItemKind, Mode, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const EDICT: u32 = 90;
    const THEIR_SECOND: u32 = 91;
    const THIRD_SEATS: u32 = 92;

    static MOURNED: Card = unit(
        "Mourned",
        &[Keyword::Deathknell],
        &[deathknell(&[], |ctx, item, _| {
            draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut edict = fixtures::spell(EDICT, fixtures::HAND, 0, "King's Edict", 0, 0);
        edict.domain = vec!["Order".into()];
        fixture.table.cards.push(edict);
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SECOND, fixtures::BF2, 1, "Mourned", 1));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(THEIR_SECOND, &MOURNED);
        fixture
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, EDICT).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.e · nothing is targeted");
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_targetless_sorcery_asking_only_the_other_players() {
        assert!(std::ptr::eq(script_of("King's Edict").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_opponent_chooses_among_units_i_do_not_control_and_the_choice_dies_with_its_deathknell() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let their_hand = ctx.hand_of(1).len();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (1, 1, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_SECOND}}}")
            ],
            "my own unit is never on the list and there is no skip"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 1 }),
            format!("{{seat 1}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_SECOND}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "one other player in a duel");
        assert_eq!(ctx.card(THEIR_SECOND).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, unit: true, .. } if *card == THEIR_SECOND
        )));
        assert_eq!(ctx.card(EDICT).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.chain.len(), 1, "the Deathknell waits");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == THEIR_SECOND
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(1).len(), their_hand + 1);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 1}} chooses {{card {THEIR_SECOND}}}")));
        assert!(ctx
            .blob
            .log
            .contains(&"the chosen units are killed".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_lone_candidate_is_chosen_unasked_and_no_candidate_asks_nobody() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id));
        fixture.table.tokens.clear();
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(THEIR_SECOND, &MOURNED);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.card(THEIR_SECOND).unwrap().zone, Some(fixtures::TRASH));
        let mut fixture = armed();
        fixture.table.cards.retain(|card| {
            ![fixtures::SPRITE, fixtures::THEIR_UNIT, THEIR_SECOND].contains(&card.id)
        });
        fixture.table.tokens.clear();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no unit to choose".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_spells_controller_cannot_answer_for_the_opponent() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 }))
        );
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 3 }),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 3,
                count: 3
            }))
        );
        assert!(ctx.on_board(THEIR_SECOND));
    }

    #[test]
    fn with_three_players_each_chooser_skips_the_units_already_chosen_and_the_kills_land_together()
    {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::unit(THIRD_SEATS, fixtures::BF1, 2, "Jinx", 2));
        fixture.table.players = 3;
        fixture.blob = GameBlob::start(3, 0, Mode::Enforced);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(THEIR_SECOND, &MOURNED);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EDICT).unwrap();
        for seat in 0..3 {
            if ctx.blob.prompt.is_some() {
                break;
            }
            priority::pass(&mut ctx, seat).unwrap();
        }
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_SECOND}}}"),
                format!("{{card {THIRD_SEATS}}}")
            ]
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_SECOND}}}")).unwrap();
        assert!(
            ctx.on_board(THEIR_SECOND),
            "nothing dies until every other player has chosen"
        );
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_SECOND)]);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 2 }));
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 2);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THIRD_SEATS}}}")
            ],
            "a unit already chosen for this spell is off the list"
        );
        fixtures::choose(&mut ctx, 2, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.card(THEIR_SECOND).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.card(fixtures::SPRITE).is_none(), "the token vanishes");
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.on_board(THIRD_SEATS));
        let died: Vec<u32> = ctx
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Died { card, .. } => Some(*card),
                _ => None,
            })
            .collect();
        assert_eq!(died, [THEIR_SECOND, fixtures::SPRITE]);
        assert!(ctx.blob.chain.iter().any(
            |held| matches!(held.kind, ItemKind::Trigger { source, index: 0 } if source == THEIR_SECOND)
        ));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }
}
