use super::prelude::{asking, done, play, seat_target, spell, target, with_candidates};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::engine::targets;
use crate::state::TargetRef;

pub const AWAITED: u8 = 1;
pub const PICKED: u8 = 2;
pub const QUESTION: &str = "a non-unit card from their hand to recycle";
pub const AN_OPPONENT: TargetSpec = target(Filter::Enemy, 1, 1, TargetKind::Seat, "an opponent");
pub const NON_UNIT_IN_HAND: Filter = Filter::And(&[
    Filter::InHand,
    Filter::Enemy,
    Filter::Not(&Filter::Kind(KIND_UNIT)),
]);

pub fn revealed_matching(ctx: &Ctx, item: &Item, filter: Filter) -> Vec<TargetRef> {
    let Some(opponent) = seat_target(item, 0) else {
        return Vec::new();
    };
    let spec = target(filter, 1, 1, TargetKind::Card, "");
    targets::candidates(ctx, item, &spec)
        .into_iter()
        .filter(|held| matches!(held, TargetRef::Card(card) if ctx.controller(*card) == opponent))
        .collect()
}

pub fn reveal_and_pick(ctx: &mut Ctx, item: &Item, stage: Stage, filter: Filter) -> Flow {
    match stage.0 {
        AWAITED => offer(ctx, item, filter),
        PICKED => recycle(ctx, item, filter),
        _ => reveal(ctx, item, filter),
    }
}

fn reveal(ctx: &mut Ctx, item: &Item, filter: Filter) -> Flow {
    let Some(opponent) = seat_target(item, 0) else {
        return done();
    };
    let hand = ctx.hand_of(opponent);
    if hand.is_empty() {
        ctx.narrate(format!("{{seat {opponent}}} has no cards in hand"));
        return done();
    }
    ctx.narrate(format!("{{seat {opponent}}} reveals their hand"));
    let unseen: Vec<u32> = hand
        .into_iter()
        .filter(|card| !ctx.table.is_revealed(*card))
        .collect();
    if unseen.is_empty() {
        return offer(ctx, item, filter);
    }
    Flow::Ask(ctx.await_faces(item, &unseen, AWAITED))
}

fn offer(ctx: &mut Ctx, item: &Item, filter: Filter) -> Flow {
    if revealed_matching(ctx, item, filter).is_empty() {
        ctx.narrate(format!(
            "{{card {}}} finds nothing to recycle",
            item.kind.source()
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, PICKED, 1, 1))
}

fn recycle(ctx: &mut Ctx, item: &Item, filter: Filter) -> Flow {
    let offered = revealed_matching(ctx, item, filter);
    let Some(card) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| offered.contains(&TargetRef::Card(*card)))
    else {
        return done();
    };
    let owner = ctx.controller(card);
    ctx.recycle_to_bottom(card);
    ctx.narrate(format!("{{seat {owner}}} recycles {{card {card}}}"));
    done()
}

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    revealed_matching(ctx, item, NON_UNIT_IN_HAND)
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    reveal_and_pick(ctx, item, stage, NON_UNIT_IN_HAND)
}

