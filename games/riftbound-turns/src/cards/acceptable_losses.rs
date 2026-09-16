use super::prelude::{asking, done, play, spell, with_candidates};
use super::{Card, Flow, Item, Keyword, Stage, KIND_GEAR};
use crate::engine::ctx::{Cause, Ctx};
use crate::state::TargetRef;

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

fn gear_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.faces_on_board()
        .filter(|card| card.is_kind(KIND_GEAR))
        .map(|card| card.id)
        .filter(|gear| ctx.controller(*gear) == seat && !ctx.is_facedown(*gear))
        .collect()
}

fn seat_at(ctx: &Ctx, stage: Stage) -> Option<u8> {
    let index = usize::from(stage.0).checked_sub(1)?;
    seats_in_turn_order(ctx).get(index).copied()
}

fn their_gear(ctx: &Ctx, _: &Item, stage: Stage) -> Vec<TargetRef> {
    seat_at(ctx, stage)
        .map(|seat| gear_of(ctx, seat))
        .unwrap_or_default()
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn each_player(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seats = seats_in_turn_order(ctx);
    let mut next = usize::from(stage.0);
    if let Some(chooser) = seat_at(ctx, stage) {
        if let Some(gear) = ctx.picks().first().copied() {
            if gear_of(ctx, chooser).contains(&gear) {
                ctx.narrate(format!("{{seat {chooser}}} kills {{card {gear}}}"));
                ctx.kill(gear, Cause::Item(item.id));
            }
        }
    }
    while let Some(seat) = seats.get(next).copied() {
        next += 1;
        if gear_of(ctx, seat).is_empty() {
            ctx.narrate(format!("{{seat {seat}}} has no gear to kill"));
            continue;
        }
        let stage = u8::try_from(next).unwrap_or(u8::MAX);
        return Flow::Ask(ctx.ask_seat_resume(item, seat, stage, 1, 1));
    }
    done()
}

pub static CARD: Card = spell(
    "Acceptable Losses",
    &[Keyword::Action],
    &[asking(
        with_candidates(play(&[], each_player), their_gear),
        "one of your gear to kill",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, deathknell, gear};
    use crate::cards::Trigger;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const LOSSES: u32 = 90;
    const MY_GEAR: u32 = 91;
    const MY_SPARE: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const THEIR_SPARE: u32 = 94;

    static MOURNED: Card = gear(
        "Mourned",
        &[Keyword::Deathknell],
        &[deathknell(&[], |ctx, item, _| {
            prelude::draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    fn losses(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Acceptable Losses", 1, 0);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(losses(LOSSES, 0));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Mourned", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Mourned", 1));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(MY_GEAR, &MOURNED)
            .with_script(THEIR_GEAR, &MOURNED);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LOSSES).unwrap(),
            &CARD
        ));
        fixture
    }

    fn with_spares(mut fixture: Fixture) -> Fixture {
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_SPARE, fixtures::BASE, 0, "Spare", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_SPARE, fixtures::BASE, 1, "Spare", 1));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(MY_GEAR, &MOURNED)
            .with_script(THEIR_GEAR, &MOURNED);
        fixture
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, LOSSES).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "355.10.e · the spell targets nothing"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_spell_is_a_targetless_action_that_asks_each_seat_in_turn() {
        assert_eq!(CARD.name, "Acceptable Losses");
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some("one of your gear to kill"));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(seats_in_turn_order(&ctx), [0, 1]);
        assert_eq!(gear_of(&ctx, 0), [MY_GEAR]);
        assert_eq!(gear_of(&ctx, 1), [THEIR_GEAR]);
        assert_eq!(seat_at(&ctx, Stage(0)), None);
        assert_eq!(seat_at(&ctx, Stage(1)), Some(0));
        assert_eq!(seat_at(&ctx, Stage(2)), Some(1));
        assert_eq!(seat_at(&ctx, Stage(3)), None);
    }

    #[test]
    fn each_seat_picks_from_its_own_gear_turn_player_first_and_the_deathknells_queue_after() {
        let mut fixture = with_spares(armed());
        let mut ctx = fixture.ctx();
        let my_hand = ctx.hand_of(0).len();
        let their_hand = ctx.hand_of(1).len();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {MY_GEAR}}}"),
                format!("{{card {MY_SPARE}}}")
            ],
            "303.2.a · the turn player chooses first, from their own gear"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 1 }),
            format!("{{card {LOSSES}}}: choose one of your gear to kill (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_GEAR}}}")).unwrap();
        assert_eq!(
            ctx.card(MY_GEAR).unwrap().zone,
            Some(fixtures::TRASH),
            "the first kill is applied before the next seat chooses"
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 2 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 1, "the other seat answers its own prompt");
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_GEAR}}}"),
                format!("{{card {THEIR_SPARE}}}")
            ]
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_SPARE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.card(THEIR_SPARE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(THEIR_GEAR));
        assert!(ctx.on_board(MY_SPARE));
        assert_eq!(
            ctx.card(LOSSES).unwrap().zone,
            Some(fixtures::TRASH),
            "the spell finished"
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "one Deathknell waits: only Mourned has one"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MY_GEAR
        ));
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, unit: false, .. } if *card == MY_GEAR)
        ));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.hand_of(0).len(),
            my_hand - 1 + 1,
            "the spell left, the Deathknell drew"
        );
        assert_eq!(ctx.hand_of(1).len(), their_hand);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} kills {{card {MY_GEAR}}}")));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 1}} kills {{card {THEIR_SPARE}}}")));
    }

    #[test]
    fn a_seat_with_one_gear_answers_unasked_and_both_deathknells_are_ordered_by_their_controllers()
    {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one gear each: nothing to choose"
        );
        assert_eq!(ctx.card(MY_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "both Deathknells reached the chain"
        );
        let sources: Vec<u32> = ctx
            .blob
            .chain
            .iter()
            .map(|item| item.kind.source())
            .collect();
        assert!(sources.contains(&MY_GEAR) && sources.contains(&THEIR_GEAR));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_seat_without_gear_is_skipped_and_an_attached_gear_counts_as_its_controllers() {
        let mut fixture = with_spares(armed());
        fixture
            .table
            .cards
            .retain(|card| ![THEIR_GEAR, THEIR_SPARE].contains(&card.id));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(MY_GEAR, &MOURNED);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_SPARE}}}")).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "055 · seat 1 has nothing to kill and is not asked"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no gear to kill".to_string()));
        assert_eq!(ctx.card(MY_SPARE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(MY_GEAR));
        let mut worn = armed();
        worn.table.cards.retain(|card| card.id != MY_GEAR);
        worn.resolve();
        worn.scripts = worn.scripts.clone().with_script(THEIR_GEAR, &MOURNED);
        let mut ctx = worn.ctx();
        assert_eq!(
            ctx.attach(THEIR_GEAR, fixtures::THEIR_UNIT),
            prelude::Attached::Yes
        );
        cast(&mut ctx);
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.card(THEIR_GEAR).unwrap().zone,
            Some(fixtures::TRASH),
            "gear attached to a unit is still that seat's gear"
        );
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
    }

    #[test]
    fn a_stale_pick_from_the_other_seat_is_refused_and_the_spell_is_the_turn_players_only() {
        let mut fixture = with_spares(armed());
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the first prompt is the turn player's"
        );
        assert_eq!(priority::pass(&mut ctx, 1), Err(Refusal::PromptOpen));
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_SPARE}}}")).unwrap();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 })),
            "and the second is the other seat's"
        );
        assert!(ctx.on_board(THEIR_GEAR) && ctx.on_board(THEIR_SPARE));
        drop(ctx);
        let mut theirs = armed();
        theirs.table.cards.push(losses(95, 1));
        theirs.resolve();
        let ctx = theirs.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: 95,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::NotYourTurn),
            "an Action on the opponent's turn outside a showdown"
        );
    }
}
