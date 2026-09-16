use super::prelude::{
    asking, battlefield, card_target, detach_gear, done, is_attached, on_conquer, ready, target,
    with_candidates,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const DETACH: u8 = 1;

pub const A_FRIENDLY_GEAR: TargetSpec = target(
    Filter::And(&[Filter::Gear, Filter::Friendly]),
    0,
    1,
    TargetKind::Card,
    "a gear you control to ready",
);

pub fn is_equipment(ctx: &Ctx, gear: u32) -> bool {
    ctx.script(gear).is_some_and(|script| script.is_equipment())
}

fn the_attached_equipment(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    card_target(ctx, item, 0)
        .filter(|gear| is_equipment(ctx, *gear) && is_attached(ctx, *gear))
        .map(TargetRef::Card)
        .into_iter()
        .collect()
}

fn ready_a_gear_and_offer_to_detach_it(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(gear) = card_target(ctx, item, 0) else {
        return done();
    };
    if stage.0 == DETACH {
        if ctx.picks().contains(&gear) {
            detach_gear(ctx, gear);
        } else {
            ctx.narrate(format!("{{card {gear}}} stays attached"));
        }
        return done();
    }
    if ready(ctx, gear) {
        ctx.narrate(format!("{{card {gear}}} readies"));
    }
    if the_attached_equipment(ctx, item, stage).is_empty() {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, DETACH, 0, 1))
}

pub static CARD: Card = battlefield(
    "Veiled Temple",
    &[],
    &[asking(
        with_candidates(
            on_conquer(&[A_FRIENDLY_GEAR], ready_a_gear_and_offer_to_detach_it),
            the_attached_equipment,
        ),
        "the Equipment to detach",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, attach, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const TEMPLE: u32 = fixtures::GROUNDS;
    const BOOTS: u32 = fixtures::HAND_GEAR;
    const TROVE: u32 = 90;
    const THEIR_BOOTS: u32 = 91;

    fn temple() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(TEMPLE).unwrap().name = "Veiled Temple".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let boots = fixture.table.card_mut(BOOTS).unwrap();
        boots.zone = Some(fixtures::BASE);
        boots.exhausted = true;
        let mut trove = fixtures::gear(TROVE, fixtures::BASE, 0, "Turret", 2);
        trove.exhausted = true;
        fixture.table.cards.push(trove);
        let mut theirs = fixtures::gear(THEIR_BOOTS, fixtures::BASE, 1, "Boots of Swiftness", 2);
        theirs.exhausted = true;
        fixture.table.cards.push(theirs);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(TEMPLE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn temple_item(ctx: &Ctx) -> u16 {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .find_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == TEMPLE => Some(item.id),
                _ => None,
            })
            .expect("the temple's trigger")
    }

    fn card_label(card: u32) -> String {
        format!("{{card {card}}}")
    }

    #[test]
    fn the_temple_is_a_conquer_trigger_with_one_optional_friendly_gear_target() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Veiled Temple").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert_eq!(ability.targets, &[A_FRIENDLY_GEAR]);
        assert_eq!((A_FRIENDLY_GEAR.min, A_FRIENDLY_GEAR.max), (0, 1));
        assert_eq!(A_FRIENDLY_GEAR.kind, TargetKind::Card);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some("the Equipment to detach"));
        assert!(ability.timing().is_none());
    }

    #[test]
    fn equipment_is_read_off_the_script_and_the_attached_one_is_the_only_detach_candidate() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        assert!(is_equipment(&ctx, BOOTS));
        assert!(!is_equipment(&ctx, TROVE));
        assert!(!is_equipment(&ctx, fixtures::VI));
        assert_eq!(
            attach::attach(&mut ctx, BOOTS, fixtures::VI),
            attach::Attached::Yes
        );
        conquer(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            ["skip".to_string(), card_label(BOOTS), card_label(TROVE)],
            "friendly gear only, attached or not"
        );
    }

    #[test]
    fn conquering_offers_friendly_gear_and_readying_a_loose_gear_asks_nothing_more() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0);
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
        fixtures::choose(&mut ctx, 0, &card_label(TROVE)).unwrap();
        let item = temple_item(&ctx);
        assert!(ctx.card(TROVE).unwrap().exhausted, "not before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(TROVE).unwrap().exhausted);
        assert!(ctx.card(BOOTS).unwrap().exhausted);
        assert!(
            ctx.blob.prompt.is_none(),
            "a gear that is not an Equipment asks no detach: {:?}",
            ctx.blob.why
        );
        assert!(ctx.blob.log.contains(&format!("{{card {TROVE}}} readies")));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TEMPLE}}} ability resolves")));
        assert!(!ctx.blob.chain.iter().any(|held| held.id == item));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_attached_equipment_is_readied_then_its_controller_may_detach_it() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        assert_eq!(
            attach::attach(&mut ctx, BOOTS, fixtures::VI),
            attach::Attached::Yes
        );
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, &card_label(BOOTS)).unwrap();
        let item = temple_item(&ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(BOOTS).unwrap().exhausted, "readied first");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: DETACH
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [card_label(BOOTS), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {TEMPLE}}}: choose the Equipment to detach (0 of 1)")
        );
        assert!(attach::is_attached(&ctx, BOOTS), "not before the answer");
        fixtures::choose(&mut ctx, 0, &card_label(BOOTS)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!attach::is_attached(&ctx, BOOTS));
        assert_eq!(
            attach::attachments_of(&ctx, fixtures::VI),
            Vec::<u32>::new()
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BOOTS}}} detaches from {{card {}}}",
            fixtures::VI
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_the_detach_keeps_the_readied_equipment_on_its_unit() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        assert_eq!(
            attach::attach(&mut ctx, BOOTS, fixtures::VI),
            attach::Attached::Yes
        );
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, &card_label(BOOTS)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(BOOTS).unwrap().exhausted);
        assert!(attach::is_attached(&ctx, BOOTS));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BOOTS}}} stays attached")));
    }

    #[test]
    fn a_loose_equipment_readies_without_a_detach_question_and_skipping_the_gear_readies_nothing() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, &card_label(BOOTS)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(BOOTS).unwrap().exhausted);
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut declined = temple();
        let mut ctx = declined.ctx();
        conquer(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(BOOTS).unwrap().exhausted);
        assert!(ctx.card(TROVE).unwrap().exhausted);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_friendly_gear_the_trigger_resolves_unasked_and_it_is_not_an_affordance() {
        let mut fixture = temple();
        fixture
            .table
            .cards
            .retain(|card| card.id != BOOTS && card.id != TROVE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        let item = temple_item(&ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.blob.chain.iter().any(|held| held.id == item));
        assert!(
            ctx.card(THEIR_BOOTS).unwrap().exhausted,
            "an enemy gear is never a candidate"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, TEMPLE, 0),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
    }
}
