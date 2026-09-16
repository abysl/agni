use super::prelude::{
    asking, burn_cards, done, faceless, friendly_units, gear, might_this_turn, play, remember_card,
    remembered_cards, triggered, with_candidates,
};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const BURN: usize = 1;
pub const ON_PLAY: u8 = 0;
pub const ON_BEGINNING: u8 = 1;
pub const REVEALED: u8 = 1;
pub const GIVE: u8 = 2;
pub const QUESTION: &str = "a friendly unit to give the burned unit's Might this turn";

pub fn burned_unit_might(ctx: &Ctx, card: u32) -> Option<i16> {
    let held = ctx.card(card)?;
    if !ctx.is_unit(card) {
        return None;
    }
    held.might.map(i16::from)
}

pub fn trash_top(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.trash_of(seat).last().copied()
}

fn beneficiaries(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    friendly_units(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn offer(ctx: &mut Ctx, item: &Item, burned: u32) -> Flow {
    let relic = item.kind.source();
    let seat = item.controller;
    let Some(might) = burned_unit_might(ctx, burned) else {
        ctx.narrate(format!(
            "{{card {burned}}} is not a unit · {{card {relic}}} gives nothing"
        ));
        return done();
    };
    if beneficiaries(ctx, item, Stage(GIVE)).is_empty() {
        ctx.narrate(format!(
            "{{seat {seat}}} has no unit to give the {might} Might of {{card {burned}}}"
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, GIVE, 1, 1))
}

fn kindle(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let relic = item.kind.source();
    let seat = item.controller;
    match stage.0 {
        REVEALED => {
            let Some(burned) = remembered_cards(item).first().copied() else {
                return done();
            };
            offer(ctx, item, burned)
        }
        GIVE => {
            let Some(burned) = remembered_cards(item).first().copied() else {
                return done();
            };
            let Some(might) = burned_unit_might(ctx, burned) else {
                return done();
            };
            let offered = beneficiaries(ctx, item, stage);
            let Some(unit) = ctx
                .picks()
                .first()
                .copied()
                .filter(|unit| offered.contains(&TargetRef::Card(*unit)))
            else {
                return done();
            };
            might_this_turn(ctx, item, unit, might, None);
            ctx.narrate(format!(
                "{{card {unit}}} gets +{might} Might this turn · the Might of the burned {{card {burned}}}"
            ));
            done()
        }
        _ => {
            let Some(burned) = burn_cards(ctx, seat, BURN).first().copied() else {
                ctx.narrate(format!(
                    "{{card {relic}}} burns nothing · the deck is empty"
                ));
                return done();
            };
            remember_card(ctx, burned);
            if !faceless(ctx, &[burned]).is_empty() {
                return Flow::Ask(ctx.await_faces(item, &[burned], REVEALED));
            }
            offer(ctx, item, burned)
        }
    }
}

pub static CARD: Card = gear(
    "Forgotten Relic",
    &[],
    &[
        asking(with_candidates(play(&[], kindle), beneficiaries), QUESTION),
        asking(
            with_candidates(
                triggered(Trigger::BeginningPhase, &[], kindle),
                beneficiaries,
            ),
            QUESTION,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, KIND_SPELL, KIND_UNIT};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, expiry, phases, priority, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, Phase, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::prompt::PickRefusal;
    use agni_plugin_sdk::table::{CardInfo, Face};

    const RELIC: u32 = 90;
    const ALLY: u32 = 91;
    const DECK_TOP: u32 = 23;
    const THEIR_DECK_TOP: u32 = 25;

    fn relic(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Chaos".into()],
            ..fixtures::gear(RELIC, zone, seat, "Forgotten Relic", 5)
        }
    }

    fn crypt(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(relic(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Wailer", 2));
        for id in [46, 47] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Chaos", false));
        }
        for id in [40, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(RELIC).unwrap(), &CARD));
        fixture
    }

    fn buried_unit(might: u8) -> Face {
        Face::named("Buried Colossus")
            .with_kind(KIND_UNIT)
            .with_might(Some(might))
    }

    fn buried_spell() -> Face {
        Face::named("Buried Spark").with_kind(KIND_SPELL)
    }

    fn burned(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Burned { seat: who, card } if *who == seat => Some(*card),
                _ => None,
            })
            .collect()
    }

    fn relic_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == RELIC))
            .count()
    }

    fn parked_on_the_burned_face(ctx: &Ctx) {
        assert_eq!(trash_top(ctx, 0), Some(DECK_TOP));
        assert_eq!(
            ctx.kind_of(DECK_TOP),
            None,
            "the face is not in the fold yet"
        );
        let parked = ctx.blob.chain.last().unwrap();
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [DECK_TOP]);
        assert_eq!(parked.targets, [TargetRef::Card(DECK_TOP)]);
        assert!(ctx.blob.prompt.is_none(), "the host's reveal is awaited");
    }

    fn beginning(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(relic_items(&ctx), 1);
        pass_until_parked(&mut ctx);
        assert_eq!(burned(&ctx, 0), [DECK_TOP]);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, face: Face) {
        let action = Action::Reveal {
            card: DECK_TOP,
            face,
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, DECK_TOP).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn their_turn(fixture: &mut Fixture) {
        fixture.blob.core_mut().unwrap().turn = 2;
        fixture.blob.core_mut().unwrap().player = 1;
    }

    #[test]
    fn the_script_is_a_keywordless_gear_whose_play_and_beginning_phase_triggers_share_one_run() {
        assert!(std::ptr::eq(script_of("Forgotten Relic").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let on_play = &CARD.abilities[usize::from(ON_PLAY)];
        assert_eq!(on_play.trigger, Trigger::Play);
        let on_beginning = &CARD.abilities[usize::from(ON_BEGINNING)];
        assert_eq!(on_beginning.trigger, Trigger::BeginningPhase);
        for ability in [on_play, on_beginning] {
            assert!(!ability.optional);
            assert!(ability.targets.is_empty());
            assert!(ability.candidates.is_some());
            assert_eq!(ability.question, Some(QUESTION));
            assert_eq!(ability.burn, 0, "Burn 1 is the effect, not a cost");
        }
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(BURN, 1);
    }

    #[test]
    fn at_your_beginning_phase_it_burns_one_waits_for_the_face_and_the_burned_units_might_goes_to_a_friendly_unit(
    ) {
        let mut fixture = crypt(fixtures::BASE);
        assert_eq!(fixture.ctx().top_of(fixtures::MAIN_DECK, 0, 1), [DECK_TOP]);
        beginning(&mut fixture);
        parked_on_the_burned_face(&fixture.ctx());
        arrives(&mut fixture, buried_unit(4));
        let mut ctx = fixture.ctx();
        assert_eq!(burned_unit_might(&ctx, DECK_TOP), Some(4));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: GIVE
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {ALLY}}}")
            ],
            "every friendly unit, the enemy's none, no skip"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {RELIC}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(ALLY), 2 + 4);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {ALLY}}} gets +4 Might this turn · the Might of the burned {{card {DECK_TOP}}}"
        )));
        expiry::at_expiration(&mut ctx);
        assert_eq!(ctx.current_might(ALLY), 2, "this turn only");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn playing_it_burns_one_the_same_way_and_pays_five_energy() {
        let mut fixture = crypt(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, RELIC).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 5);
        assert_eq!(ctx.location(RELIC), Some(Location::Base(0)));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: ON_PLAY } if source == RELIC
        ));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(burned(&ctx, 0), [DECK_TOP]);
        parked_on_the_burned_face(&ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        arrives(&mut fixture, buried_unit(1));
        let mut ctx = fixture.ctx();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume { stage: GIVE, .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.current_might(fixtures::VI), 3 + 1);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_burned_spell_gives_nothing_a_faced_top_asks_at_once_and_an_empty_deck_burns_nothing() {
        let mut fixture = crypt(fixtures::BASE);
        beginning(&mut fixture);
        parked_on_the_burned_face(&fixture.ctx());
        arrives(&mut fixture, buried_spell());
        let ctx = fixture.ctx();
        assert_eq!(burned_unit_might(&ctx, DECK_TOP), None);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DECK_TOP}}} is not a unit · {{card {RELIC}}} gives nothing"
        )));
        drop(ctx);

        let mut faced = crypt(fixtures::BASE);
        faced.table.cards.retain(|card| card.id != DECK_TOP);
        faced.table.cards.push(fixtures::unit(
            DECK_TOP,
            fixtures::MAIN_DECK,
            0,
            "Buried Colossus",
            4,
        ));
        faced.resolve();
        let mut ctx = faced.ctx();
        phases::start_turn(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Resume { stage: GIVE, .. })),
            "a face already in the fold skips the wait"
        );
        drop(ctx);

        let mut fixture = crypt(fixtures::BASE);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(burned(&ctx, 0).is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {RELIC}}} burns nothing · the deck is empty"
        )));
        assert_eq!(
            ctx.top_of(fixtures::MAIN_DECK, 1, 1),
            [THEIR_DECK_TOP],
            "the opponent's deck is untouched"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_beginning_phase_a_relic_in_hand_and_a_wrong_seats_answer_are_refused() {
        let mut fixture = crypt(fixtures::BASE);
        their_turn(&mut fixture);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(relic_items(&ctx), 0, "your Beginning Phase only");
        fixtures::pass_until_open(&mut ctx);
        assert!(burned(&ctx, 0).is_empty());
        assert!(burned(&ctx, 1).is_empty());
        drop(ctx);
        let mut fixture = crypt(fixtures::HAND);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(relic_items(&ctx), 0, "in hand it hears nothing");
        drop(ctx);
        let mut fixture = crypt(fixtures::BASE);
        beginning(&mut fixture);
        arrives(&mut fixture, buried_unit(4));
        let mut ctx = fixture.ctx();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        assert_eq!(
            fixtures::choose(&mut ctx, 1, &format!("{{card {ALLY}}}")),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(ctx.current_might(ALLY), 2);
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert_eq!(ctx.current_might(ALLY), 6);
    }
}
