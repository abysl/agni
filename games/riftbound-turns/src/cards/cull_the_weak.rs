use super::prelude::{
    asking, done, friendly_units, play, remember_card, remembered_cards, spell, with_candidates,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::kill;
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

fn units_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| !ctx.is_facedown(*unit))
        .collect()
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

fn each_player(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seats = seats_in_turn_order(ctx);
    let mut next = usize::from(stage.0);
    let mut chosen = remembered_cards(item);
    if let Some(chooser) = seat_at(ctx, stage) {
        if let Some(unit) = ctx.picks().first().copied() {
            if units_of(ctx, chooser).contains(&unit) {
                ctx.narrate(format!("{{seat {chooser}}} chooses {{card {unit}}}"));
                remember_card(ctx, unit);
                chosen.push(unit);
            }
        }
    }
    while let Some(seat) = seats.get(next).copied() {
        next += 1;
        if units_of(ctx, seat).is_empty() {
            ctx.narrate(format!("{{seat {seat}}} has no unit to kill"));
            continue;
        }
        let stage = u8::try_from(next).unwrap_or(u8::MAX);
        return Flow::Ask(ctx.ask_seat_resume(item, seat, stage, 1, 1));
    }
    if !chosen.is_empty() {
        ctx.narrate("the chosen units are killed together".to_string());
        kill::batch(ctx, &chosen, Cause::Item(item.id));
    }
    done()
}

pub static CARD: Card = spell(
    "Cull the Weak",
    &[],
    &[asking(
        with_candidates(play(&[], each_player), their_units),
        "one of your units to kill",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, deathknell, unit};
    use crate::cards::{Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const CULL: u32 = 90;
    const THEIR_CULL: u32 = 91;
    const MOURNED: u32 = 92;
    const ORDER_RUNE: u32 = 100;

    static MOURNED_CARD: Card = unit(
        "Mourned",
        &[Keyword::Deathknell],
        &[deathknell(&[], |ctx, item, _| {
            prelude::draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    fn cull(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Cull the Weak", 2, 1);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(cull(CULL, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(MOURNED, fixtures::BF1, 0, "Mourned", 2));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(MOURNED, &MOURNED_CARD);
        assert!(std::ptr::eq(fixture.scripts.of_card(CULL).unwrap(), &CARD));
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
        fixtures::play_from_hand(ctx, 0, CULL).unwrap();
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
        assert_eq!(CARD.name, "Cull the Weak");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some("one of your units to kill"));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(seats_in_turn_order(&ctx), [0, 1]);
        assert_eq!(units_of(&ctx, 0), [fixtures::VI, MOURNED]);
        assert_eq!(units_of(&ctx, 1), [fixtures::SPRITE, fixtures::THEIR_UNIT]);
        assert_eq!(seat_at(&ctx, Stage(0)), None);
        assert_eq!(seat_at(&ctx, Stage(1)), Some(0));
        assert_eq!(seat_at(&ctx, Stage(2)), Some(1));
        assert_eq!(seat_at(&ctx, Stage(3)), None);
    }

    #[test]
    fn each_seat_picks_one_of_its_own_units_turn_player_first_and_the_deathknell_queues_after() {
        let mut fixture = armed();
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
            "303.2.a · the turn player chooses first, from their own units, in a base or at a battlefield"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 1 }),
            "{card 90}: choose one of your units to kill (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(
            ctx.on_board(MOURNED),
            "the first choice waits for the rest · the kills land together"
        );
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(MOURNED)]);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 2 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 1, "the other seat answers its own prompt");
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 81}"]);
        fixtures::choose(&mut ctx, 1, "{card 60}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.card(fixtures::SPRITE).is_none(), "the token vanishes");
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.on_board(fixtures::VI));
        assert_eq!(ctx.card(CULL).unwrap().zone, Some(fixtures::TRASH));
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
            .contains(&"{seat 0} chooses {card 92}".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} chooses {card 60}".to_string()));
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "the lone unit at the battlefield is gone"
        );
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_seat_with_one_unit_answers_unasked_and_a_seat_with_none_is_skipped() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![MOURNED, fixtures::SPRITE].contains(&card.id));
        fixture.table.tokens.clear();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one unit each: nothing to choose"
        );
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut lone = armed();
        lone.table
            .cards
            .retain(|card| ![fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id));
        lone.table.tokens.clear();
        lone.resolve();
        lone.scripts = lone.scripts.clone().with_script(MOURNED, &MOURNED_CARD);
        let mut ctx = lone.ctx();
        cast(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "055 · seat 1 has nothing to kill and is not asked"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no unit to kill".to_string()));
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(MOURNED));
    }

    #[test]
    fn a_stolen_unit_is_killed_by_its_controller_not_its_owner() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, fixtures::VI));
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(0)));
        assert_eq!(
            units_of(&ctx, 0),
            [fixtures::VI, MOURNED, fixtures::THEIR_UNIT],
            "the stolen unit sits on top of the thief's base"
        );
        assert_eq!(units_of(&ctx, 1), [fixtures::SPRITE]);
        cast(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 92}", "{card 81}"],
            "the stolen unit is one of the thief's"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the Sprite is seat 1's only unit and dies unasked"
        );
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.card(fixtures::SPRITE).is_none());
    }

    #[test]
    fn a_stale_pick_from_the_other_seat_is_refused_and_the_spell_is_the_turn_players_only() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
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
            "and the second is the other seat's"
        );
        assert!(ctx.on_board(fixtures::SPRITE) && ctx.on_board(fixtures::THEIR_UNIT));
        drop(ctx);

        let mut theirs = armed();
        theirs.table.cards.push(cull(THEIR_CULL, 1));
        theirs.resolve();
        let ctx = theirs.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CULL)),
            Err(Refusal::NotYourTurn),
            "a plain spell on the opponent's turn"
        );
    }
}
