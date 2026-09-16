use super::prelude::{
    a_battlefield, asking, done, play, seat_target, spell, stun, target, with_candidates,
    zone_target, LimitedPlay, Location, Price,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::state::{Origin, TargetRef};

const BATTLEFIELD: usize = 0;
const OPPONENT: usize = 1;
pub const REVEALED: u8 = 1;
pub const PICKED: u8 = 2;
pub const QUESTION: &str = "a unit from their revealed hand that they play to the battlefield";

pub const AN_OPPONENT: TargetSpec = target(Filter::Enemy, 1, 1, TargetKind::Seat, "an opponent");

pub fn units_in_hand(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.hand_of(seat)
        .into_iter()
        .filter(|card| ctx.kind_of(*card) == Some(KIND_UNIT))
        .collect()
}

pub fn opponent_plays_to_the_battlefield_ignoring_costs(
    ctx: &mut Ctx,
    opponent: u8,
    unit: u32,
    zone: u16,
) -> bool {
    if !ctx.in_hand(unit) || ctx.owner(unit) != opponent {
        ctx.narrate(format!("{{card {unit}}} is no longer in their hand"));
        return false;
    }
    if ctx.kind_of(unit) != Some(KIND_UNIT) {
        ctx.narrate(format!("{{card {unit}}} is not a unit · it stays in hand"));
        return false;
    }
    let there = Location::Battlefield(zone);
    if !ctx.zones.is_battlefield(zone)
        || ctx
            .limited_play_locations(opponent, unit, &[there])
            .is_empty()
    {
        ctx.narrate(format!(
            "{{card {unit}}} stays in hand · it can't be played to {{zone {zone}}}"
        ));
        return false;
    }
    ctx.narrate(format!(
        "{{seat {opponent}}} plays {{card {unit}}} to {{zone {zone}}}, ignoring any and all costs"
    ));
    ctx.play_limited(LimitedPlay {
        card: unit,
        by: opponent,
        origin: Origin::Hand,
        locations: vec![there],
        price: Price::Ignored,
    })
    .is_ok()
}

fn hand_units(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != PICKED {
        return Vec::new();
    }
    let Some(opponent) = seat_target(item, OPPONENT) else {
        return Vec::new();
    };
    units_in_hand(ctx, opponent)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn skewer(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let (Some(zone), Some(opponent)) =
        (zone_target(item, BATTLEFIELD), seat_target(item, OPPONENT))
    else {
        return done();
    };
    match stage.0 {
        REVEALED => {
            if units_in_hand(ctx, opponent).is_empty() {
                ctx.narrate(format!("{{seat {opponent}}} has no unit in hand"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, PICKED, 0, 1))
        }
        PICKED => {
            let Some(unit) = ctx
                .picks()
                .first()
                .copied()
                .filter(|unit| units_in_hand(ctx, opponent).contains(unit))
            else {
                ctx.narrate(format!("{{card {}}} chooses no unit", item.kind.source()));
                return done();
            };
            if opponent_plays_to_the_battlefield_ignoring_costs(ctx, opponent, unit, zone) {
                stun(ctx, unit);
            }
            done()
        }
        _ => {
            let hand = ctx.reveal_hand(opponent);
            if hand.is_empty() {
                ctx.narrate(format!("{{seat {opponent}}} has no card in hand"));
                return done();
            }
            let unseen: Vec<u32> = hand
                .into_iter()
                .filter(|card| !ctx.table.is_revealed(*card))
                .collect();
            if unseen.is_empty() {
                return skewer(ctx, item, Stage(REVEALED));
            }
            Flow::Ask(ctx.await_faces(item, &unseen, REVEALED))
        }
    }
}

pub static CARD: Card = spell(
    "Bone Skewer",
    &[Keyword::Hidden],
    &[asking(
        with_candidates(
            play(&[a_battlefield("a battlefield"), AN_OPPONENT], skewer),
            hand_units,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, KIND_SPELL};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, hide, priority, prompts, settle};
    use crate::state::{GameBlob, ItemStatus, PromptWhy};
    use agni_plugin_sdk::decide::{Action, Effect};
    use agni_plugin_sdk::table::{CardInfo, Face};

    const SKEWER: u32 = 90;
    const BRUTE: u32 = 91;
    const SPARK: u32 = 92;
    const CHAOS_RUNE: u32 = 46;

    fn face_of(id: u32) -> Face {
        match id {
            SPARK => Face::named("Spark")
                .with_kind(KIND_SPELL)
                .with_cost(Some(2), Some(1))
                .with_domain(vec!["Fury".into()]),
            _ => Face::named("Brute")
                .with_kind(KIND_UNIT)
                .with_cost(Some(6), Some(2))
                .with_might(Some(5))
                .with_domain(vec!["Fury".into()]),
        }
    }

    fn reveal_action(card: u32) -> Action {
        Action::Reveal {
            card,
            face: face_of(card),
        }
    }

    fn skewer_card() -> CardInfo {
        let mut card = fixtures::spell(SKEWER, fixtures::HAND, 0, "Bone Skewer", 2, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn hidden_hand() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(skewer_card());
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::THEIR_HAND_CARD);
        for id in [BRUTE, SPARK] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::HAND, 1));
        }
        fixture.resolve();
        fixture
    }

    fn revealed_hand() -> Fixture {
        let mut fixture = hidden_hand();
        for id in [BRUTE, SPARK] {
            fixture.table.apply_entry(&reveal_action(id), 1).unwrap();
        }
        fixture.resolve();
        fixture
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, SKEWER).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(fixtures::labels(ctx), ["{seat 1}", "cancel"]);
        fixtures::choose(ctx, 0, "{seat 1}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Zone(fixtures::BF1), TargetRef::Seat(1)]
        );
    }

    fn pass_both(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_hidden_spell_over_a_battlefield_and_an_opponent_with_a_hand_pick() {
        assert!(std::ptr::eq(script_of("Bone Skewer").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(!CARD.has_keyword(Keyword::Action));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, Filter::AtBattlefield);
        assert_eq!(ability.targets[0].kind, TargetKind::Zone);
        assert_eq!(ability.targets[1], AN_OPPONENT);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = revealed_hand();
        let ctx = fixture.ctx();
        assert_eq!(units_in_hand(&ctx, 1), [BRUTE], "the Spark is a spell");
        assert!(units_in_hand(&ctx, 0).contains(&fixtures::HAND_UNIT));
    }

    #[test]
    fn the_hand_is_revealed_the_caster_picks_a_unit_and_they_play_it_stunned_to_the_battlefield() {
        let mut fixture = hidden_hand();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        pass_both(&mut ctx);
        let parked = &ctx.blob.chain[0];
        assert_eq!(
            (parked.status, parked.stage),
            (ItemStatus::Resolving, REVEALED),
            "the spell waits for the faces"
        );
        assert_eq!(parked.awaiting, [BRUTE, SPARK]);
        for card in [BRUTE, SPARK] {
            assert!(ctx.effects.contains(&Effect::Reveal { card }));
        }
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} reveals their hand".to_string()));
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let mut ctx = fixture.ctx_for(1, &reveal_action(BRUTE));
        assert!(chain::face_arrived(&mut ctx, BRUTE).unwrap());
        assert!(ctx.blob.prompt.is_none(), "one face still to come");
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let mut ctx = fixture.ctx_for(1, &reveal_action(SPARK));
        assert!(chain::face_arrived(&mut ctx, SPARK).unwrap());
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICKED
            })
        );
        assert_eq!(
            ctx.blob.prompt.as_ref().map(|prompt| prompt.seat),
            Some(0),
            "the caster chooses"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BRUTE}}}"), "skip".to_string()],
            "the units among the revealed hand, and the may"
        );
        let runes = ctx.ready_runes_of(1).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.controller(BRUTE), 1, "their unit, played by them");
        assert!(
            ctx.card(BRUTE).unwrap().exhausted,
            "369.3 · it enters exhausted"
        );
        assert_eq!(
            ctx.ready_runes_of(1).len(),
            runes,
            "ignoring any and all costs: none of their runes is touched"
        );
        assert!(ctx.is_stunned(BRUTE));
        assert_eq!(ctx.combat_might(BRUTE), 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 1, .. } if *card == BRUTE
        )));
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(1),
            "their unit contests the open battlefield"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 1}} plays {{card {BRUTE}}} to {{zone {}}}, ignoring any and all costs",
            fixtures::BF1
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BRUTE}}} is stunned")));
        assert_eq!(ctx.card(SPARK).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(SKEWER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_public_hand_skips_the_wait_and_the_pick_may_be_declined() {
        let mut fixture = revealed_hand();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICKED
            }),
            "faces already public: straight to the pick"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SKEWER}}} chooses no unit")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_hand_without_units_and_an_empty_hand_end_the_spell_after_the_reveal() {
        let mut fixture = revealed_hand();
        fixture.table.cards.retain(|card| card.id != BRUTE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no unit in hand".to_string()));
        let mut bare = revealed_hand();
        bare.table
            .cards
            .retain(|card| card.id != BRUTE && card.id != SPARK);
        bare.resolve();
        let mut ctx = bare.ctx();
        cast(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no card in hand".to_string()));
    }

    #[test]
    fn a_battlefield_that_refuses_played_units_keeps_the_unit_in_hand() {
        let mut fixture = revealed_hand();
        let mut ctx = fixture.ctx();
        assert!(!ctx.units_played_here(fixtures::BF2), "Rockfall Path");
        assert!(!opponent_plays_to_the_battlefield_ignoring_costs(
            &mut ctx,
            1,
            BRUTE,
            fixtures::BF2
        ));
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::HAND));
        assert!(!opponent_plays_to_the_battlefield_ignoring_costs(
            &mut ctx,
            1,
            SPARK,
            fixtures::BF1
        ));
        assert!(!opponent_plays_to_the_battlefield_ignoring_costs(
            &mut ctx,
            0,
            BRUTE,
            fixtures::BF1
        ));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn played_from_facedown_it_reacts_for_nothing_and_the_hand_is_still_revealed() {
        let mut fixture = revealed_hand();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        hide::hide(&mut ctx, 0, SKEWER, fixtures::BF1).unwrap();
        ctx.blob.card_state_mut(SKEWER).hidden_since = 0;
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        hide::play_from_facedown(&mut ctx, 0, SKEWER).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 2,
                stage: PICKED
            })
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.is_stunned(BRUTE));
        assert_eq!(ctx.blob.chain.len(), 1, "Spark still waits below");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    fn revealed_hand_with_an_accelerator() -> Fixture {
        let mut fixture = hidden_hand();
        fixture
            .table
            .apply_entry(
                &Action::Reveal {
                    card: BRUTE,
                    face: Face::named("Blazing Scorcher")
                        .with_kind(KIND_UNIT)
                        .with_cost(Some(1), Some(0))
                        .with_might(Some(2))
                        .with_domain(vec!["Mind".into()]),
                },
                1,
            )
            .unwrap();
        fixture.table.apply_entry(&reveal_action(SPARK), 1).unwrap();
        fixture.resolve();
        fixture
    }

    fn ignored_accelerate_fixture() -> Fixture {
        let mut fixture = revealed_hand_with_an_accelerator();
        for card in &mut fixture.table.cards {
            if card.zone == Some(fixtures::RUNE_POOL) && card.owner == 1 {
                card.exhausted = true;
            }
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn ignored_play_offers_accelerate_and_accepting_it_keeps_zero_resources_before_stun() {
        let mut fixture = ignored_accelerate_fixture();
        let mut ctx = fixture.ctx();
        assert!(ctx.has_keyword(BRUTE, Keyword::Accelerate));
        cast(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.ready_runes_of(1).is_empty());
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: crate::state::SLOT_ACCELERATE as u8,
            })
        );
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 1, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.is_stunned(BRUTE), "the stun lands on entry");
        assert_eq!(ctx.ready_runes_of(1).len(), 0);
        assert!(!ctx.card(BRUTE).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn ignored_accelerate_decline_reloads_and_enters_exhausted_but_stunned() {
        let mut fixture = ignored_accelerate_fixture();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 1, "no").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.is_stunned(BRUTE), "the stun lands on entry");
        assert_eq!(ctx.ready_runes_of(1).len(), 0);
        assert!(ctx.card(BRUTE).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn their_play_is_reported_from_hand() {
        let mut fixture = revealed_hand();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 1, origin: Origin::Hand, .. } if *card == BRUTE
        )));
    }
}
