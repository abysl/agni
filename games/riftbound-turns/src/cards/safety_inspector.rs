use super::prelude::{
    asking, done, friendly_units, play, remember_card, remembered_cards, unit, when,
    with_candidates,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::cost;
use crate::engine::ctx::{Cause, Ctx, Event};
use crate::engine::kill;
use crate::state::{ItemKind, TargetRef};

pub const ADDITIONAL_XP: u8 = 3;
pub const QUESTION: &str = "one of your units to kill";
pub const UNPAID: u8 = 0;
pub const PAID: u8 = 1;

pub fn xp_as_the_additional_cost_until_card_additional_carries_xp() -> cost::Cost {
    cost::Cost {
        xp: ADDITIONAL_XP,
        ..cost::Cost::free()
    }
}

fn paid_the_xp(_: &Ctx, event: &Event, source: Source) -> bool {
    matches!(
        event,
        Event::Played {
            card,
            paid_additional: true,
            ..
        } if *card == source.card
    )
}

fn skipped_the_xp(_: &Ctx, event: &Event, source: Source) -> bool {
    matches!(
        event,
        Event::Played {
            card,
            paid_additional: false,
            ..
        } if *card == source.card
    )
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

pub fn seats_that_kill(ctx: &Ctx, item: &Item) -> Vec<u8> {
    let spared = match item.kind {
        ItemKind::Trigger { index: PAID, .. } => Some(item.controller),
        _ => None,
    };
    seats_in_turn_order(ctx)
        .into_iter()
        .filter(|seat| Some(*seat) != spared)
        .collect()
}

fn units_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| !ctx.is_facedown(*unit))
        .collect()
}

fn seat_at(ctx: &Ctx, item: &Item, stage: Stage) -> Option<u8> {
    let index = usize::from(stage.0).checked_sub(1)?;
    seats_that_kill(ctx, item).get(index).copied()
}

