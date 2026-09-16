use super::prelude::{
    asking, attach_gear, done, forget_revealing, on_attack, optional, unit, when, with_candidates,
    LimitedPlay, Location, Price,
};
use super::{Card, Event, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::Ctx;
use crate::state::{Origin, TargetRef, FLAG_REVEALING};

pub const ENERGY_AT_MOST: u8 = 2;
pub const QUESTION: &str = "an Equipment in your hand costing 2 or less to play and attach to me";
pub const PICKED: u8 = 1;
pub const REVEALED: u8 = 2;

fn a_card_in_hand(ctx: &Ctx, _: &Event, source: Source) -> bool {
    !ctx.hand_of(ctx.controller(source.card)).is_empty()
}

fn hand_cards(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != PICKED {
        return Vec::new();
    }
    ctx.hand_of(item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn picked_in_hand(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .find(|card| ctx.owner(*card) == seat && ctx.in_hand(*card))
}

pub fn is_cheap_equipment(ctx: &Ctx, card: u32) -> bool {
    ctx.is_gear(card)
        && ctx.script(card).is_some_and(|script| script.is_equipment())
        && ctx
            .card(card)
            .and_then(|face| face.energy)
            .is_some_and(|energy| energy <= ENERGY_AT_MOST)
}

pub fn origin_of_a_free_hand_play() -> Origin {
    Origin::Hand
}

pub fn play_equipment_from_hand_ignoring_cost_and_attach(
    ctx: &mut Ctx,
    item: &Item,
    card: u32,
) -> bool {
    let me = item.kind.source();
    let seat = item.controller;
    if !ctx.on_board(me) {
        ctx.narrate(format!(
            "{{card {me}}} is gone · {{card {card}}} stays in hand"
        ));
        return false;
    }
    if !is_cheap_equipment(ctx, card) {
        ctx.narrate(format!(
            "{{card {card}}} is not an Equipment costing {ENERGY_AT_MOST} or less · it stays in hand"
        ));
        return false;
    }
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {card}}} from hand, ignoring its cost"
    ));
    let played = ctx.play_limited(LimitedPlay {
        card,
        by: seat,
        origin: origin_of_a_free_hand_play(),
        locations: vec![Location::Base(seat)],
        price: Price::Free,
    });
    if played.is_err() {
        return false;
    }
    attach_gear(ctx, card, me);
    true
}

