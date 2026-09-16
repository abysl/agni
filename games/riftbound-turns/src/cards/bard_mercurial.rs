use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{
    asking, card_targets, done, exhaust, is_open_battlefield, move_destinations, move_unit, play,
    target, unit, when, with_candidates, Location, MOVABLE_FRIENDLY_UNIT,
};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec, KIND_LEGEND};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const ANY_NUMBER: u8 = u8::MAX;
const STAGE_DESTINATION: u8 = 1;
pub const QUESTION: &str = "an open battlefield to move the chosen units to";

pub const TRAVELLERS: TargetSpec = target(
    MOVABLE_FRIENDLY_UNIT,
    0,
    ANY_NUMBER,
    TargetKind::Card,
    "any number of your units to move",
);

pub fn legend_of(ctx: &Ctx, seat: u8) -> Option<u32> {
    let legend = ctx.zones.legend?;
    ctx.table
        .held(legend, seat)
        .find(|card| card.is_kind(KIND_LEGEND))
        .map(|card| card.id)
}

pub fn exhaust_legend_cost_payable(ctx: &Ctx, seat: u8) -> bool {
    legend_of(ctx, seat).is_some_and(|legend| ctx.card(legend).is_some_and(|held| !held.exhausted))
}

pub fn pay_exhaust_legend_cost(ctx: &mut Ctx, seat: u8) -> bool {
    let Some(legend) = legend_of(ctx, seat) else {
        return false;
    };
    if !exhaust(ctx, legend) {
        return false;
    }
    ctx.narrate(format!(
        "{{card {legend}}} is exhausted as an additional cost"
    ));
    true
}

pub fn open_destinations(ctx: &Ctx, units: &[u32]) -> Vec<Location> {
    ctx.zones
        .battlefields
        .iter()
        .copied()
        .filter(|zone| is_open_battlefield(ctx, *zone))
        .map(Location::Battlefield)
        .filter(|to| {
            units
                .iter()
                .all(|unit| move_destinations(ctx, *unit).contains(to))
        })
        .collect()
}

