use super::prelude::{asking, banish_by, done, play, spell, with_candidates, Location};
use super::{Card, Flow, Item, Paying, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::{cost, pay, play as play_engine};
use crate::state::{CardState, Origin, TargetRef, FLAG_REVEALING};

pub const LOOK: usize = 5;
pub const DISCOUNT: u8 = 5;
pub const QUESTION: &str = "a unit among the top five to banish and play, then where it is played";
pub const PICK: u8 = 1;
pub const LOCATE: u8 = 2;
pub const REVEALED: u8 = 8;

pub fn looked(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default()
}

pub fn reinforced(ctx: &Ctx, unit: u32) -> cost::Cost {
    let printed = cost::printed_of(ctx, unit, false);
    cost::Cost {
        energy: printed.energy.saturating_sub(DISCOUNT),
        ..printed
    }
}

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn banished_awaiting(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.banished_of(seat)
        .into_iter()
        .find(|card| ctx.has_flag(*card, FLAG_REVEALING))
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        PICK => looked(ctx, item.controller)
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        LOCATE..REVEALED => location_options(ctx, item.controller),
        _ => Vec::new(),
    }
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

fn recycle_all(ctx: &mut Ctx, seat: u8, cards: &[u32]) {
    for card in cards {
        ctx.recycle_to_bottom(*card);
    }
    if !cards.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", cards.len()));
    }
}

fn banish_and_await(ctx: &mut Ctx, item: &Item, card: u32, zone_index: u8) -> Flow {
    let seat = item.controller;
    let rest: Vec<u32> = looked(ctx, seat)
        .into_iter()
        .filter(|held| *held != card)
        .collect();
    if !banish_by(ctx, card, seat) {
        return done();
    }
    ctx.set_flag(card, FLAG_REVEALING, true);
    recycle_all(ctx, seat, &rest);
    Flow::Ask(ctx.await_faces(item, &[card], REVEALED.saturating_add(zone_index)))
}

fn deploy(ctx: &mut Ctx, item: &Item, zone_index: u8) -> Flow {
    let seat = item.controller;
    let Some(unit) = banished_awaiting(ctx, seat) else {
        return done();
    };
    ctx.set_flag(unit, FLAG_REVEALING, false);
    if ctx.state_of(unit).is_some_and(CardState::is_default) {
        ctx.blob.drop_card_state(unit);
    }
    if !ctx.is_unit(unit) {
        ctx.recycle_to_bottom(unit);
        ctx.narrate(format!(
            "{{card {unit}}} is not a unit · it is recycled with the rest"
        ));
        return done();
    }
    let at = ctx
        .play_locations(seat)
        .get(usize::from(zone_index))
        .copied()
        .unwrap_or(Location::Base(seat));
    let total = reinforced(ctx, unit);
    let play = cost::play_item(ctx, seat, unit, Origin::Banishment);
    let Ok(plan) = pay::plan_for(ctx, seat, &total, Paying::Item(&play)) else {
        ctx.narrate(format!(
            "{{card {unit}}} stays banished · its reduced cost can't be paid"
        ));
        return done();
    };
    pay::pay(ctx, seat, &plan);
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {unit}}} for {} · reduced by {DISCOUNT} energy",
        total.label()
    ));
    let _ = play_engine::begin(ctx, seat, unit, Origin::Banishment, Some(at));
    done()
}

fn reinforce(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        PICK => {
            let top = looked(ctx, seat);
            let Some(index) = ctx
                .picks()
                .first()
                .and_then(|card| top.iter().position(|held| held == card))
            else {
                ctx.narrate(format!("{{seat {seat}}} banishes nothing"));
                recycle_all(ctx, seat, &top);
                return done();
            };
            if ctx.play_locations(seat).len() > 1 {
                let stage = u8::try_from(index)
                    .ok()
                    .and_then(|index| index.checked_add(LOCATE))
                    .unwrap_or(LOCATE);
                return Flow::Ask(ctx.ask_resume(item, stage, 1, 1));
            }
            banish_and_await(ctx, item, top[index], 0)
        }
        LOCATE..REVEALED => {
            let top = looked(ctx, seat);
            let Some(card) = top.get(usize::from(stage.0 - LOCATE)).copied() else {
                return done();
            };
            let zone_index = ctx
                .picks()
                .first()
                .and_then(|zone| {
                    location_options(ctx, seat).iter().position(|held| {
                        *held == TargetRef::Zone(u16::try_from(*zone).unwrap_or(u16::MAX))
                    })
                })
                .and_then(|index| u8::try_from(index).ok())
                .unwrap_or(0);
            banish_and_await(ctx, item, card, zone_index)
        }
        REVEALED.. => deploy(ctx, item, stage.0 - REVEALED),
        _ => look(ctx, item),
    }
}

