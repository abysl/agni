use super::prelude::{asking, done, draw_revealed, forget_revealing, play, spell, with_candidates};
use super::{Card, Cost, Flow, Item, Keyword, Stage, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, FLAG_REVEALING};
use agni_plugin_sdk::decide::{Effect, TOP};

pub const LOOK: usize = 3;
pub const QUESTION: &str = "a unit among the top three to reveal and draw";
pub const PICK: u8 = 1;
pub const REVEALED: u8 = 2;
pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};

pub fn looked(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default()
}

fn may_be_unit(ctx: &Ctx, card: u32) -> bool {
    ctx.kind_of(card).is_none_or(|kind| kind == KIND_UNIT)
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        PICK => looked(ctx, item.controller)
            .into_iter()
            .filter(|card| may_be_unit(ctx, *card))
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
    if ctx.is_unit(card) {
        draw_revealed(ctx, seat, card);
    } else {
        ctx.recycle_to_bottom(card);
        ctx.narrate(format!(
            "{{card {card}}} is not a unit · it is recycled with the rest"
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
        .filter(|card| top.contains(card) && may_be_unit(ctx, *card));
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

fn trouble(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
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

pub static CARD: Card = spell(
    "Double Trouble",
    &[Keyword::Repeat(REPEAT)],
    &[asking(
        with_candidates(play(&[], trouble), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, KIND_SPELL};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::engine::play::SLOT_REPEAT;
    use crate::engine::{chain, prompts, settle};
    use crate::state::{ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, BOTTOM};
    use agni_plugin_sdk::table::{CardInfo, Face};

    const TROUBLE: u32 = 90;
    const THEIR_TROUBLE: u32 = 91;
    const EXTRA_RUNE: u32 = 100;

    fn trouble_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Double Trouble", 2, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(trouble_card(TROUBLE, 0));
        fixture.table.cards.push(trouble_card(THEIR_TROUBLE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(EXTRA_RUNE, 0, "Calm", false));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast(fixture: &mut Fixture, repeat: &str) {
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TROUBLE).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, repeat).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        for card in looked(&ctx, 0) {
            assert!(ctx.effects.contains(&Effect::Peek { card, seat: 0 }));
            assert!(!ctx.effects.contains(&Effect::Peek { card, seat: 1 }));
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

    fn unit_face(name: &str) -> Face {
        Face::named(name)
            .with_kind(KIND_UNIT)
            .with_might(Some(2))
            .with_cost(Some(2), None)
    }

    #[test]
    fn the_script_is_a_repeatable_sorcery_spell_that_asks_at_resolution() {
        assert!(std::ptr::eq(script_of("Double Trouble").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Repeat(REPEAT)]);
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(REPEAT.energy, 2);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(!ability.optional, "the may is the skip of the pick");
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(LOOK, 3);
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(looked(&ctx, 0), [23, 22, 21]);
        assert_eq!(looked(&ctx, 1), [25, 24]);
    }

    #[test]
    fn declined_it_peeks_three_reveals_the_pick_recycles_the_rest_and_draws_a_unit_face() {
        let mut fixture = armed();
        cast(&mut fixture, "no");
        {
            let ctx = fixture.ctx();
            assert!(ctx
                .blob
                .log
                .contains(&"{seat 0} looks at the top 3 cards of their deck".to_string()));
            assert_eq!(
                fixtures::labels(&ctx),
                ["{card 23}", "{card 22}", "{card 21}", "skip"],
                "unknown faces are all offered, the fourth card is not looked at"
            );
            assert_eq!(
                prompts::status(&ctx, ctx.blob.why.unwrap()),
                format!("{{card {TROUBLE}}}: choose {QUESTION} (0 of 1)")
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
                [21, 23, 20],
                "the two others went under in the listed order"
            );
            let parked = &ctx.blob.chain[0];
            assert_eq!(parked.status, ItemStatus::Resolving);
            assert_eq!(parked.stage, REVEALED);
            assert_eq!(parked.awaiting, [22]);
            assert!(ctx
                .blob
                .log
                .contains(&"{seat 0} reveals {card 22}".to_string()));
            assert!(ctx.blob.log.contains(&"{seat 0} recycles 2".to_string()));
        }
        arrives(&mut fixture, 22, unit_face("Jinx"));
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
        assert_eq!(deck_of(&ctx, 0), [21, 23, 20]);
        assert_eq!(ctx.card(TROUBLE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_pick_that_turns_out_not_to_be_a_unit_is_recycled_under_the_rest_and_skip_recycles_all() {
        let mut fixture = armed();
        cast(&mut fixture, "no");
        let hand = fixture.ctx().hand_of(0).len();
        pick(&mut fixture, "{card 23}");
        arrives(
            &mut fixture,
            23,
            Face::named("Spark")
                .with_kind(KIND_SPELL)
                .with_cost(Some(2), Some(1)),
        );
        {
            let ctx = fixture.ctx();
            assert!(ctx.blob.chain.is_empty());
            assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::MAIN_DECK));
            assert_eq!(deck_of(&ctx, 0), [23, 21, 22, 20]);
            assert_eq!(ctx.hand_of(0).len(), hand);
            assert_eq!(ctx.blob.seat(0).draws, 0);
            assert!(ctx
                .blob
                .log
                .contains(&"{card 23} is not a unit · it is recycled with the rest".to_string()));
            assert!(ctx.effects.iter().all(|effect| !matches!(
                effect,
                Effect::Move { zone, index, .. } if *zone == fixtures::MAIN_DECK && *index != BOTTOM
            )));
        }
        let mut skipped = armed();
        cast(&mut skipped, "no");
        let hand = skipped.ctx().hand_of(0).len();
        pick(&mut skipped, "skip");
        let ctx = skipped.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 0), [21, 22, 23, 20]);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals nothing".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 3".to_string()));
    }

    #[test]
    fn repeated_it_looks_twice_and_a_known_unit_face_is_drawn_without_waiting() {
        let mut fixture = armed();
        fixture.table.card_mut(21).unwrap().name = "Jinx".into();
        fixture.table.card_mut(21).unwrap().kind = Some(KIND_UNIT.into());
        fixture.table.card_mut(22).unwrap().name = "Spark".into();
        fixture.table.card_mut(22).unwrap().kind = Some(KIND_SPELL.into());
        fixture.resolve();
        cast(&mut fixture, "yes");
        {
            let ctx = fixture.ctx();
            assert!(ctx.blob.chain[0].repeated());
            assert_eq!(
                ctx.ready_runes_of(0).len(),
                0,
                "two energy for the spell and two for the Repeat"
            );
            assert_eq!(
                fixtures::labels(&ctx),
                ["{card 23}", "{card 21}", "skip"],
                "a face the table already knows as a spell is not a unit to reveal"
            );
        }
        let hand = fixture.ctx().hand_of(0).len();
        pick(&mut fixture, "{card 21}");
        let ctx = fixture.ctx();
        assert_eq!(ctx.card(21).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(
            deck_of(&ctx, 0),
            [22, 23, 20],
            "23 and 22 went under, 20 is now on top"
        );
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            }),
            "the Repeat looks at the top three again"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 20}", "{card 23}", "skip"],
            "the new top three, the known spell excluded"
        );
        drop(ctx);
        pick(&mut fixture, "skip");
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            deck_of(&ctx, 0),
            [22, 23, 20],
            "the three went under in the listed order"
        );
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.card(TROUBLE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn an_empty_deck_looks_at_nothing_and_the_opponents_copy_waits_for_their_turn() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_TROUBLE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, TROUBLE).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing to look at");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to look at".to_string()));
        assert_eq!(ctx.card(TROUBLE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }
}
