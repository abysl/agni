use super::prelude::{done, draw_revealed, forget_revealing, unit, Location};
use super::{Card, Flow, Item, Stage, KIND_SPELL};
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, FLAG_REVEALING};
use agni_plugin_sdk::decide::{Effect, TOP};

pub const QUESTION: &str = "the top card of your deck to recycle before the reveal";
pub const RECYCLE: u8 = 1;
pub const REVEALED: u8 = 2;

pub static CARD: Card = unit("Diana - Lunari", &[], &[]);

pub fn a_showdown_is_open_here(ctx: &Ctx, me: u32) -> bool {
    let Some(showdown) = ctx.blob.showdown.as_ref() else {
        return false;
    };
    ctx.location(me) == Some(Location::Battlefield(showdown.zone))
}

fn top_card(ctx: &Ctx, seat: u8) -> Option<u32> {
    let deck = ctx.zones.main_deck?;
    ctx.table.held(deck, seat).last().map(|card| card.id)
}

pub fn top_of_deck(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != RECYCLE {
        return Vec::new();
    }
    top_card(ctx, item.controller)
        .map(TargetRef::Card)
        .into_iter()
        .collect()
}

fn revealing(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .find(|card| {
            ctx.owner(*card) == seat
                && ctx
                    .card(*card)
                    .is_some_and(|held| held.zone == ctx.zones.chain)
        })
}

fn reveal(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let Some(top) = ctx.reveal_top(seat) else {
        ctx.narrate(format!("{{seat {seat}}} has no card to reveal"));
        return done();
    };
    Flow::Ask(ctx.await_faces(item, &[top], REVEALED))
}

fn sort(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let Some(card) = revealing(ctx, seat) else {
        return done();
    };
    if ctx.kind_of(card) == Some(KIND_SPELL) {
        draw_revealed(ctx, seat, card);
        return done();
    }
    forget_revealing(ctx, card);
    if let Some(deck) = ctx.zones.main_deck {
        ctx.emit(Effect::Move {
            card,
            zone: deck,
            seat,
            index: TOP,
        });
    }
    ctx.narrate(format!(
        "{{card {card}}} is not a spell · it goes back on top of the deck"
    ));
    done()
}