fn magnetize(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        PICKED => {
            let Some(card) = ctx
                .picks()
                .first()
                .copied()
                .filter(|card| ctx.hand_of(seat).contains(card))
            else {
                ctx.narrate(format!("{{seat {seat}}} plays nothing"));
                return done();
            };
            ctx.set_flag(card, FLAG_REVEALING, true);
            if ctx.table.is_revealed(card) {
                return magnetize(ctx, item, Stage(REVEALED));
            }
            ctx.narrate(format!("{{seat {seat}}} reveals {{card {card}}}"));
            Flow::Ask(ctx.await_faces(item, &[card], REVEALED))
        }
        REVEALED => {
            let Some(card) = picked_in_hand(ctx, seat) else {
                return done();
            };
            forget_revealing(ctx, card);
            play_equipment_from_hand_ignoring_cost_and_attach(ctx, item, card);
            done()
        }
        _ => {
            if ctx.hand_of(seat).is_empty() {
                ctx.narrate(format!("{{seat {seat}}} has no card in hand"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Rell - Magnetic",
    &[Keyword::Tank],
    &[asking(
        with_candidates(
            when(optional(on_attack(&[], magnetize)), a_card_in_hand),
            hand_cards,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, attachments_of, Location};
    use crate::cards::{script_of, Trigger, Who, KIND_GEAR, KIND_UNIT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, prompts, settle, triggers};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use agni_plugin_sdk::decide::{Action, Effect};
    use agni_plugin_sdk::table::Face;

    const RELL: u32 = 90;
    const BOOTS: u32 = 91;
    const HEAVY: u32 = 92;
    const BRUTE: u32 = 93;
    const GLOVES: u32 = 94;

    fn face_of(id: u32) -> Face {
        match id {
            BOOTS => Face::named("Boots of Swiftness")
                .with_kind(KIND_GEAR)
                .with_cost(Some(2), None)
                .with_domain(vec!["Fury".into()]),
            HEAVY => Face::named("Brutalizer")
                .with_kind(KIND_GEAR)
                .with_cost(Some(3), None)
                .with_domain(vec!["Fury".into()]),
            GLOVES => Face::named("Treasure Trove")
                .with_kind(KIND_GEAR)
                .with_cost(Some(0), None),
            _ => Face::named("Brute")
                .with_kind(KIND_UNIT)
                .with_might(Some(3))
                .with_cost(Some(2), None)
                .with_domain(vec!["Fury".into()]),
        }
    }

    fn lists() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut rell = fixtures::unit(RELL, fixtures::BF1, 0, "Rell - Magnetic", 4);
        rell.domain = vec!["Fury".into()];
        rell.energy = Some(4);
        fixture.table.cards.push(rell);
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::HAND) && card.seat == 0));
        for id in [BOOTS, HEAVY, BRUTE, GLOVES] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::HAND, 0));
        }
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(RELL).unwrap(), &CARD));
        fixture
    }

    fn attacks(ctx: &mut Ctx) -> usize {
        ctx.raise(crate::engine::ctx::Event::Attacks { card: RELL });
        let found = triggers::collect(ctx);
        chain::proceed(ctx);
        found
    }

    fn pick(fixture: &mut Fixture, card: u32) {
        let mut ctx = fixture.ctx();
        assert_eq!(attacks(&mut ctx), 1);
        assert!(
            ctx.blob.prompt.is_none(),
            "no cost, so the may waits for the pick"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICKED
            })
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {card}}}")).unwrap();
        let parked = &ctx.blob.chain[0];
        assert_eq!(
            (parked.status, parked.stage),
            (ItemStatus::Resolving, REVEALED)
        );
        assert_eq!(parked.awaiting, [card]);
        assert!(ctx.effects.contains(&Effect::Reveal { card }));
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn reveal(fixture: &mut Fixture, card: u32) {
        let action = Action::Reveal {
            card,
            face: face_of(card),
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn runes_of(ctx: &Ctx) -> Vec<(u32, bool)> {
        ctx.runes_of(0)
            .into_iter()
            .map(|rune| (rune.id, rune.exhausted))
            .collect()
    }

    #[test]
    fn the_script_prints_tank_and_an_optional_attack_trigger_that_asks_for_a_hand_card() {
        assert!(std::ptr::eq(script_of("Rell - Magnetic").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Tank]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_some());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn attacking_offers_every_card_of_the_blank_hand_and_skip_plays_nothing() {
        let mut fixture = lists();
        let mut ctx = fixture.ctx();
        assert_eq!(attacks(&mut ctx), 1);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == RELL
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BOOTS}}}"),
                format!("{{card {HEAVY}}}"),
                format!("{{card {BRUTE}}}"),
                format!("{{card {GLOVES}}}"),
                "skip".to_string()
            ],
            "the fold carries no hand faces, so every hand card is offered"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} plays nothing".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_revealed_two_cost_equipment_is_played_for_nothing_and_attached_to_her() {
        let mut fixture = lists();
        let runes_before = runes_of(&fixture.ctx());
        pick(&mut fixture, BOOTS);
        reveal(&mut fixture, BOOTS);
        let ctx = fixture.ctx();
        assert!(
            ctx.blob.chain.is_empty(),
            "the trigger and the free play both resolved"
        );
        assert!(ctx.on_board(BOOTS));
        assert_eq!(attached_to(&ctx, BOOTS), Some(RELL));
        assert_eq!(attachments_of(&ctx, RELL), [BOOTS]);
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Battlefield(fixtures::BF1)),
            "the gear follows the unit it is attached to"
        );
        assert!(!ctx.has_flag(BOOTS, FLAG_REVEALING));
        assert_eq!(runes_of(&ctx), runes_before, "nothing pays the Boots");
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {BOOTS}}} from hand, ignoring its cost"
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BOOTS}}} is attached to {{card {RELL}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_three_cost_equipment_a_unit_or_a_plain_gear_stays_in_hand() {
        for (card, why) in [
            (HEAVY, "not an Equipment costing 2 or less"),
            (BRUTE, "not an Equipment costing 2 or less"),
            (GLOVES, "not an Equipment costing 2 or less"),
        ] {
            let mut fixture = lists();
            pick(&mut fixture, card);
            reveal(&mut fixture, card);
            let ctx = fixture.ctx();
            assert!(ctx.blob.chain.is_empty());
            assert!(ctx.in_hand(card), "{card} should stay in hand");
            assert!(!ctx.has_flag(card, FLAG_REVEALING));
            assert!(attachments_of(&ctx, RELL).is_empty());
            assert!(ctx
                .blob
                .log
                .contains(&format!("{{card {card}}} is {why} · it stays in hand")));
        }
    }

    #[test]
    fn a_face_already_public_skips_the_wait_and_an_empty_hand_never_offers_the_may() {
        let mut fixture = lists();
        fixture
            .table
            .apply_entry(
                &Action::Reveal {
                    card: BOOTS,
                    face: face_of(BOOTS),
                },
                0,
            )
            .unwrap();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(is_cheap_equipment(&ctx, BOOTS));
        attacks(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BOOTS}}}")).unwrap();
        assert!(ctx.blob.chain.iter().all(|held| held.awaiting.is_empty()));
        assert_eq!(attached_to(&ctx, BOOTS), Some(RELL));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut empty = lists();
        empty
            .table
            .cards
            .retain(|card| ![BOOTS, HEAVY, BRUTE, GLOVES].contains(&card.id));
        empty.resolve();
        let mut ctx = empty.ctx();
        assert!(ctx.hand_of(0).is_empty());
        assert_eq!(
            attacks(&mut ctx),
            0,
            "an empty hand: the may is not offered"
        );
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    fn the_free_play_reports_the_hand_as_its_origin() {
        let mut fixture = lists();
        pick(&mut fixture, BOOTS);
        let action = Action::Reveal {
            card: BOOTS,
            face: face_of(BOOTS),
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, BOOTS).unwrap());
        settle(&mut ctx).unwrap();
        assert_eq!(origin_of_a_free_hand_play(), Origin::Hand);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            crate::engine::ctx::Event::Played {
                card: BOOTS,
                origin: Origin::Hand,
                ..
            }
        )));
    }
}