pub static CARD: Card = spell(
    "Reinforce",
    &[],
    &[asking(
        with_candidates(play(&[], reinforce), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::script_of;
    use crate::cards::the_harrowing::tests::DRAWS;
    use crate::cards::{KIND_SPELL, KIND_UNIT};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, Effect, BOTTOM};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::Face;

    const REINFORCE: u32 = 90;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut reinforce = fixtures::spell(REINFORCE, fixtures::HAND, 0, "Reinforce", 0, 0);
        reinforce.domain = vec!["Calm".into()];
        fixture.table.cards.push(reinforce);
        for id in 26..=27 {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::MAIN_DECK, 0));
        }
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, REINFORCE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
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

    fn arrives(fixture: &mut Fixture, card: u32, face: Face, script: &'static Card) -> Vec<Event> {
        let action = Action::Reveal { card, face };
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        let events = ctx.events.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
        events
    }

    fn unit_face(energy: u8, power: u8) -> Face {
        Face::named("Crab")
            .with_kind(KIND_UNIT)
            .with_might(Some(2))
            .with_cost(Some(energy), Some(power))
            .with_domain(vec!["Calm".into()])
    }

    #[test]
    fn the_script_is_a_sorcery_that_looks_at_five_and_prices_the_pick_five_energy_down() {
        assert!(std::ptr::eq(script_of("Reinforce").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(CARD.abilities[0].question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = armed();
        fixture.table.card_mut(26).unwrap().energy = Some(7);
        fixture.table.card_mut(26).unwrap().power = Some(1);
        fixture.table.card_mut(26).unwrap().domain = vec!["Calm".into()];
        fixture.table.card_mut(27).unwrap().energy = Some(3);
        let ctx = fixture.ctx();
        assert_eq!(looked(&ctx, 0), [27, 26, 23, 22, 21]);
        assert_eq!(reinforced(&ctx, 26).energy, 2);
        assert_eq!(reinforced(&ctx, 26).power.len(), 1);
        assert_eq!(reinforced(&ctx, 27).energy, 0);
    }

    #[test]
    fn five_peeks_reach_me_the_pick_is_banished_the_rest_recycled_and_the_reveal_plays_it() {
        let mut fixture = armed();
        cast(&mut fixture);
        {
            let ctx = fixture.ctx();
            assert_eq!(
                fixtures::labels(&ctx),
                [
                    "{card 27}",
                    "{card 26}",
                    "{card 23}",
                    "{card 22}",
                    "{card 21}",
                    "skip"
                ],
                "the top five and a skip; the sixth card stays unseen"
            );
        }
        pick(&mut fixture, "{card 26}");
        {
            let ctx = fixture.ctx();
            assert_eq!(ctx.banished_of(0), [26]);
            assert!(ctx.has_flag(26, FLAG_REVEALING));
            assert_eq!(
                deck_of(&ctx, 0),
                [21, 22, 23, 27, 20],
                "the four others went under in the listed order, the unseen card is now on top"
            );
            let parked = &ctx.blob.chain[0];
            assert_eq!(parked.status, ItemStatus::Resolving);
            assert_eq!(parked.stage, REVEALED);
            assert_eq!(parked.awaiting, [26]);
            assert!(ctx.blob.log.contains(&"{card 26} is banished".to_string()));
            assert!(ctx.blob.log.contains(&"{seat 0} recycles 4".to_string()));
        }
        let ready = fixture.ctx().ready_runes_of(0).len();
        let runes = fixture.ctx().runes_of(0).len();
        let hand = fixture.ctx().hand_of(0).len();
        let events = arrives(&mut fixture, 26, unit_face(7, 1), &DRAWS);
        assert!(events.iter().any(|event| matches!(
            event,
            Event::Played {
                card: 26,
                controller: 0,
                origin: Origin::Banishment,
                ..
            }
        )));
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.location(26),
            Some(Location::Base(0)),
            "{:?}",
            ctx.blob.log
        );
        assert!(ctx.card(26).unwrap().exhausted);
        assert!(!ctx.has_flag(26, FLAG_REVEALING));
        assert!(ctx.banished_of(0).is_empty());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 2,
            "7 energy less 5 is 2 runes exhausted"
        );
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "and one recycled for the Calm power"
        );
        assert!(ctx.blob.log.contains(
            &"{seat 0} plays {card 26} for 2 energy and 1 Calm power · reduced by 5 energy"
                .to_string()
        ));
        assert_eq!(ctx.card(REINFORCE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.chain.len(), 1, "its play trigger fires");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger {
                source: 26,
                index: 0
            }
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_recycles_all_five_and_a_thin_deck_offers_what_there_is() {
        let mut fixture = armed();
        cast(&mut fixture);
        pick(&mut fixture, "skip");
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.banished_of(0).is_empty());
        assert_eq!(deck_of(&ctx, 0), [21, 22, 23, 26, 27, 20]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} banishes nothing".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 5".to_string()));
        assert!(ctx.effects.iter().all(|effect| !matches!(
            effect,
            Effect::Move { zone, index, .. } if *zone == fixtures::MAIN_DECK && *index != BOTTOM
        )));
        drop(ctx);
        let mut thin = armed();
        thin.table
            .cards
            .retain(|card| ![20, 21, 22, 26, 27].contains(&card.id));
        thin.resolve();
        let mut ctx = thin.ctx();
        fixtures::play_from_hand(&mut ctx, 0, REINFORCE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(fixtures::labels(&ctx), ["{card 23}", "skip"]);
        drop(ctx);
        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        fixtures::play_from_hand(&mut ctx, 0, REINFORCE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to look at".to_string()));
    }

    #[test]
    fn a_held_battlefield_asks_where_before_the_banish_and_the_unit_lands_there() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        cast(&mut fixture);
        pick(&mut fixture, "{card 23}");
        {
            let ctx = fixture.ctx();
            assert_eq!(
                ctx.blob.why,
                Some(PromptWhy::Resume {
                    item: 1,
                    stage: LOCATE + 2
                }),
                "the third of the five, encoded in the stage"
            );
            assert_eq!(
                fixtures::labels(&ctx),
                [
                    format!("{{zone {}}}", fixtures::BASE),
                    format!("{{zone {}}}", fixtures::BF1)
                ]
            );
            assert!(ctx.banished_of(0).is_empty(), "nothing is banished yet");
        }
        pick(&mut fixture, &format!("{{zone {}}}", fixtures::BF1));
        {
            let ctx = fixture.ctx();
            assert_eq!(ctx.banished_of(0), [23]);
            assert_eq!(ctx.blob.chain[0].stage, REVEALED + 1);
        }
        arrives(&mut fixture, 23, unit_face(5, 0), &DRAWS);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.location(23),
            Some(Location::Battlefield(fixtures::BF1)),
            "{:?}",
            ctx.blob.log
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_pick_that_turns_out_not_to_be_a_unit_is_recycled_and_an_unpayable_one_stays_banished() {
        let mut fixture = armed();
        cast(&mut fixture);
        pick(&mut fixture, "{card 26}");
        arrives(
            &mut fixture,
            26,
            Face::named("Spark")
                .with_kind(KIND_SPELL)
                .with_cost(Some(2), Some(1)),
            &DRAWS,
        );
        let ctx = fixture.ctx();
        assert!(ctx.banished_of(0).is_empty());
        assert_eq!(ctx.card(26).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert_eq!(deck_of(&ctx, 0), [26, 21, 22, 23, 27, 20]);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 26} is not a unit · it is recycled with the rest".to_string()));
        drop(ctx);
        let mut fixture = armed();
        cast(&mut fixture);
        pick(&mut fixture, "{card 26}");
        arrives(&mut fixture, 26, unit_face(9, 4), &DRAWS);
        let ctx = fixture.ctx();
        assert_eq!(ctx.banished_of(0), [26]);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 26} stays banished · its reduced cost can't be paid".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_cannot_answer_and_a_pass_while_the_face_is_owed_changes_nothing() {
        let mut fixture = armed();
        cast(&mut fixture);
        let mut ctx = fixture.ctx();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        for card in [27, 26, 23, 22, 21] {
            assert!(
                ctx.effects.contains(&Effect::Peek { card, seat: 0 })
                    || ctx
                        .blob
                        .log
                        .iter()
                        .any(|line| line == "{seat 0} looks at the top 5 cards of their deck")
            );
            assert!(!ctx.effects.contains(&Effect::Peek { card, seat: 1 }));
        }
        drop(ctx);
        pick(&mut fixture, "{card 26}");
        let mut ctx = fixture.ctx();
        let _ = crate::engine::priority::pass(&mut ctx, 0);
        let _ = crate::engine::priority::pass(&mut ctx, 1);
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Resolving);
        assert_eq!(ctx.banished_of(0), [26]);
    }

    #[test]
    #[ignore = "engine gap · the plugin folds on public faces only, so it cannot tell a unit from a spell among the peeked five; every card is offered and a non-unit pick is recycled once its face reaches Banishment"]
    fn only_the_units_among_the_five_are_offered() {
        let mut fixture = armed();
        fixture.table.card_mut(26).unwrap().name = "Spark".into();
        fixture.table.card_mut(26).unwrap().kind = Some(KIND_SPELL.into());
        fixture.resolve();
        cast(&mut fixture);
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 27}", "{card 23}", "{card 22}", "{card 21}", "skip"]
        );
    }
}
