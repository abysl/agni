use super::prelude::{asking, done, draw_revealed, forget_revealing, play, unit, with_candidates};
use super::{Card, Flow, Item, Stage, KIND_SPELL};
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, FLAG_REVEALING};
use agni_plugin_sdk::decide::{Effect, TOP};

pub const LOOK: usize = 4;
pub const ENERGY_AT_LEAST: u8 = 4;
pub const QUESTION: &str = "a spell costing 4 or more among the top four to reveal and draw";
pub const PICK: u8 = 1;
pub const REVEALED: u8 = 2;

pub fn looked(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default()
}

pub fn is_big_spell(ctx: &Ctx, card: u32) -> bool {
    ctx.kind_of(card) == Some(KIND_SPELL)
        && ctx
            .card(card)
            .and_then(|face| face.energy)
            .is_some_and(|energy| energy >= ENERGY_AT_LEAST)
}

fn may_be_big_spell(ctx: &Ctx, card: u32) -> bool {
    ctx.kind_of(card).is_none() || is_big_spell(ctx, card)
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        PICK => looked(ctx, item.controller)
            .into_iter()
            .filter(|card| may_be_big_spell(ctx, *card))
            .map(TargetRef::Card)
            .collect(),
        _ => Vec::new(),
    }
}

fn revealing(ctx: &Ctx, seat: u8) -> Option<u32> {
    let chain = ctx.zones.chain?;
    ctx.table
        .held(chain, 0)
        .map(|card| card.id)
        .find(|card| ctx.has_flag(*card, FLAG_REVEALING) && ctx.owner(*card) == seat)
}

fn reveal_from_the_deck(ctx: &mut Ctx, seat: u8, card: u32) -> bool {
    let Some(chain) = ctx.zones.chain else {
        return false;
    };
    ctx.emit(Effect::Move {
        card,
        zone: chain,
        seat: 0,
        index: TOP,
    });
    ctx.set_flag(card, FLAG_REVEALING, true);
    ctx.reveal(card);
    ctx.narrate(format!("{{seat {seat}}} reveals {{card {card}}}"));
    true
}

fn recycle_all(ctx: &mut Ctx, seat: u8, cards: &[u32]) {
    for card in cards {
        ctx.recycle_to_bottom(*card);
    }
    if !cards.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", cards.len()));
    }
}

fn claim(ctx: &mut Ctx, seat: u8, card: u32) -> Flow {
    forget_revealing(ctx, card);
    if is_big_spell(ctx, card) {
        draw_revealed(ctx, seat, card);
    } else {
        ctx.recycle_to_bottom(card);
        ctx.narrate(format!(
            "{{card {card}}} is not a spell costing {ENERGY_AT_LEAST} or more · it is recycled with the rest"
        ));
    }
    done()
}

fn look(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    if top.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no cards left to look at"));
        return done();
    }
    for card in &top {
        ctx.peek(*card, seat);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} looks at the top {} cards of their deck",
        top.len()
    ));
    Flow::Ask(ctx.ask_resume(item, PICK, 0, 1))
}

fn pick(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    let picked = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| top.contains(card) && may_be_big_spell(ctx, *card));
    let Some(card) = picked else {
        ctx.narrate(format!("{{seat {seat}}} reveals nothing"));
        recycle_all(ctx, seat, &top);
        return done();
    };
    let rest: Vec<u32> = top.into_iter().filter(|held| *held != card).collect();
    if !reveal_from_the_deck(ctx, seat, card) {
        recycle_all(ctx, seat, &rest);
        return done();
    }
    recycle_all(ctx, seat, &rest);
    if ctx.kind_of(card).is_some() {
        return claim(ctx, seat, card);
    }
    Flow::Ask(ctx.await_faces(item, &[card], REVEALED))
}

fn weave(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        PICK => pick(ctx, item),
        REVEALED => match revealing(ctx, seat) {
            Some(card) => claim(ctx, seat, card),
            None => done(),
        },
        _ => look(ctx, item),
    }
}

