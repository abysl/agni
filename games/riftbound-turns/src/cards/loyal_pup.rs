use super::prelude::{
    asking, done, move_destinations, optional, trigger_subject, triggered, unit, when,
    with_candidates, Location,
};
use super::{Card, Event, Flow, Item, Source, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;
use crate::engine::march;
use crate::state::{TargetRef, FLAG_DEFENDER};

const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str = "the battlefield you defend to move the pup to";

fn first_defender_at_a_battlefield(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Defends { card } = event else {
        return false;
    };
    let Some(there @ Location::Battlefield(_)) = ctx.location(*card) else {
        return false;
    };
    ctx.on_board(source.card)
        && ctx.location(source.card) != Some(there)
        && ctx
            .designated(FLAG_DEFENDER)
            .into_iter()
            .find(|unit| ctx.location(*unit) == Some(there))
            == Some(*card)
}

pub fn defended_battlefield(ctx: &Ctx, item: &Item) -> Option<Location> {
    trigger_subject(item)
        .and_then(|defender| ctx.location(defender))
        .filter(|there| matches!(there, Location::Battlefield(_)))
        .or_else(|| {
            ctx.blob
                .showdown
                .as_ref()
                .filter(|held| held.combat && held.defender == item.controller)
                .map(|held| Location::Battlefield(held.zone))
        })
}

pub fn there(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let me = item.kind.source();
    let Some(there) = defended_battlefield(ctx, item) else {
        return Vec::new();
    };
    if !ctx.on_board(me) || !move_destinations(ctx, me).contains(&there) {
        return Vec::new();
    }
    ctx.zone_of(there)
        .map(|(zone, _)| vec![TargetRef::Zone(zone)])
        .unwrap_or_default()
}

fn follow(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_PICKED => {
            let offered = there(ctx, item, stage);
            let Some(zone) = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .filter(|zone| offered.contains(&TargetRef::Zone(*zone)))
            else {
                ctx.narrate(format!("{{card {me}}} stays put"));
                return done();
            };
            march::effect_move(ctx, item, me, Location::Battlefield(zone));
            done()
        }
        _ => {
            if there(ctx, item, Stage(STAGE_PICKED)).is_empty() {
                ctx.narrate(format!("{{card {me}}} has nowhere to go"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Loyal Pup",
    &[],
    &[asking(
        with_candidates(
            optional(when(
                triggered(Trigger::Defends(Who::You), &[], follow),
                first_defender_at_a_battlefield,
            )),
            there,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::script_of;
    use crate::engine::ctx::MoveCause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const PUP: u32 = 90;
    const SECOND: u32 = 91;

    fn kennel() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut pup = fixtures::unit(PUP, fixtures::BASE, 0, "Loyal Pup", 3);
        pup.domain = vec!["Chaos".into()];
        pup.energy = Some(3);
        fixture.table.cards.push(pup);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PUP).unwrap(), &CARD));
        fixture
    }

    fn they_attack(ctx: &mut Ctx) {
        ctx.actor = 1;
        ctx.move_unit(
            fixtures::THEIR_UNIT,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        cleanup::run(ctx, None);
        settle(ctx).unwrap();
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        assert!(ctx.is_defender(fixtures::VI));
    }

    fn pup_trigger(ctx: &Ctx) -> Option<u16> {
        ctx.blob
            .chain
            .iter()
            .find(
                |held| matches!(held.kind, ItemKind::Trigger { source, index: 0 } if source == PUP),
            )
            .map(|held| held.id)
    }

    #[test]
    fn the_script_is_a_unit_with_one_optional_conditional_you_defend_trigger() {
        assert!(std::ptr::eq(script_of("Loyal Pup").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Defends(Who::You));
        assert!(ability.optional);
        assert!(ability.condition.is_some());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn defending_at_a_battlefield_offers_it_and_the_pup_moves_there_and_joins_the_defence() {
        let mut fixture = kennel();
        let mut ctx = fixture.ctx();
        they_attack(&mut ctx);
        let item = pup_trigger(&ctx).expect("the pup's trigger is on the chain");
        assert_eq!(ctx.blob.chain[0].controller, 0);
        pass_until_parked(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: STAGE_PICKED
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{zone {}}}", fixtures::BF1), "skip".to_string()]
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the attacker does not answer for the pup"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(PUP),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.is_defender(PUP),
            "464.2.c.3.a · arriving during the combat, the pup defends"
        );
        assert!(
            pup_trigger(&ctx).is_none(),
            "383.4.f.2.a · its own arrival is not a second you-defend"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_leaves_the_pup_home_and_two_defenders_fire_it_once() {
        let mut fixture = kennel();
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 0, "Second", 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        they_attack(&mut ctx);
        assert_eq!(
            ctx.blob
                .chain
                .iter()
                .filter(
                    |held| matches!(held.kind, ItemKind::Trigger { source, .. } if source == PUP)
                )
                .count(),
            1,
            "two defenders, one defend"
        );
        pass_until_parked(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.location(PUP), Some(Location::Base(0)));
        assert!(ctx.blob.log.contains(&format!("{{card {PUP}}} stays put")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_pup_already_there_or_an_attack_of_my_own_fires_nothing() {
        let mut fixture = kennel();
        fixture.table.card_mut(PUP).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        they_attack(&mut ctx);
        assert!(
            pup_trigger(&ctx).is_none(),
            "already at the battlefield it defends"
        );
        drop(ctx);

        let mut fixture = kennel();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(ctx.is_attacker(fixtures::VI));
        assert!(pup_trigger(&ctx).is_none(), "attacking is not defending");
        assert!(ctx.fault.is_none());
    }
}
