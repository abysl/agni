use super::frisky_hunter::play_birds;
use super::prelude::{
    activated, ask_discard, asking, done, exhausting_self, friendly_units, gear, named, triggered,
    usable_if, when, win_the_game, with_candidates, Location,
};
use super::unlicensed_armory::can_discard_one;
use super::{Card, Cost, Flow, Item, Source, Stage, Timing, Trigger};
use crate::engine::ctx::{Ctx, Event};
use crate::state::TargetRef;

pub const CARDS_IN_HAND: usize = 4;
pub const UNITS_AT_BATTLEFIELDS: usize = 4;
pub const WIN: u8 = 0;
pub const HATCH: u8 = 1;
pub const DISCARDED: u8 = 1;
pub const WHERE: u8 = 2;
pub const BIRDS: usize = 1;

pub fn units_at_battlefields_of(ctx: &Ctx, seat: u8) -> usize {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| ctx.at_battlefield(*unit))
        .count()
}

pub fn exactly_four_and_four(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let seat = ctx.controller(source.card);
    matches!(event, Event::BeginningPhase { seat: turn } if *turn == seat)
        && ctx.hand_of(seat).len() == CARDS_IN_HAND
        && units_at_battlefields_of(ctx, seat) == UNITS_AT_BATTLEFIELDS
}

fn crowned(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    win_the_game(ctx, item.controller, item.kind.source());
    done()
}

fn roosts(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != WHERE {
        return Vec::new();
    }
    ctx.play_locations(item.controller)
        .into_iter()
        .filter_map(|at| ctx.zone_of(at))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn picked_roost(ctx: &Ctx, item: &Item) -> Option<Location> {
    let zone = u16::try_from(*ctx.picks().first()?).ok()?;
    let seat = item.controller;
    let at = Location::of_zone(zone, seat, &ctx.zones)?;
    ctx.play_locations(seat).contains(&at).then_some(at)
}

fn hatch(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        DISCARDED => {
            if ctx.play_locations(seat).len() > 1 {
                return Flow::Ask(ctx.ask_resume(item, WHERE, 1, 1));
            }
            play_birds(ctx, seat, Location::Base(seat), BIRDS);
            done()
        }
        WHERE => {
            let at = picked_roost(ctx, item).unwrap_or(Location::Base(seat));
            play_birds(ctx, seat, at, BIRDS);
            done()
        }
        _ => match ask_discard(ctx, item, DISCARDED) {
            Some(ask) => Flow::Ask(ask),
            None => {
                ctx.narrate(format!(
                    "{{card {}}} has nothing to discard · no Bird",
                    item.kind.source()
                ));
                done()
            }
        },
    }
}

