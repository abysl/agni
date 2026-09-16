use super::prelude::{
    asking, done, friendly_units, play, remember_card, remembered_cards, spell, units_on_board,
    with_candidates,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::kill;
use crate::state::TargetRef;

pub const QUESTION: &str = "one of your units to keep";

pub fn seats_in_turn_order(ctx: &Ctx) -> Vec<u8> {
    let order = ctx.blob.order();
    let mut seats = Vec::new();
    let mut seat = ctx.turn_player();
    for _ in 0..ctx.players() {
        seats.push(seat);
        seat = order.next_seat(seat);
    }
    seats
}

pub fn units_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| !ctx.is_facedown(*unit))
        .collect()
}

pub fn the_rest(ctx: &Ctx, kept: &[u32]) -> Vec<u32> {
    let mut rest: Vec<u32> = units_on_board(ctx)
        .into_iter()
        .filter(|unit| !ctx.is_facedown(*unit) && !kept.contains(unit))
        .collect();
    rest.sort_unstable();
    rest
}

fn seat_at(ctx: &Ctx, stage: Stage) -> Option<u8> {
    let index = usize::from(stage.0).checked_sub(1)?;
    seats_in_turn_order(ctx).get(index).copied()
}

fn their_units(ctx: &Ctx, _: &Item, stage: Stage) -> Vec<TargetRef> {
    seat_at(ctx, stage)
        .map(|seat| units_of(ctx, seat))
        .unwrap_or_default()
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn duel(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seats = seats_in_turn_order(ctx);
    let mut next = usize::from(stage.0);
    let mut kept = remembered_cards(item);
    if let Some(chooser) = seat_at(ctx, stage) {
        if let Some(unit) = ctx.picks().first().copied() {
            if units_of(ctx, chooser).contains(&unit) {
                ctx.narrate(format!("{{seat {chooser}}} keeps {{card {unit}}}"));
                remember_card(ctx, unit);
                kept.push(unit);
            }
        }
    }
    while let Some(seat) = seats.get(next).copied() {
        next += 1;
        if units_of(ctx, seat).is_empty() {
            ctx.narrate(format!("{{seat {seat}}} has no unit to keep"));
            continue;
        }
        let stage = u8::try_from(next).unwrap_or(u8::MAX);
        return Flow::Ask(ctx.ask_seat_resume(item, seat, stage, 1, 1));
    }
    let rest = the_rest(ctx, &kept);
    if rest.is_empty() {
        ctx.narrate(format!(
            "{{card {}}} finds no other unit",
            item.kind.source()
        ));
        return done();
    }
    ctx.narrate("the rest are killed together".to_string());
    let dead = kill::batch(ctx, &rest, Cause::Item(item.id));
    for unit in dead {
        ctx.narrate(format!("{{card {unit}}} dies"));
    }
    done()
}

pub static CARD: Card = spell(
    "Cataclysmic Duel",
    &[],
    &[asking(
        with_candidates(play(&[], duel), their_units),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{deathknell, draw, unit};
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::engine::{priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const DUEL: u32 = 90;
    const THEIR_DUEL: u32 = 91;
    const MOURNED: u32 = 92;
    const BRUTE: u32 = 93;
    const MY_GEAR: u32 = 94;
    const BODY_RUNES: [u32; 3] = [100, 101, 102];
    const MORE_RUNES: [u32; 2] = [103, 104];

    static MOURNED_CARD: Card = unit(
        "Mourned",
        &[Keyword::Deathknell],
        &[deathknell(&[], |ctx, item, _| {
            draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    fn duel_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Cataclysmic Duel", 8, 3);
        card.domain = vec!["Body".into()];
        card
    }

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(duel_card(DUEL, 0));
        fixture.table.cards.push(duel_card(THEIR_DUEL, 1));
        for rune in BODY_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        for rune in MORE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(MOURNED, fixtures::BF1, 0, "Mourned", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(MOURNED, &MOURNED_CARD);
        assert!(std::ptr::eq(fixture.scripts.of_card(DUEL).unwrap(), &CARD));
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, DUEL).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "355.10.e · the spell targets nothing"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_spell_is_targetless_and_asks_each_seat_in_turn_order_for_one_of_its_units() {
        assert!(std::ptr::eq(script_of("Cataclysmic Duel").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = arena();
        let ctx = fixture.ctx();
        assert_eq!(seats_in_turn_order(&ctx), [0, 1]);
        assert_eq!(units_of(&ctx, 0), [fixtures::VI, MOURNED]);
        assert_eq!(
            units_of(&ctx, 1),
            [fixtures::SPRITE, fixtures::THEIR_UNIT, BRUTE]
        );
        assert_eq!(
            the_rest(&ctx, &[fixtures::VI, BRUTE]),
            [fixtures::SPRITE, fixtures::THEIR_UNIT, MOURNED],
            "every other unit, mine and theirs, in id order"
        );
        assert_eq!(seat_at(&ctx, Stage(0)), None);
        assert_eq!(seat_at(&ctx, Stage(1)), Some(0));
        assert_eq!(seat_at(&ctx, Stage(2)), Some(1));
        assert_eq!(seat_at(&ctx, Stage(3)), None);
    }

    #[test]
    fn each_seat_keeps_one_of_its_own_turn_player_first_and_the_rest_die_together() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        let my_hand = ctx.hand_of(0).len();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 92}"],
            "303.2.a · the turn player chooses first, from their own units"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 1 }),
            "{card 90}: choose one of your units to keep (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(ctx.on_board(MOURNED), "the kills wait for every choice");
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 2 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 1, "the other seat answers its own prompt");
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", &format!("{{card {BRUTE}}}")]
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 2 }),
            "{seat 1}: choose one of your units to keep (0 of 1)"
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.on_board(fixtures::VI), "kept");
        assert!(ctx.on_board(BRUTE), "kept");
        assert!(ctx.card(fixtures::SPRITE).is_none(), "the token vanishes");
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.card(MOURNED).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(MY_GEAR), "gear is not a unit");
        assert_eq!(ctx.card(DUEL).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.chain.len(), 1, "one Deathknell waits");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MOURNED
        ));
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, unit: true, .. } if *card == MOURNED)
        ));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.hand_of(0).len(),
            my_hand - 1 + 1,
            "the spell left, the Deathknell drew"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} keeps {card 50}".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 1}} keeps {{card {BRUTE}}}")));
        assert!(ctx
            .blob
            .log
            .contains(&"the rest are killed together".to_string()));
        assert!(ctx.blob.log.contains(&format!("{{card {MOURNED}}} dies")));
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(1),
            "the Brute holds the battlefield the Mourned left"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_seat_with_one_unit_keeps_it_unasked_and_a_seat_with_none_is_skipped() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .retain(|card| ![MOURNED, fixtures::SPRITE, BRUTE].contains(&card.id));
        fixture.table.tokens.clear();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one unit each: nothing to choose"
        );
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {DUEL}}} finds no other unit")));
        drop(ctx);

        let mut lone = arena();
        lone.table
            .cards
            .retain(|card| ![fixtures::SPRITE, fixtures::THEIR_UNIT, BRUTE].contains(&card.id));
        lone.table.tokens.clear();
        lone.resolve();
        lone.scripts = lone.scripts.clone().with_script(MOURNED, &MOURNED_CARD);
        let mut ctx = lone.ctx();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "seat 1 has nothing to keep and is not asked"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no unit to keep".to_string()));
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(MOURNED));
    }

    #[test]
    fn a_stolen_unit_is_kept_or_lost_by_its_controller_not_its_owner() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, fixtures::VI));
        assert_eq!(
            units_of(&ctx, 0),
            [fixtures::VI, MOURNED, fixtures::THEIR_UNIT],
            "the stolen unit sits on top of the thief's base"
        );
        cast(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 92}", "{card 81}"],
            "the stolen unit is one of the thief's"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 1, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.on_board(fixtures::THEIR_UNIT), "kept by its controller");
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.card(fixtures::SPRITE).is_none());
    }

    #[test]
    fn a_shrouded_unit_is_offered_to_its_own_controller_and_a_seat_of_only_shrouded_units_is_not_stuck(
    ) {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        assert!(ctx.shroud(fixtures::THEIR_UNIT));
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 2 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", &format!("{{card {BRUTE}}}")],
            "355.10.e · a unit its own controller chooses is not a target, so Shroud does not hide it"
        );
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        assert!(ctx.on_board(fixtures::THEIR_UNIT), "kept");
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::TRASH));
        drop(ctx);

        let mut lone = arena();
        lone.table
            .cards
            .retain(|card| ![fixtures::SPRITE, BRUTE].contains(&card.id));
        lone.table.tokens.clear();
        lone.resolve();
        lone.scripts = lone.scripts.clone().with_script(MOURNED, &MOURNED_CARD);
        let mut ctx = lone.ctx();
        assert!(ctx.shroud(fixtures::THEIR_UNIT));
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one shrouded unit: kept unasked, no empty mandatory prompt"
        );
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert_eq!(ctx.card(MOURNED).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.chain.len() == 1, "the Deathknell waits");
    }

    #[test]
    fn a_pick_from_the_wrong_seat_is_refused_and_the_spell_is_the_turn_players_only() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DUEL)),
            Err(Refusal::NotYourTurn)
        );
        cast(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the first prompt is the turn player's"
        );
        assert_eq!(priority::pass(&mut ctx, 1), Err(Refusal::PromptOpen));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 })),
            "the second prompt is the other seat's"
        );
        assert!(
            ctx.on_board(MOURNED),
            "nothing dies until every seat has kept"
        );
    }
}
