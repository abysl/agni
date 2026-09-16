use super::frisky_hunter::play_birds;
use super::prelude::{asking, done, play, seat_target, target, unit, with_candidates, Location};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const AN_OPPONENT: TargetSpec = target(
    Filter::Enemy,
    1,
    1,
    TargetKind::Seat,
    "an opponent who plays a Bird",
);
pub const QUESTION: &str = "where the Bird is played";
pub const LOCATE: u8 = 1;
pub const BIRDS: usize = 1;

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn where_the_opponent_plays_it(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != LOCATE {
        return Vec::new();
    }
    seat_target(item, 0)
        .map(|opponent| location_options(ctx, opponent))
        .unwrap_or_default()
}

fn roost(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(opponent) = seat_target(item, 0) else {
        return done();
    };
    if stage.0 == LOCATE {
        let at = ctx
            .picks()
            .first()
            .and_then(|zone| u16::try_from(*zone).ok())
            .and_then(|zone| Location::of_zone(zone, opponent, &ctx.zones))
            .filter(|at| ctx.play_locations(opponent).contains(at))
            .unwrap_or(Location::Base(opponent));
        play_birds(ctx, opponent, at, BIRDS);
        return done();
    }
    match ctx.play_locations(opponent).as_slice() {
        [] => done(),
        [only] => {
            play_birds(ctx, opponent, *only, BIRDS);
            done()
        }
        _ => Flow::Ask(ctx.ask_seat_resume(item, opponent, LOCATE, 1, 1)),
    }
}

pub static CARD: Card = unit(
    "Walking Roost",
    &[Keyword::Deflect(1)],
    &[asking(
        with_candidates(play(&[AN_OPPONENT], roost), where_the_opponent_plays_it),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::frisky_hunter::tests::birds_of;
    use crate::cards::frisky_hunter::BIRD_MIGHT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const ROOST: u32 = 90;
    const CHAOS_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn roost_card(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(ROOST, zone, 0, "Walking Roost", 6);
        card.domain = vec!["Chaos".into()];
        card.energy = Some(5);
        card
    }

    fn marsh() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(roost_card(fixtures::HAND));
        for rune in CHAOS_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ROOST).unwrap(), &CARD));
        fixture
    }

    fn walk(ctx: &mut Ctx) {
        play_engine::begin(ctx, 0, ROOST, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_deflect_unit_whose_play_trigger_chooses_an_opponent() {
        assert!(std::ptr::eq(script_of("Walking Roost").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deflect(1)]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].kind, TargetKind::Seat);
        assert_eq!(ability.targets[0].filter, Filter::Enemy);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert!(ability.candidates.is_some(), "the opponent picks where");
        assert_eq!(ability.question, Some(QUESTION));
        assert_eq!(BIRDS, 1);
    }

    #[test]
    fn the_only_opponent_is_chosen_for_them_and_picks_where_their_bird_is_played() {
        let mut fixture = marsh();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        let action = fixtures::move_action(ROOST, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        walk(&mut ctx);
        assert_eq!(ctx.location(ROOST), Some(Location::Base(0)));
        assert_eq!(ctx.deflect_of(ROOST), 1);
        assert!(
            ctx.blob.prompt.is_none(),
            "one opponent answers its own prompt"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ROOST
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Seat(1)]);
        assert!(birds_of(&ctx, 1).is_empty(), "the Bird waits for the chain");
        assert_eq!(
            ctx.play_locations(1),
            [Location::Base(1), Location::Battlefield(fixtures::BF1)],
            "seat 1 holds Proving Grounds, so it has two play locations"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume { stage: LOCATE, .. })
        ));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(
            (prompt.seat, prompt.min, prompt.max),
            (1, 1, 1),
            "the opponent plays the Bird, so the opponent picks where"
        );
        assert_eq!(fixtures::labels(&ctx), ["{zone 8}", "{zone 9}"]);
        fixtures::choose(&mut ctx, 1, "{zone 9}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        let birds = birds_of(&ctx, 1);
        assert_eq!(birds, [next]);
        let bird = birds[0];
        assert_eq!(
            ctx.location(bird),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.controller(bird), 1);
        assert_eq!(ctx.card(bird).unwrap().owner, 1, "183 · theirs to keep");
        assert!(ctx.is_token(bird));
        assert_eq!(ctx.current_might(bird), i32::from(BIRD_MIGHT));
        assert_eq!(ctx.deflect_of(bird), 1);
        assert!(ctx.card(bird).unwrap().exhausted);
        assert!(birds_of(&ctx, 0).is_empty(), "never our own");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 1}} plays {{card {bird}}} to {{zone 9}}")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_opponent_whose_only_play_location_is_their_base_gets_the_bird_there_without_a_prompt() {
        let mut fixture = marsh();
        assert_eq!(
            fixture.blob.holder(fixtures::BF2),
            Some(1),
            "seat 1 holds Rockfall Path, where units can't be played"
        );
        let action = fixtures::move_action(ROOST, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        walk(&mut ctx);
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "one location needs no pick");
        assert!(ctx.blob.chain.is_empty());
        let bird = *birds_of(&ctx, 1).first().expect("their Bird");
        assert_eq!(bird, next);
        assert_eq!(ctx.location(bird), Some(Location::Base(1)));
        assert_eq!(
            ctx.units_at(Location::Base(1)).len(),
            2,
            "Jinx and the Bird"
        );
    }

    #[test]
    fn a_roost_killed_in_response_still_makes_the_opponent_play_a_bird() {
        let mut fixture = marsh();
        let action = fixtures::move_action(ROOST, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        walk(&mut ctx);
        ctx.kill(ROOST, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(ROOST));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert_eq!(
            birds_of(&ctx, 1).len(),
            1,
            "the Bird goes to the opponent's base, not to where the Roost was"
        );
    }

    #[test]
    fn the_location_stage_refuses_a_zone_the_opponent_cannot_play_to() {
        let mut fixture = marsh();
        fixture.table.card_mut(ROOST).unwrap().zone = Some(fixtures::BASE);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let mut item = crate::state::ChainItem::new(
            7,
            ItemKind::Trigger {
                source: ROOST,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.targets.push(TargetRef::Seat(1));
        assert_eq!(
            where_the_opponent_plays_it(&ctx, &item, Stage(LOCATE)),
            [
                TargetRef::Zone(fixtures::BASE),
                TargetRef::Zone(fixtures::BF1)
            ]
        );
        assert!(where_the_opponent_plays_it(&ctx, &item, Stage(0)).is_empty());
        ctx.picked = vec![u32::from(fixtures::BF2)];
        assert_eq!(roost(&mut ctx, &item, Stage(LOCATE)), Flow::Done);
        let bird = *birds_of(&ctx, 1).first().expect("their Bird");
        assert_eq!(
            ctx.location(bird),
            Some(Location::Base(1)),
            "a held battlefield where units can't be played falls back to their base"
        );
    }
}