pub static CARD: Card = gear(
    "Gutter Palace",
    &[],
    &[
        when(
            triggered(Trigger::BeginningPhase, &[], crowned),
            exactly_four_and_four,
        ),
        named(
            usable_if(
                asking(
                    with_candidates(
                        exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], hatch)),
                        roosts,
                    ),
                    "where the Bird is played",
                ),
                can_discard_one,
            ),
            "discard 1: play a 1 Might Bird with Deflect",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::frisky_hunter::{is_bird, BIRD_DEFLECT, BIRD_MIGHT};
    use crate::cards::{script_of, SelfCost};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, phases, priority, prompts};
    use crate::state::{GameBlob, ItemKind, ItemStatus, Mode, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const PALACE: u32 = 90;
    const CROWD: u32 = 91;

    fn palace(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            exhausted,
            ..fixtures::gear(PALACE, fixtures::BASE, 0, CARD.name, 4)
        }
    }

    fn slums(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(palace(exhausted));
        fixture.resolve();
        fixture
    }

    fn at_dawn(units: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.core_mut().unwrap().turn = 3;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.table.cards.push(palace(false));
        for offset in 0..units {
            let id = CROWD + offset as u32;
            fixture
                .table
                .cards
                .push(fixtures::unit(id, fixtures::BF1, 0, "Recruit", 1));
        }
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        for _ in 0..6 {
            if ctx.blob.chain.is_empty() {
                break;
            }
            let Some(holder) = priority::holder(ctx) else {
                break;
            };
            priority::pass(ctx, holder).unwrap();
        }
    }

    fn birds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        friendly_units(ctx, seat)
            .into_iter()
            .filter(|unit| is_bird(ctx, *unit))
            .collect()
    }

    #[test]
    fn the_script_is_a_conditioned_beginning_phase_win_and_a_gated_discard_exhaust_activation() {
        assert!(std::ptr::eq(script_of("Gutter Palace").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let win = &CARD.abilities[usize::from(WIN)];
        assert_eq!(win.trigger, Trigger::BeginningPhase);
        assert!(win.condition.is_some());
        assert!(!win.optional);
        assert!(win.targets.is_empty());
        let hatch = &CARD.abilities[usize::from(HATCH)];
        assert_eq!(hatch.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(hatch.self_cost, SelfCost::Exhaust);
        assert_eq!(hatch.cost, Some(Cost::FREE));
        assert!(
            hatch.usable.is_some(),
            "no card in hand, no discard, no Bird"
        );
        assert!(hatch.candidates.is_some());
        assert_eq!(hatch.question, Some("where the Bird is played"));
        assert!(hatch.targets.is_empty());
        assert_eq!((CARDS_IN_HAND, UNITS_AT_BATTLEFIELDS), (4, 4));
        assert_eq!((BIRD_MIGHT, BIRD_DEFLECT), (1, 1));
    }

    #[test]
    fn the_condition_counts_your_hand_and_your_units_at_battlefields_at_your_beginning_phase() {
        let mut fixture = at_dawn(4);
        let ctx = fixture.ctx();
        let source = Source {
            card: PALACE,
            ability: WIN,
        };
        assert_eq!(ctx.hand_of(0).len(), 4);
        assert_eq!(
            units_at_battlefields_of(&ctx, 0),
            4,
            "Vi in the base is not at a battlefield"
        );
        assert_eq!(units_at_battlefields_of(&ctx, 1), 1, "their Sprite");
        assert!(exactly_four_and_four(
            &ctx,
            &Event::BeginningPhase { seat: 0 },
            source
        ));
        assert!(
            !exactly_four_and_four(&ctx, &Event::BeginningPhase { seat: 1 }, source),
            "the other seat's Beginning Phase"
        );
        assert!(!exactly_four_and_four(
            &ctx,
            &Event::EndingStep { seat: 0 },
            source
        ));
        drop(ctx);
        let mut five = at_dawn(5);
        let ctx = five.ctx();
        assert!(
            !exactly_four_and_four(&ctx, &Event::BeginningPhase { seat: 0 }, source),
            "exactly"
        );
        drop(ctx);
        let mut light = at_dawn(4);
        light
            .table
            .cards
            .retain(|card| card.id != fixtures::HAND_HIDDEN);
        light.resolve();
        let ctx = light.ctx();
        assert!(
            !exactly_four_and_four(&ctx, &Event::BeginningPhase { seat: 0 }, source),
            "three in hand"
        );
    }

    #[test]
    fn at_your_beginning_phase_with_four_and_four_the_palace_wins_the_game_when_its_trigger_resolves(
    ) {
        let mut fixture = at_dawn(4);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert!(ctx.blob.chain.iter().any(|item| matches!(
            item.kind,
            ItemKind::Trigger { source, index } if source == PALACE && index == WIN
        )));
        assert_eq!(ctx.won, None, "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert_eq!(ctx.won, Some(0));
        assert_eq!(ctx.winner(), Some(0));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} wins the game · {{card {PALACE}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_five_units_at_battlefields_the_beginning_phase_passes_without_a_crown() {
        let mut fixture = at_dawn(5);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert!(!ctx
            .blob
            .chain
            .iter()
            .any(|item| item.kind.source() == PALACE));
        resolve_chain(&mut ctx);
        assert_eq!(ctx.won, None);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_activation_discards_one_at_resolution_and_plays_an_exhausted_deflecting_bird_in_the_base(
    ) {
        let mut fixture = slums(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == PALACE && offer.index == HATCH && offer.enabled));
        activate::activate(&mut ctx, 0, PALACE, HATCH).unwrap();
        assert!(ctx.card(PALACE).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the discard waits for resolution"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: DISCARDED
            })
        );
        assert_eq!(
            ctx.blob.chain.last().map(|top| top.status),
            Some(ItemStatus::Resolving)
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.in_trash(fixtures::HAND_SPELL));
        assert!(
            ctx.blob.prompt.is_none(),
            "the base is the only play location"
        );
        assert!(ctx.blob.chain.is_empty());
        let birds = birds_of(&ctx, 0);
        assert_eq!(birds.len(), 1);
        let bird = birds[0];
        assert_eq!(ctx.location(bird), Some(Location::Base(0)));
        assert!(ctx.card(bird).unwrap().exhausted);
        assert_eq!(ctx.current_might(bird), i32::from(BIRD_MIGHT));
        assert_eq!(ctx.deflect_of(bird), BIRD_DEFLECT);
        assert!(ctx.is_token(bird));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: crate::state::Origin::Board, .. } if *card == bird
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn holding_a_battlefield_that_takes_units_asks_where_the_bird_is_played() {
        let mut fixture = slums(false);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, PALACE, HATCH).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: WHERE
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {PALACE}}}: choose where the Bird is played (0 of 1)")
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        let birds = birds_of(&ctx, 0);
        assert_eq!(birds.len(), 1);
        assert_eq!(
            ctx.location(birds[0]),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_hand_a_spent_palace_or_the_other_seat_the_activation_is_refused() {
        let mut fixture = slums(false);
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::HAND) && card.owner == 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, PALACE, HATCH),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the usable gate refuses with the engine's one gate reason"
        );
        assert!(!ctx.card(PALACE).unwrap().exhausted);
        drop(ctx);
        let mut spent = slums(true);
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, PALACE, HATCH),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, PALACE, HATCH),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(birds_of(&ctx, 0).is_empty());
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    #[ignore = "engine gaps · the discard is a base cost paid at the pay stage (204.1.b, the Unlicensed Armory row) and the Bird is Token::Bird in engine/ctx.rs with the Bird tag and printed Deflect rather than a granted one (the Token::Recruit row)"]
    fn the_discard_is_paid_before_the_chain_and_the_bird_is_a_real_token() {
        let mut fixture = slums(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, PALACE, HATCH).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "paid with the exhaust");
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let bird = birds_of(&ctx, 0)[0];
        assert!(ctx.has_keyword(bird, crate::cards::Keyword::Deflect(1)));
        assert!(
            ctx.script(bird).is_some(),
            "a token face with its own script"
        );
    }
}