fn their_units(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    seat_at(ctx, item, stage)
        .map(|seat| units_of(ctx, seat))
        .unwrap_or_default()
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn inspect(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seats = seats_that_kill(ctx, item);
    let mut next = usize::from(stage.0);
    let mut chosen = remembered_cards(item);
    if let Some(chooser) = seat_at(ctx, item, stage) {
        if let Some(unit) = ctx.picks().first().copied() {
            if units_of(ctx, chooser).contains(&unit) {
                ctx.narrate(format!("{{seat {chooser}}} chooses {{card {unit}}}"));
                remember_card(ctx, unit);
                chosen.push(unit);
            }
        }
    }
    if stage.0 == 0 && matches!(item.kind, ItemKind::Trigger { index: PAID, .. }) {
        let seat = item.controller;
        ctx.narrate(format!(
            "{{seat {seat}}} paid the inspection · they kill no unit"
        ));
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

pub static CARD: Card = unit(
    "Safety Inspector",
    &[],
    &[
        when(
            asking(with_candidates(play(&[], inspect), their_units), QUESTION),
            skipped_the_xp,
        ),
        when(
            asking(with_candidates(play(&[], inspect), their_units), QUESTION),
            paid_the_xp,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{pay, prompts, settle, triggers};
    use crate::state::{Origin, PromptWhy, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const INSPECTOR: u32 = 90;
    const CLERK: u32 = 91;
    const SPARE_RUNES: [u32; 3] = [46, 47, 48];

    fn inspector(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(INSPECTOR, zone, 0, "Safety Inspector", 3);
        card.energy = Some(5);
        card.power = Some(1);
        card.domain = vec!["Order".into()];
        card
    }

    fn site(zone: u16, xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(inspector(zone));
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(CLERK, fixtures::BASE, 0, "Clerk", 1));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(INSPECTOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn played_card(card: u32, paid_additional: bool) -> Event {
        Event::Played {
            card,
            controller: 0,
            kind: crate::cards::KIND_UNIT.into(),
            origin: Origin::Hand,
            paid_additional,
        }
    }

    fn played(paid_additional: bool) -> Event {
        played_card(INSPECTOR, paid_additional)
    }

    fn additional_confirm(ctx: &Ctx) -> bool {
        matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL
        )
    }

    #[test]
    fn the_script_is_two_gated_play_triggers_and_the_xp_cost_is_the_named_seam() {
        assert!(std::ptr::eq(script_of("Safety Inspector").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is energy and power · XP is not one"
        );
        assert_eq!(CARD.abilities.len(), 2);
        for ability in CARD.abilities {
            assert_eq!(ability.trigger, Trigger::Play);
            assert!(ability.targets.is_empty());
            assert!(!ability.optional, "the may is the cost, not the kill");
            assert!(ability.condition.is_some());
            assert_eq!(ability.question, Some(QUESTION));
            assert!(ability.candidates.is_some());
        }
        assert_eq!(ADDITIONAL_XP, 3);
        let seam = xp_as_the_additional_cost_until_card_additional_carries_xp();
        assert_eq!(seam.xp, 3);
        assert!(!seam.needs_runes());
        assert_eq!(seam.label(), "3 XP");
        let mut rich = site(fixtures::HAND, 3);
        let ctx = rich.ctx();
        assert!(pay::affordable(&ctx, 0, &seam));
        drop(ctx);
        let mut poor = site(fixtures::HAND, 2);
        let ctx = poor.ctx();
        assert!(!pay::affordable(&ctx, 0, &seam));
    }

    #[test]
    fn the_gates_split_on_the_paid_additional_flag_and_the_paid_trigger_spares_its_controller() {
        let mut fixture = site(fixtures::BASE, 0);
        let ctx = fixture.ctx();
        let source = Source {
            card: INSPECTOR,
            ability: 0,
        };
        assert!(skipped_the_xp(&ctx, &played(false), source));
        assert!(!skipped_the_xp(&ctx, &played(true), source));
        assert!(paid_the_xp(&ctx, &played(true), source));
        assert!(!paid_the_xp(&ctx, &played(false), source));
        assert!(!paid_the_xp(&ctx, &played_card(CLERK, true), source));
        let unpaid = Item::new(
            7,
            ItemKind::Trigger {
                source: INSPECTOR,
                index: UNPAID,
            },
            0,
            Origin::Board,
        );
        let paid = Item::new(
            8,
            ItemKind::Trigger {
                source: INSPECTOR,
                index: PAID,
            },
            0,
            Origin::Board,
        );
        assert_eq!(seats_that_kill(&ctx, &unpaid), [0, 1]);
        assert_eq!(seats_that_kill(&ctx, &paid), [1]);
        assert_eq!(seat_at(&ctx, &unpaid, Stage(1)), Some(0));
        assert_eq!(seat_at(&ctx, &unpaid, Stage(2)), Some(1));
        assert_eq!(seat_at(&ctx, &paid, Stage(1)), Some(1));
        assert_eq!(seat_at(&ctx, &paid, Stage(2)), None);
        assert_eq!(
            their_units(&ctx, &unpaid, Stage(1)),
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Card(INSPECTOR),
                TargetRef::Card(CLERK)
            ],
            "the inspector is one of its controller's units"
        );
        assert_eq!(
            their_units(&ctx, &paid, Stage(1)),
            [
                TargetRef::Card(fixtures::SPRITE),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
    }

    #[test]
    fn unpaid_each_seat_kills_one_of_its_units_turn_player_first_and_the_kills_land_together() {
        let mut fixture = site(fixtures::HAND, 0);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, INSPECTOR).unwrap();
        assert!(ctx.on_board(INSPECTOR));
        assert!(!additional_confirm(&ctx), "no XP, no confirm");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: UNPAID } if source == INSPECTOR
        ));
        fixtures::pass_until_open(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 91}", "{card 90}"],
            "303.2.a · the turn player chooses first, the inspector among its own units"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 1 }),
            "{card 90}: choose one of your units to kill (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert!(ctx.on_board(CLERK), "the kills wait for every choice");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 2 }));
        assert_eq!(ctx.blob.prompt.clone().unwrap().seat, 1);
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 81}"]);
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(CLERK).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.on_board(INSPECTOR));
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.on_board(fixtures::SPRITE));
        assert!(ctx
            .blob
            .log
            .contains(&"the chosen units are killed together".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn paid_the_controller_is_spared_and_only_the_opponents_choose() {
        let mut fixture = site(fixtures::BASE, 0);
        let mut ctx = fixture.ctx();
        ctx.raise(played(true));
        assert_eq!(triggers::collect(&mut ctx), 1, "one gate opens");
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index: PAID } if source == INSPECTOR
        ));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        assert_eq!(
            ctx.blob.prompt.clone().unwrap().seat,
            1,
            "seat 0 paid and is never asked"
        );
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 81}"]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} paid the inspection · they kill no unit".to_string()));
        fixtures::choose(&mut ctx, 1, "{card 60}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::SPRITE).is_none(), "the token vanishes");
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.on_board(CLERK));
        assert!(ctx.on_board(INSPECTOR));
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_seat_with_one_unit_answers_unasked_and_a_stale_pick_from_the_other_seat_is_refused() {
        let mut fixture = site(fixtures::HAND, 0);
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.tokens.clear();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, INSPECTOR).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the first prompt is the turn player's"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "Jinx is seat 1's only unit and dies unasked"
        );
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.on_board(INSPECTOR));
    }

    #[test]
    fn a_pool_short_of_the_printed_cost_refuses_the_play() {
        let mut fixture = site(fixtures::HAND, 3);
        for rune in SPARE_RUNES {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, INSPECTOR),
            Err(Refusal::NotEnoughRunes {
                needed: 5,
                ready: 3
            })
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 3, "a refused play spends no XP");
    }

    #[test]
    #[ignore = "engine gap · cards::Cost carries energy and power only, so Card.additional cannot ask for 3 XP; with an xp field the script is with_additional(unit(...), ADDITIONAL) and play::advance offers the confirm at STAGE_ADDITIONAL when the seat holds 3 XP"]
    fn with_three_xp_the_play_offers_the_additional_cost_and_paying_it_spares_the_controller() {
        let mut fixture = site(fixtures::HAND, 3);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, INSPECTOR).unwrap();
        assert!(additional_confirm(&ctx), "{:?}", ctx.blob.why);
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.on_board(INSPECTOR));
        assert_eq!(ctx.xp(0), 0, "the XP is spent with the rest of the cost");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: PAID } if source == INSPECTOR
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.prompt.clone().unwrap().seat, 1);
        fixtures::choose(&mut ctx, 1, "{card 60}").unwrap();
        assert!(ctx.on_board(fixtures::VI) && ctx.on_board(CLERK));
        assert!(ctx.card(fixtures::SPRITE).is_none());
    }
}