fn open_battlefields(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    open_destinations(ctx, &card_targets(ctx, item))
        .into_iter()
        .filter_map(|to| ctx.zone_of(to))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn travel(ctx: &mut Ctx, item: &Item, units: &[u32], to: Location) {
    for unit in units {
        move_unit(ctx, item, *unit, to);
    }
}

fn wander(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let units = card_targets(ctx, item);
    if units.is_empty() {
        ctx.narrate(format!("{{card {me}}} moves no unit"));
        return done();
    }
    match stage.0 {
        STAGE_DESTINATION => {
            let picked = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .map(Location::Battlefield)
                .filter(|to| open_destinations(ctx, &units).contains(to));
            if let Some(to) = picked {
                travel(ctx, item, &units, to);
            }
            done()
        }
        _ => match open_destinations(ctx, &units).as_slice() {
            [] => {
                ctx.narrate(format!("{{card {me}}}: no open battlefield to move to"));
                done()
            }
            [only] => {
                travel(ctx, item, &units, *only);
                done()
            }
            _ => Flow::Ask(ctx.ask_resume(item, STAGE_DESTINATION, 1, 1)),
        },
    }
}

pub static CARD: Card = unit(
    "Bard - Mercurial",
    &[],
    &[asking(
        with_candidates(
            when(play(&[TRAVELLERS], wander), paid_additional_on_entry),
            open_battlefields,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{chain, play as play_engine, priority, prompts, triggers};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BARD: u32 = 90;
    const ALLY: u32 = 91;
    const PLAIN: u32 = 54;
    const MIND_RUNE: u32 = 46;

    fn bard(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(BARD, zone, 0, "Bard - Mercurial", 4)
        }
    }

    fn wandering(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bard(zone));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Chime", 2));
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn paid_and_landed(ctx: &mut Ctx) {
        ctx.raise(Event::Played {
            card: BARD,
            controller: 0,
            kind: "Unit".into(),
            origin: Origin::Hand,
            paid_additional: true,
        });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
    }

    fn choose_all(ctx: &mut Ctx, units: &[u32]) {
        for unit in units {
            fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        }
        if matches!(ctx.blob.why, Some(PromptWhy::Target { .. })) {
            fixtures::choose(ctx, 0, "done").unwrap();
        }
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_champion_unit_whose_gated_play_trigger_moves_any_number_of_your_units() {
        assert!(std::ptr::eq(script_of("Bard - Mercurial").unwrap(), &CARD));
        assert_eq!(CARD.name, "Bard - Mercurial");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is an optional rune cost; his exhausts your legend"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(
            !ability.optional,
            "the may is the cost; any number includes none"
        );
        assert!(ability.condition.is_some());
        assert_eq!(ability.targets, &[TRAVELLERS]);
        assert_eq!((TRAVELLERS.min, TRAVELLERS.max), (0, ANY_NUMBER));
        assert_eq!(TRAVELLERS.kind, TargetKind::Card);
        assert_eq!(TRAVELLERS.filter, MOVABLE_FRIENDLY_UNIT);
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_cost_reads_your_own_ready_legend_and_paying_exhausts_it() {
        let mut fixture = wandering(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(legend_of(&ctx, 0), Some(fixtures::LEGEND_CARD));
        assert_eq!(
            legend_of(&ctx, 1),
            None,
            "the fixture seats no enemy legend"
        );
        assert!(exhaust_legend_cost_payable(&ctx, 0));
        assert!(!exhaust_legend_cost_payable(&ctx, 1));
        assert!(pay_exhaust_legend_cost(&mut ctx, 0));
        assert!(ctx.card(fixtures::LEGEND_CARD).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} is exhausted as an additional cost",
            fixtures::LEGEND_CARD
        )));
        assert!(
            !exhaust_legend_cost_payable(&ctx, 0),
            "356.2.b · an exhausted legend cannot be offered as the cost"
        );
        assert!(
            !pay_exhaust_legend_cost(&mut ctx, 0),
            "an exhausted legend cannot pay twice"
        );
        assert!(!pay_exhaust_legend_cost(&mut ctx, 1));
    }

    #[test]
    fn an_open_battlefield_is_unoccupied_and_uncontrolled_and_reachable_by_every_chosen_unit() {
        let mut fixture = wandering(fixtures::BASE);
        let ctx = fixture.ctx();
        assert_eq!(
            open_destinations(&ctx, &[fixtures::VI, ALLY]),
            [Location::Battlefield(fixtures::BF3)],
            "BF1 is held, BF2 is held and occupied by the Sprite"
        );
        assert_eq!(
            open_destinations(&ctx, &[]),
            [Location::Battlefield(fixtures::BF3)]
        );
        let mut fixture = wandering(fixtures::BASE);
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.table.card_mut(ALLY).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            open_destinations(&ctx, &[fixtures::VI, ALLY]),
            [
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF3)
            ],
            "an emptied, unheld battlefield opens up"
        );
    }

    #[test]
    fn paid_the_trigger_takes_any_number_of_your_units_and_walks_them_to_the_one_open_battlefield()
    {
        let mut fixture = wandering(fixtures::BASE);
        let mut ctx = fixture.ctx();
        paid_and_landed(&mut ctx);
        let item = 1;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                "done".to_string(),
                "skip".to_string(),
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {BARD}}}"),
                format!("{{card {ALLY}}}"),
            ],
            "your units, Bard included · any number, none included"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is not yours"
        );
        choose_all(&mut ctx, &[fixtures::VI, ALLY]);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BARD
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(ALLY)]
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "the move waits"
        );
        resolve_chain(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "one open battlefield is no choice"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF3))
        );
        assert_eq!(
            ctx.location(ALLY),
            Some(Location::Battlefield(fixtures::BF3))
        );
        assert_eq!(
            ctx.location(BARD),
            Some(Location::Base(0)),
            "not chosen, not moved"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, cause: MoveCause::Effect, .. } if *card == fixtures::VI
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_two_open_battlefields_the_resolving_trigger_asks_which_and_moves_them_there() {
        let mut fixture = wandering(fixtures::BASE);
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.table.card_mut(ALLY).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        paid_and_landed(&mut ctx);
        choose_all(&mut ctx, &[ALLY]);
        resolve_chain(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: STAGE_DESTINATION
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BF1),
                format!("{{zone {}}}", fixtures::BF3)
            ],
            "the open battlefields only"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {BARD}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(ALLY),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "not chosen"
        );
    }

    #[test]
    fn no_units_chosen_or_no_open_battlefield_moves_nothing() {
        let mut fixture = wandering(fixtures::BASE);
        let mut ctx = fixture.ctx();
        paid_and_landed(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BARD}}} moves no unit")));
        assert_eq!(
            ctx.location(ALLY),
            Some(Location::Battlefield(fixtures::BF1))
        );

        let mut fixture = wandering(fixtures::BASE);
        fixture.blob.set_holder(fixtures::BF3, Some(1));
        let mut ctx = fixture.ctx();
        paid_and_landed(&mut ctx);
        choose_all(&mut ctx, &[fixtures::VI]);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BARD}}}: no open battlefield to move to")));
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
    }

    #[test]
    fn unpaid_the_trigger_never_queues() {
        let mut fixture = wandering(fixtures::BASE);
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Played {
            card: BARD,
            controller: 0,
            kind: "Unit".into(),
            origin: Origin::Hand,
            paid_additional: false,
        });
        assert_eq!(triggers::collect(&mut ctx), 0);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · a non-resource optional additional cost (exhaust your legend) at the pay stage: play::advance knows only the rune cost in Card.additional, so the play never offers exhausting the legend, never records it as paid_additional, and the gated trigger cannot fire from a real play"]
    fn playing_him_offers_exhausting_the_legend_and_a_paid_play_fires_the_move() {
        let mut fixture = wandering(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BARD).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "exhaust your legend as an additional cost? {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(fixtures::LEGEND_CARD).unwrap().exhausted,
            "357.2 · the legend is exhausted before he enters"
        );
        assert_eq!(ctx.location(BARD), Some(Location::Base(0)));
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "the paid play queues the move trigger: {:?}",
            ctx.blob.why
        );
    }
}
