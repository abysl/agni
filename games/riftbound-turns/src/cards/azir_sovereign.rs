use super::prelude::{
    asking, done, friendly_units, move_destinations, on_attack, optional, target, unit,
    with_candidates, zone_target, Location,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::march;
use crate::state::TargetRef;

const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str = "any number of your token units to move to this battlefield";
pub const HERE: TargetSpec = target(Filter::Here, 1, 1, TargetKind::Zone, "this battlefield");

fn this_battlefield(ctx: &Ctx, item: &Item) -> Option<Location> {
    let zone = zone_target(item, 0)?;
    ctx.zones
        .is_battlefield(zone)
        .then_some(Location::Battlefield(zone))
}

pub fn token_units_elsewhere(ctx: &Ctx, item: &Item) -> Vec<u32> {
    let Some(here) = this_battlefield(ctx, item) else {
        return Vec::new();
    };
    friendly_units(ctx, item.controller)
        .into_iter()
        .filter(|unit| ctx.is_token(*unit))
        .filter(|unit| ctx.location(*unit) != Some(here))
        .filter(|unit| move_destinations(ctx, *unit).contains(&here))
        .collect()
}

fn soldiers(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != STAGE_PICKED {
        return Vec::new();
    }
    token_units_elsewhere(ctx, item)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn arise(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_PICKED => {
            let Some(here) = this_battlefield(ctx, item) else {
                return done();
            };
            let offered = token_units_elsewhere(ctx, item);
            let picked: Vec<u32> = ctx
                .picks()
                .iter()
                .copied()
                .filter(|unit| offered.contains(unit))
                .collect();
            if picked.is_empty() {
                ctx.narrate(format!("{{card {me}}} summons nobody"));
                return done();
            }
            for unit in picked {
                march::effect_move(ctx, item, unit, here);
            }
            done()
        }
        _ => {
            let count = token_units_elsewhere(ctx, item).len();
            if count == 0 {
                ctx.narrate(format!("{{card {me}}} has no token unit to summon"));
                return done();
            }
            let max = u8::try_from(count).unwrap_or(u8::MAX);
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, max))
        }
    }
}

pub static CARD: Card = unit(
    "Azir - Sovereign",
    &[Keyword::Accelerate],
    &[asking(
        with_candidates(optional(on_attack(&[HERE], arise)), soldiers),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, prompts, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const AZIR: u32 = 90;
    const SOLDIER: u32 = 91;
    const SECOND_SOLDIER: u32 = 92;
    const PLAIN: u32 = 93;

    fn empire() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut azir = fixtures::unit(AZIR, fixtures::BF1, 0, "Azir - Sovereign", 4);
        azir.domain = vec!["Order".into()];
        azir.energy = Some(4);
        fixture.table.cards.push(azir);
        for id in [SOLDIER, SECOND_SOLDIER] {
            fixture
                .table
                .cards
                .push(fixtures::unit(id, fixtures::BASE, 0, "Sand Soldier", 2));
            fixture.table.tokens.push(id);
        }
        fixture.table.tokens.sort_unstable();
        fixture
            .table
            .cards
            .push(fixtures::unit(PLAIN, fixtures::BASE, 0, "Plain", 2));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(AZIR).unwrap(), &CARD));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.blob.set_contested(fixtures::BF1, Some(0));
        cleanup::run(ctx, None);
        settle(ctx).unwrap();
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        assert!(ctx.is_attacker(AZIR));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [crate::state::TargetRef::Zone(fixtures::BF1)],
            "his battlefield is locked as the trigger goes on the chain"
        );
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_prints_accelerate_and_an_optional_attack_trigger_that_names_this_battlefield() {
        assert!(std::ptr::eq(script_of("Azir - Sovereign").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Accelerate]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.optional);
        assert_eq!(ability.targets, &[HERE]);
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn attacking_offers_the_token_units_elsewhere_and_the_picks_march_to_his_battlefield() {
        let mut fixture = empire();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_PICKED
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 2));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {SOLDIER}}}"),
                format!("{{card {SECOND_SOLDIER}}}"),
                "done".to_string(),
                "skip".to_string()
            ],
            "the tokens, not the plain unit"
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SOLDIER}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(SOLDIER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(SECOND_SOLDIER), Some(Location::Base(0)));
        assert_eq!(ctx.location(PLAIN), Some(Location::Base(0)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved {
                card: SOLDIER,
                to: Location::Battlefield(fixtures::BF1),
                cause: MoveCause::Effect,
                ..
            }
        )));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn both_tokens_can_come_and_skip_brings_none() {
        let mut fixture = empire();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {SOLDIER}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND_SOLDIER}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "two picks fill the prompt");
        assert_eq!(
            ctx.location(SOLDIER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(SECOND_SOLDIER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        drop(ctx);

        let mut fixture = empire();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(SOLDIER), Some(Location::Base(0)));
        assert_eq!(ctx.location(SECOND_SOLDIER), Some(Location::Base(0)));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {AZIR}}} summons nobody")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_token_elsewhere_nothing_is_asked_and_their_tokens_are_never_offered() {
        let mut fixture = empire();
        fixture
            .table
            .cards
            .retain(|card| ![SOLDIER, SECOND_SOLDIER].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(
            ctx.is_token(fixtures::SPRITE),
            "their Sprite is a token unit"
        );
        attacks(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {AZIR}}} has no token unit to summon")));
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }
}