pub fn lunari(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        RECYCLE => {
            let top = top_card(ctx, seat);
            match ctx.picks().first().copied() {
                Some(card) if top == Some(card) => {
                    ctx.recycle_to_bottom(card);
                    ctx.narrate(format!(
                        "{{seat {seat}}} recycles the top card of their deck"
                    ));
                }
                _ => ctx.narrate(format!("{{seat {seat}}} keeps the top card of their deck")),
            }
            reveal(ctx, item)
        }
        REVEALED => sort(ctx, item),
        _ => {
            if ctx.peek_top(seat).is_none() {
                ctx.narrate(format!("{{seat {seat}}} has no card to predict"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, RECYCLE, 0, 1))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{
        asking, on_attack, optional, with_candidates, with_cost, ONE_ENERGY,
    };
    use crate::cards::{script_of, Trigger, Who, KIND_UNIT};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{chain, cleanup, prompts, settle, triggers};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::{CardInfo, Face, Snapshot};

    const DIANA: u32 = 90;
    const MY_TOP: u32 = 23;
    const MY_NEXT: u32 = 22;

    static WIRED_TO_HER_ATTACK_FOR_THE_TEST: Card = unit(
        "Diana - Lunari",
        &[],
        &[optional(with_cost(
            asking(
                with_candidates(on_attack(&[], lunari), top_of_deck),
                QUESTION,
            ),
            ONE_ENERGY,
        ))],
    );

    fn diana(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Mind".into()],
            ..fixtures::unit(DIANA, zone, 0, "Diana - Lunari", 3)
        }
    }

    fn temple() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(diana(fixtures::BF1));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(DIANA).unwrap(), &CARD));
        fixture
    }

    fn wire(fixture: &mut Fixture) {
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(DIANA, &WIRED_TO_HER_ATTACK_FOR_THE_TEST);
    }

    fn wired() -> Fixture {
        let mut fixture = temple();
        wire(&mut fixture);
        fixture
    }

    fn commit(fixture: &mut Fixture, table: Snapshot) {
        fixture.commit(table);
        wire(fixture);
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: DIANA });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(ctx, ctx.blob.why.unwrap()),
            "pay 1 energy for the {card 90} trigger?"
        );
    }

    fn pay_and_predict(fixture: &mut Fixture, recycle: bool) -> u32 {
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1, "one rune pays");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DIANA
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: RECYCLE
            }),
            "Predict looks at the top card as the trigger resolves"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {MY_TOP}}}"), "skip".to_string()]
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} looks at the top card of their deck".to_string()));
        let pick = if recycle {
            format!("{{card {MY_TOP}}}")
        } else {
            "skip".to_string()
        };
        fixtures::choose(&mut ctx, 0, &pick).unwrap();
        let revealed = if recycle { MY_NEXT } else { MY_TOP };
        let parked = &ctx.blob.chain[0];
        assert_eq!(
            (parked.status, parked.stage),
            (ItemStatus::Resolving, REVEALED)
        );
        assert_eq!(parked.awaiting, [revealed], "the reveal is awaited");
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.effects.contains(&Effect::Reveal { card: revealed }));
        assert_eq!(
            ctx.card(revealed).unwrap().zone,
            ctx.zones.chain,
            "the revealed card sits on the chain while its face is awaited"
        );
        let table = ctx.table.clone();
        drop(ctx);
        commit(fixture, table);
        revealed
    }

    fn reveal(fixture: &mut Fixture, card: u32, face: Face) {
        let action = Action::Reveal { card, face };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        commit(fixture, table);
    }

    #[test]
    fn the_stub_is_the_pool_name_and_the_showdown_begins_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Diana - Lunari").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        let wired = &WIRED_TO_HER_ATTACK_FOR_THE_TEST.abilities[0];
        assert_eq!(wired.trigger, Trigger::Attacks(Who::Me));
        assert!(wired.optional, "you may pay 1");
        assert_eq!(wired.cost, Some(ONE_ENERGY));
        assert!(wired.candidates.is_some());
        assert_eq!(wired.question, Some(QUESTION));
        let mut fixture = temple();
        let ctx = fixture.ctx();
        assert!(!a_showdown_is_open_here(&ctx, DIANA), "nothing is open yet");
        drop(ctx);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(a_showdown_is_open_here(&ctx, DIANA));
        assert!(
            !a_showdown_is_open_here(&ctx, fixtures::VI),
            "Vi stands in the base"
        );
    }

    #[test]
    fn keeping_the_top_card_then_revealing_a_spell_draws_it() {
        let mut fixture = wired();
        let revealed = pay_and_predict(&mut fixture, false);
        assert_eq!(revealed, MY_TOP);
        reveal(
            &mut fixture,
            MY_TOP,
            Face::named("Spark")
                .with_kind(KIND_SPELL)
                .with_domain(vec!["Mind".into()]),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty(), "the trigger resolved");
        assert!(ctx.in_hand(MY_TOP), "a spell is drawn");
        assert!(!ctx.has_flag(MY_TOP, FLAG_REVEALING));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} draws {{card {MY_TOP}}}")));
        assert_eq!(
            ctx.top_of(fixtures::MAIN_DECK, 0, 1),
            [MY_NEXT],
            "the deck lost its top card"
        );
    }

    #[test]
    fn recycling_the_top_card_reveals_the_next_and_a_unit_goes_back_on_top() {
        let mut fixture = wired();
        let revealed = pay_and_predict(&mut fixture, true);
        assert_eq!(revealed, MY_NEXT, "the recycled card is at the bottom now");
        {
            let ctx = fixture.ctx();
            assert_eq!(
                ctx.table
                    .held(fixtures::MAIN_DECK, 0)
                    .next()
                    .map(|card| card.id),
                Some(MY_TOP),
                "the predicted card went to the bottom"
            );
            assert!(ctx
                .blob
                .log
                .contains(&"{seat 0} recycles the top card of their deck".to_string()));
        }
        reveal(
            &mut fixture,
            MY_NEXT,
            Face::named("Brute")
                .with_kind(KIND_UNIT)
                .with_might(Some(2))
                .with_domain(vec!["Fury".into()]),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.in_hand(MY_NEXT), "a unit is not drawn");
        assert_eq!(
            ctx.top_of(fixtures::MAIN_DECK, 0, 1),
            [MY_NEXT],
            "it goes back on top of the deck"
        );
        assert!(!ctx.has_flag(MY_NEXT, FLAG_REVEALING));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MY_NEXT}}} is not a spell · it goes back on top of the deck"
        )));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
    }

    #[test]
    fn declining_the_energy_or_lacking_it_removes_the_trigger_and_an_empty_deck_predicts_nothing() {
        let mut fixture = wired();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost is declined".to_string()));
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Reveal { .. })));
        drop(ctx);

        let mut poor = wired();
        for rune in [41, 42, 43] {
            poor.table.card_mut(rune).unwrap().exhausted = true;
        }
        poor.resolve();
        wire(&mut poor);
        let mut ctx = poor.ctx();
        ctx.raise(Event::Attacks { card: DIANA });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost can't be paid".to_string()));
        drop(ctx);

        let mut empty = wired();
        empty
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::MAIN_DECK) && card.owner == 0));
        empty.resolve();
        wire(&mut empty);
        let mut ctx = empty.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card to predict".to_string()));
    }

    #[test]
    #[ignore = "engine gap · no trigger subject for a showdown beginning: Trigger has no ShowdownBegins arm and cleanup/showdown raise no event as one opens (464.2.b), so her trigger never fires; the engine owes Trigger::ShowdownBegins(Where::Here) raised with the zone as the showdown opens, wired as optional(with_cost(asking(with_candidates(triggered(ShowdownBegins(Here), &[], lunari), top_of_deck), QUESTION), ONE_ENERGY))"]
    fn a_showdown_beginning_at_her_battlefield_asks_for_the_energy() {
        let mut fixture = temple();
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(a_showdown_is_open_here(&ctx, DIANA));
        let hers = ctx
            .blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .find(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == DIANA))
            .map(|item| item.id)
            .expect("her trigger fires as the showdown begins here");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: hers,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
    }
}