pub static CARD: Card = spell(
    "Sabotage",
    &[],
    &[asking(
        with_candidates(play(&[AN_OPPONENT], run), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::cards::{Trigger, KIND_GEAR, KIND_SPELL};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, legal, priority, prompts, settle};
    use crate::state::{ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::{CardInfo, Face};

    pub const SPELL: u32 = 90;
    pub const THEIR_UNIT_CARD: u32 = 85;
    pub const THEIR_SPELL_CARD: u32 = 86;
    pub const THEIR_GEAR_CARD: u32 = 87;
    pub const THEIR_MIND_GEAR: u32 = 88;

    pub fn spell_card(id: u32, seat: u8, name: &str, domain: &str, power: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, name, 1, power);
        card.domain = vec![domain.into()];
        card
    }

    pub fn hand_of_three() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::THEIR_HAND_CARD);
        for id in [THEIR_UNIT_CARD, THEIR_SPELL_CARD, THEIR_GEAR_CARD] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::HAND, 1));
        }
        fixture
            .table
            .cards
            .push(spell_card(SPELL, 0, "Sabotage", "Body", 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Body", false));
        fixture.resolve();
        fixture
    }

    pub fn face_of(id: u32) -> Face {
        match id {
            THEIR_UNIT_CARD => Face::named("Jinx").with_kind(KIND_UNIT),
            THEIR_SPELL_CARD => Face::named("Charm")
                .with_kind(KIND_SPELL)
                .with_domain(vec!["Mind".into()]),
            THEIR_GEAR_CARD => Face::named("Boots")
                .with_kind(KIND_GEAR)
                .with_domain(vec!["Fury".into()]),
            THEIR_MIND_GEAR => Face::named("Codex")
                .with_kind(KIND_GEAR)
                .with_domain(vec!["Mind".into()]),
            _ => Face::named("Spark").with_kind(KIND_SPELL),
        }
    }

    pub fn cast(fixture: &mut Fixture, card: u32) -> u16 {
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, card).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        pass_until_parked(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        1
    }

    pub fn pass_until_parked(ctx: &mut Ctx) {
        for _ in 0..16 {
            let parked = ctx
                .blob
                .chain
                .last()
                .is_none_or(|top| top.status == ItemStatus::Resolving);
            if parked || ctx.blob.prompt.is_some() {
                return;
            }
            let Some(holder) = priority::holder(ctx) else {
                return;
            };
            priority::pass(ctx, holder).unwrap();
        }
    }

    pub fn reveal(fixture: &mut Fixture, card: u32) -> bool {
        let action = Action::Reveal {
            card,
            face: face_of(card),
        };
        let mut ctx = fixture.ctx_for(1, &action);
        let arrived = chain::face_arrived(&mut ctx, card).unwrap();
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        arrived
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

    #[test]
    fn the_script_targets_an_opponent_and_asks_for_a_non_unit_card() {
        assert_eq!(CARD.name, "Sabotage");
        assert!(CARD.keywords.is_empty());
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].kind, TargetKind::Seat);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_spell_parks_until_every_face_arrives_then_offers_the_spells_and_recycles_the_pick() {
        let mut fixture = hand_of_three();
        let item = cast(&mut fixture, SPELL);
        {
            let ctx = fixture.ctx();
            let parked = &ctx.blob.chain[0];
            assert_eq!(parked.id, item);
            assert_eq!(parked.status, ItemStatus::Resolving);
            assert_eq!(parked.stage, AWAITED);
            assert_eq!(
                parked.awaiting,
                [THEIR_UNIT_CARD, THEIR_SPELL_CARD, THEIR_GEAR_CARD]
            );
            assert!(ctx.blob.prompt.is_none(), "the host's reveals are awaited");
            assert!(ctx
                .blob
                .log
                .iter()
                .any(|line| line == "{seat 1} reveals their hand"));
        }
        assert!(reveal(&mut fixture, THEIR_UNIT_CARD));
        {
            let ctx = fixture.ctx();
            assert_eq!(
                ctx.blob.chain[0].awaiting,
                [THEIR_SPELL_CARD, THEIR_GEAR_CARD]
            );
            assert!(ctx.blob.prompt.is_none());
        }
        assert!(reveal(&mut fixture, THEIR_SPELL_CARD));
        assert!(reveal(&mut fixture, THEIR_GEAR_CARD));
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: PICKED
            })
        );
        assert_eq!(ctx.blob.chain[0].awaiting, Vec::<u32>::new());
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_SPELL_CARD}}}"),
                format!("{{card {THEIR_GEAR_CARD}}}")
            ],
            "the unit is not offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {SPELL}}}: choose {QUESTION} (0 of 1)")
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the chooser is the caster, not the opponent"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_GEAR_CARD}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty(), "the spell finished");
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_GEAR_CARD,
            zone: fixtures::MAIN_DECK,
            seat: 1,
            index: BOTTOM
        }));
        assert_eq!(
            ctx.card(THEIR_SPELL_CARD).unwrap().zone,
            Some(fixtures::HAND),
            "the other spell stays"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{seat 1}} recycles {{card {THEIR_GEAR_CARD}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_stale_reveal_is_no_face_and_an_already_shown_hand_skips_the_wait() {
        let mut fixture = hand_of_three();
        {
            let action = Action::Reveal {
                card: THEIR_UNIT_CARD,
                face: face_of(THEIR_UNIT_CARD),
            };
            let mut ctx = fixture.ctx_for(1, &action);
            assert!(
                !chain::face_arrived(&mut ctx, THEIR_UNIT_CARD).unwrap(),
                "no item awaits it"
            );
        }
        for card in [THEIR_UNIT_CARD, THEIR_SPELL_CARD, THEIR_GEAR_CARD] {
            fixture
                .table
                .apply_entry(
                    &Action::Reveal {
                        card,
                        face: face_of(card),
                    },
                    1,
                )
                .unwrap();
        }
        fixture.resolve();
        let item = cast(&mut fixture, SPELL);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: PICKED
            }),
            "every face is already public, so the pick opens at once"
        );
        assert!(ctx.blob.chain[0].awaiting.is_empty());
    }

    #[test]
    fn an_empty_hand_or_a_hand_of_units_ends_the_spell_quietly() {
        let mut fixture = hand_of_three();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.seat != 1);
        fixture.resolve();
        cast(&mut fixture, SPELL);
        {
            let ctx = fixture.ctx();
            assert!(ctx.blob.chain.is_empty(), "no hand, no reveal, nothing");
            assert!(ctx
                .blob
                .log
                .iter()
                .any(|line| line == "{seat 1} has no cards in hand"));
        }
        let mut units = hand_of_three();
        units
            .table
            .cards
            .retain(|card| ![THEIR_SPELL_CARD, THEIR_GEAR_CARD].contains(&card.id));
        units.resolve();
        cast(&mut units, SPELL);
        assert!(reveal(&mut units, THEIR_UNIT_CARD));
        let ctx = units.ctx();
        assert!(
            ctx.blob.prompt.is_none(),
            "no non-unit card: nothing to choose"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(THEIR_UNIT_CARD).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {SPELL}}} finds nothing to recycle")));
    }

    #[test]
    fn the_opponent_cannot_cast_it_out_of_turn_and_a_cancel_returns_it() {
        let mut fixture = hand_of_three();
        fixture
            .table
            .cards
            .push(spell_card(91, 1, "Sabotage", "Body", 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, 91)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, SPELL).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{seat 1}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(SPELL).unwrap().zone, Some(fixtures::HAND));
    }
}
