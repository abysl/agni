use super::prelude::{
    asking, done, move_destinations, move_unit, on_move, optional, remember_card, remembered_cards,
    unit, with_candidates, Location,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const STAGE_PICKED: u8 = 1;
const STAGE_LOCATED: u8 = 2;
pub const QUESTION: &str =
    "an enemy unit here with less Might than me, then the battlefield it goes to";

pub fn weaker_enemies_here(ctx: &Ctx, me: u32) -> Vec<u32> {
    let Some(here) = ctx.location(me) else {
        return Vec::new();
    };
    let seat = ctx.controller(me);
    let might = ctx.current_might(me);
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat && ctx.current_might(*unit) < might)
        .filter(|unit| !other_battlefields(ctx, *unit).is_empty())
        .collect()
}

pub fn other_battlefields(ctx: &Ctx, unit: u32) -> Vec<Location> {
    move_destinations(ctx, unit)
        .into_iter()
        .filter(|to| to.battlefield().is_some())
        .collect()
}

fn challenges(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        STAGE_PICKED => weaker_enemies_here(ctx, item.kind.source())
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        STAGE_LOCATED => remembered_cards(item)
            .first()
            .map(|unit| {
                other_battlefields(ctx, *unit)
                    .into_iter()
                    .filter_map(|to| ctx.zone_of(to))
                    .map(|(zone, _)| TargetRef::Zone(zone))
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn shove(ctx: &mut Ctx, item: &Item, unit: u32, to: Location) {
    let me = item.kind.source();
    ctx.narrate(format!("{{card {me}}} shoves {{card {unit}}} away"));
    move_unit(ctx, item, unit, to);
}

fn impose(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_PICKED => {
            let Some(unit) = ctx
                .picks()
                .first()
                .copied()
                .filter(|unit| weaker_enemies_here(ctx, me).contains(unit))
            else {
                return done();
            };
            match other_battlefields(ctx, unit).as_slice() {
                [] => done(),
                [only] => {
                    shove(ctx, item, unit, *only);
                    done()
                }
                _ => {
                    remember_card(ctx, unit);
                    Flow::Ask(ctx.ask_resume(item, STAGE_LOCATED, 1, 1))
                }
            }
        }
        STAGE_LOCATED => {
            let Some(unit) = remembered_cards(item).first().copied() else {
                return done();
            };
            let to = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, ctx.controller(unit), &ctx.zones))
                .filter(|to| other_battlefields(ctx, unit).contains(to));
            if let Some(to) = to {
                shove(ctx, item, unit, to);
            }
            done()
        }
        _ => {
            if weaker_enemies_here(ctx, me).is_empty() {
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Imposing Challenger",
    &[],
    &[asking(
        with_candidates(optional(on_move(&[], impose)), challenges),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Where, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{act, legal, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const CHALLENGER: u32 = 90;
    const WEAKLING: u32 = 91;
    const BRUTE: u32 = 92;
    const PLAIN: u32 = 54;

    fn challenger(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: None,
            domain: vec!["Body".into()],
            ..fixtures::unit(CHALLENGER, zone, seat, "Imposing Challenger", 5)
        }
    }

    fn ring(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(challenger(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(WEAKLING, fixtures::BF1, 1, "Weakling", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn march(fixture: &mut Fixture, to: u16) -> Ctx<'_> {
        let action = fixtures::move_action(CHALLENGER, to, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        ctx
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn moves_of(ctx: &Ctx, card: u32) -> Vec<Location> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Moved {
                    card: moved, to, ..
                } if *moved == card => Some(*to),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_optional_asking_move_trigger() {
        assert!(std::ptr::eq(
            script_of("Imposing Challenger").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Imposing Challenger");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let moved = &CARD.abilities[0];
        assert_eq!(
            moved.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(moved.optional, "you may move");
        assert!(moved.candidates.is_some());
        assert_eq!(moved.question, Some(QUESTION));
        assert!(moved.targets.is_empty());
        assert!(moved.cost.is_none());
        assert!(moved.condition.is_none());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn marching_in_offers_the_weaker_enemy_here_then_a_different_battlefield_and_moves_it() {
        let mut fixture = ring(fixtures::BASE);
        let mut ctx = march(&mut fixture, fixtures::BF1);
        assert_eq!(
            ctx.location(CHALLENGER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CHALLENGER
        ));
        assert!(ctx.blob.prompt.is_none(), "the may is asked at resolution");
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_PICKED
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {WEAKLING}}}"), "skip".to_string()],
            "3 Might is less than 5; 5 is not"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {CHALLENGER}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {WEAKLING}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_LOCATED
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BF2),
                format!("{{zone {}}}", fixtures::BF3)
            ],
            "a different battlefield · not here, not a base"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF3)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(WEAKLING),
            Some(Location::Battlefield(fixtures::BF3))
        );
        assert_eq!(
            moves_of(&ctx, WEAKLING),
            [Location::Battlefield(fixtures::BF3)]
        );
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1)),
            "the equal stays"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {CHALLENGER}}} shoves {{card {WEAKLING}}} away"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_leaves_everyone_where_they_stand() {
        let mut fixture = ring(fixtures::BASE);
        let mut ctx = march(&mut fixture, fixtures::BF1);
        resolve_chain(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(WEAKLING),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(moves_of(&ctx, WEAKLING).is_empty());
    }

    #[test]
    fn with_one_other_battlefield_the_destination_is_not_asked() {
        let mut fixture = ring(fixtures::BASE);
        fixture.table.cards.retain(|card| card.id != PLAIN);
        fixture.resolve();
        let mut ctx = march(&mut fixture, fixtures::BF1);
        resolve_chain(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {WEAKLING}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(WEAKLING),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }

    #[test]
    fn with_no_weaker_enemy_here_the_trigger_resolves_without_asking() {
        let mut fixture = ring(fixtures::BASE);
        fixture.table.card_mut(WEAKLING).unwrap().might = Some(5);
        fixture.resolve();
        let mut ctx = march(&mut fixture, fixtures::BF1);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut fixture = ring(fixtures::BF1);
        let mut ctx = march(&mut fixture, fixtures::BASE);
        assert_eq!(ctx.location(CHALLENGER), Some(Location::Base(0)));
        resolve_chain(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "the walk home is a move, but no enemy stands in his base"
        );
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn a_friendly_weakling_here_is_not_an_enemy() {
        let mut fixture = ring(fixtures::BASE);
        fixture.table.card_mut(WEAKLING).unwrap().seat = 0;
        fixture.table.card_mut(WEAKLING).unwrap().owner = 0;
        fixture.resolve();
        let mut ctx = march(&mut fixture, fixtures::BF1);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(WEAKLING),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }
}