pub static CARD: Card = unit(
    "Fate Weaver",
    &[],
    &[asking(
        with_candidates(play(&[], weave), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, KIND_UNIT};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, play as play_engine, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, Origin, PromptWhy};
    use agni_plugin_sdk::decide::{Action, BOTTOM};
    use agni_plugin_sdk::table::{CardInfo, Face};

    const WEAVER: u32 = 90;
    const DECK: [u32; 4] = [23, 22, 21, 20];

    fn weaver(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Mind".into()],
            ..fixtures::unit(WEAVER, zone, 0, "Fate Weaver", 4)
        }
    }

    fn loom() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(weaver(fixtures::HAND));
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn enter(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        play_engine::begin(&mut ctx, 0, WEAVER, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing is chosen at play");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == WEAVER
        ));
        let item = ctx.blob.chain[0].id;
        fixtures::pass_until_open(&mut ctx);
        if !ctx.blob.chain.is_empty() {
            assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: PICK }));
            for card in looked(&ctx, 0) {
                assert!(ctx.effects.contains(&Effect::Peek { card, seat: 0 }));
                assert!(!ctx.effects.contains(&Effect::Peek { card, seat: 1 }));
            }
        }
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn pick(fixture: &mut Fixture, label: &str) {
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, label).unwrap();
        pass_until_parked(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face) {
        let action = Action::Reveal { card, face };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    #[test]
    fn the_script_is_a_unit_whose_play_trigger_looks_at_four_and_asks_for_a_big_spell() {
        assert!(std::ptr::eq(script_of("Fate Weaver").unwrap(), &CARD));
        assert_eq!(CARD.name, "Fate Weaver");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(!ability.optional, "the may is the skip of the pick");
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!((LOOK, ENERGY_AT_LEAST), (4, 4));
        let mut fixture = loom();
        let ctx = fixture.ctx();
        assert_eq!(looked(&ctx, 0), DECK);
        assert_eq!(looked(&ctx, 1), [25, 24]);
    }

    #[test]
    fn a_big_spell_is_a_spell_face_costing_four_or_more_energy() {
        let mut fixture = loom();
        fixture
            .table
            .cards
            .push(fixtures::spell(100, fixtures::HAND, 0, "Big", 4, 1));
        fixture
            .table
            .cards
            .push(fixtures::spell(101, fixtures::HAND, 0, "Small", 3, 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(102, fixtures::HAND, 0, "Huge Unit", 9));
        fixture.table.card_mut(102).unwrap().energy = Some(9);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_big_spell(&ctx, 100));
        assert!(!is_big_spell(&ctx, 101), "three energy is short");
        assert!(!is_big_spell(&ctx, 102), "a unit is no spell");
        assert!(
            !is_big_spell(&ctx, 20),
            "an unknown face is not yet a spell"
        );
        assert!(may_be_big_spell(&ctx, 20), "but it may turn out to be one");
    }

    #[test]
    fn the_play_peeks_the_top_four_the_pick_is_revealed_the_rest_recycled_and_a_big_spell_is_drawn()
    {
        let mut fixture = loom();
        enter(&mut fixture);
        {
            let ctx = fixture.ctx();
            assert!(ctx
                .blob
                .log
                .contains(&"{seat 0} looks at the top 4 cards of their deck".to_string()));
            assert_eq!(
                fixtures::labels(&ctx),
                ["{card 23}", "{card 22}", "{card 21}", "{card 20}", "skip"],
                "unknown faces are all offered"
            );
            assert_eq!(
                prompts::status(&ctx, ctx.blob.why.unwrap()),
                format!("{{card {WEAVER}}}: choose {QUESTION} (0 of 1)")
            );
        }
        let hand = fixture.ctx().hand_of(0).len();
        pick(&mut fixture, "{card 22}");
        {
            let ctx = fixture.ctx();
            assert_eq!(ctx.card(22).unwrap().zone, Some(fixtures::CHAIN));
            assert!(ctx.has_flag(22, FLAG_REVEALING));
            assert_eq!(
                deck_of(&ctx, 0),
                [20, 21, 23],
                "the three others went under in the listed order"
            );
            let parked = &ctx.blob.chain[0];
            assert_eq!(parked.status, ItemStatus::Resolving);
            assert_eq!(parked.stage, REVEALED);
            assert_eq!(parked.awaiting, [22]);
            assert!(ctx
                .blob
                .log
                .contains(&"{seat 0} reveals {card 22}".to_string()));
            assert!(ctx.blob.log.contains(&"{seat 0} recycles 3".to_string()));
        }
        arrives(
            &mut fixture,
            22,
            Face::named("Icathian Rain")
                .with_kind(KIND_SPELL)
                .with_cost(Some(5), Some(2)),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(22).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(!ctx.has_flag(22, FLAG_REVEALING));
        assert_eq!(
            ctx.blob.seat(0).draws,
            1,
            "revealing and drawing it is a draw"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws {card 22}".to_string()));
        assert_eq!(deck_of(&ctx, 0), [20, 21, 23]);
    }

    #[test]
    fn a_pick_that_turns_out_cheap_or_not_a_spell_is_recycled_under_the_rest() {
        let mut fixture = loom();
        enter(&mut fixture);
        let hand = fixture.ctx().hand_of(0).len();
        pick(&mut fixture, "{card 23}");
        arrives(
            &mut fixture,
            23,
            Face::named("Spark")
                .with_kind(KIND_SPELL)
                .with_cost(Some(3), Some(1)),
        );
        {
            let ctx = fixture.ctx();
            assert!(ctx.blob.chain.is_empty());
            assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::MAIN_DECK));
            assert_eq!(deck_of(&ctx, 0), [23, 20, 21, 22]);
            assert_eq!(ctx.hand_of(0).len(), hand);
            assert_eq!(ctx.blob.seat(0).draws, 0);
            assert!(ctx.blob.log.contains(
                &"{card 23} is not a spell costing 4 or more · it is recycled with the rest"
                    .to_string()
            ));
            assert!(ctx.effects.iter().all(|effect| !matches!(
                effect,
                Effect::Move { zone, index, .. } if *zone == fixtures::MAIN_DECK && *index != BOTTOM
            )));
        }
        let mut unit = loom();
        enter(&mut unit);
        pick(&mut unit, "{card 23}");
        arrives(
            &mut unit,
            23,
            Face::named("Brute")
                .with_kind(KIND_UNIT)
                .with_cost(Some(6), Some(1)),
        );
        let ctx = unit.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(23).unwrap().zone,
            Some(fixtures::MAIN_DECK),
            "a big unit is no spell"
        );
        assert_eq!(ctx.blob.seat(0).draws, 0);
    }

    #[test]
    fn skipping_recycles_all_four_and_a_known_face_that_is_no_big_spell_is_never_offered() {
        let mut fixture = loom();
        enter(&mut fixture);
        let hand = fixture.ctx().hand_of(0).len();
        pick(&mut fixture, "skip");
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23]);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals nothing".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 4".to_string()));
        drop(ctx);
        let mut known = loom();
        known.table.card_mut(22).unwrap().name = "Spark".into();
        known.table.card_mut(22).unwrap().kind = Some(KIND_SPELL.into());
        known.table.card_mut(22).unwrap().energy = Some(2);
        known.table.card_mut(21).unwrap().name = "Brute".into();
        known.table.card_mut(21).unwrap().kind = Some(KIND_UNIT.into());
        known.table.card_mut(21).unwrap().energy = Some(7);
        known.resolve();
        enter(&mut known);
        let ctx = known.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}", "{card 20}", "skip"],
            "a cheap spell and a unit the table already knows are not offered"
        );
    }

    #[test]
    fn a_known_big_spell_is_drawn_without_waiting_and_a_thin_or_empty_deck_offers_what_there_is() {
        let mut fixture = loom();
        fixture.table.card_mut(21).unwrap().name = "Icathian Rain".into();
        fixture.table.card_mut(21).unwrap().kind = Some(KIND_SPELL.into());
        fixture.table.card_mut(21).unwrap().energy = Some(4);
        fixture.resolve();
        enter(&mut fixture);
        let hand = fixture.ctx().hand_of(0).len();
        pick(&mut fixture, "{card 21}");
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty(), "nothing waits on a face");
        assert_eq!(ctx.card(21).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(deck_of(&ctx, 0), [20, 22, 23]);
        drop(ctx);
        let mut thin = loom();
        thin.table
            .cards
            .retain(|card| ![20, 21, 22].contains(&card.id));
        thin.resolve();
        enter(&mut thin);
        let ctx = thin.ctx();
        assert_eq!(fixtures::labels(&ctx), ["{card 23}", "skip"]);
        drop(ctx);
        let mut empty = loom();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        empty.resolve();
        enter(&mut empty);
        let ctx = empty.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to look at".to_string()));
    }
}
