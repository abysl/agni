use super::prelude::{buff, card_target, done, kill, optional, target, triggered, unit, GEAR};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec, Trigger, Who};
use crate::engine::ctx::{Ctx, Killed};

pub const A_GEAR: TargetSpec = target(GEAR, 0, 1, TargetKind::Card, "a gear to kill");

fn adapt(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(gear) = card_target(ctx, item, 0) else {
        return done();
    };
    if kill(ctx, item, gear) != Killed::Yes {
        return done();
    }
    ctx.narrate(format!("{{card {gear}}} dies"));
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    }
    done()
}

pub static CARD: Card = unit(
    "Adaptatron",
    &[],
    &[optional(triggered(
        Trigger::Conquer(Who::Me),
        &[A_GEAR],
        adapt,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Filter, Trigger, Who};
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cleanup, play, priority, prompts, resume, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const ADAPTATRON: u32 = 90;
    const THEIR_GEAR: u32 = 91;
    const GEAR_NAME: &str = "Boots of Swiftness";

    fn adaptatron(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Adaptatron", 3);
        card.domain = vec!["Calm".into()];
        card.energy = Some(4);
        card
    }

    fn contested(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(adaptatron(ADAPTATRON, zone, 0));
        fixture.blob.set_contested(zone, Some(0));
        fixture.resolve();
        fixture
    }

    fn armed() -> Fixture {
        let mut fixture = contested(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, GEAR_NAME, 2));
        fixture.resolve();
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn answer(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn buffs(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    fn target_item(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the conquer asks for a gear, not {other:?}"),
        }
    }

    #[test]
    fn the_script_is_a_unit_whose_conquer_trigger_optionally_kills_one_gear() {
        assert_eq!(CARD.name, "Adaptatron");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::Me));
        assert!(ability.optional, "you may kill a gear");
        assert!(ability.cost.is_none());
        assert!(ability.extra.is_none());
        assert!(ability.condition.is_none());
        assert!(ability.candidates.is_none());
        assert_eq!(ability.targets.len(), 1);
        let spec = &ability.targets[0];
        assert_eq!(spec.filter, Filter::Gear);
        assert_eq!((spec.min, spec.max), (0, 1), "the may is a 0-of-1 target");
        assert_eq!(spec.kind, TargetKind::Card);
        assert_eq!(spec.label, "a gear to kill");
        let fixture = armed();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ADAPTATRON).unwrap(),
            &CARD
        ));
        assert!(std::ptr::eq(
            super::super::script_of("Adaptatron").unwrap(),
            &CARD
        ));
    }

    #[test]
    fn conquering_asks_for_a_gear_and_killing_it_buffs_the_adaptatron() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(ADAPTATRON), 3);
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        let item = target_item(&ctx);
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            labels(&ctx),
            [format!("{{card {THEIR_GEAR}}}"), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {ADAPTATRON}}}: choose a gear to kill (0 of 1)")
        );
        answer(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ADAPTATRON
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_GEAR)]);
        assert!(ctx.on_board(THEIR_GEAR), "the kill waits for the chain");
        assert_eq!(buffs(&ctx, ADAPTATRON), 0);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.card(THEIR_GEAR).unwrap().seat,
            1,
            "into its owner's trash"
        );
        assert_eq!(buffs(&ctx, ADAPTATRON), 1);
        assert!(ctx.is_buffed(ADAPTATRON));
        assert_eq!(ctx.current_might(ADAPTATRON), 4, "703: a buff is +1 might");
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(ADAPTATRON),
            counter: COUNTER_BUFFED,
            delta: 1
        }));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {THEIR_GEAR}}} dies")));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ADAPTATRON}}} is buffed")));
    }

    #[test]
    fn the_buff_is_a_mirror_the_adaptatron_sheds_when_it_leaves_play() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        answer(&mut ctx, 0, 0).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(buffs(&ctx, ADAPTATRON), 1);
        assert!(ctx.bounce(ADAPTATRON));
        assert_eq!(ctx.card(ADAPTATRON).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(
            buffs(&ctx, ADAPTATRON),
            0,
            "705: a unit that leaves play loses its buff"
        );
        assert!(!ctx.is_buffed(ADAPTATRON));
        assert_eq!(ctx.current_might(ADAPTATRON), 3);
        assert!(
            !buff(&mut ctx, ADAPTATRON),
            "a card in hand cannot be buffed"
        );
    }

    #[test]
    fn declining_the_prompt_kills_nothing_and_leaves_the_adaptatron_unbuffed() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(labels(&ctx).last().unwrap(), "skip");
        answer(&mut ctx, 0, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        resolve_chain(&mut ctx);
        assert!(ctx.on_board(THEIR_GEAR));
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().zone, Some(fixtures::BASE));
        assert_eq!(buffs(&ctx, ADAPTATRON), 0);
        assert_eq!(ctx.current_might(ADAPTATRON), 3);
        assert!(!ctx.blob.log.iter().any(|line| line.contains("is buffed")));
    }

    #[test]
    fn no_gear_on_the_board_asks_nothing_and_a_gear_that_left_is_never_killed() {
        let mut fixture = contested(fixtures::BF1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "no candidate and a minimum of none: the trigger asks nothing"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(buffs(&ctx, ADAPTATRON), 0);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        answer(&mut ctx, 0, 0).unwrap();
        assert!(ctx.bounce(THEIR_GEAR));
        resolve_chain(&mut ctx);
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(buffs(&ctx, ADAPTATRON), 0, "no kill, so no buff");
        assert!(!ctx.blob.log.iter().any(|line| line.contains("is buffed")));
    }

    #[test]
    fn an_already_buffed_adaptatron_kills_the_gear_and_gains_no_second_buff() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(buff(&mut ctx, ADAPTATRON));
        conquer(&mut ctx);
        answer(&mut ctx, 0, 0).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(buffs(&ctx, ADAPTATRON), 1, "702.3: one buff at a time");
        assert_eq!(ctx.current_might(ADAPTATRON), 4);
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| matches!(
                    effect,
                    Effect::Counter {
                        target: Target::Card(card),
                        counter: COUNTER_BUFFED,
                        ..
                    } if *card == ADAPTATRON
                ))
                .count(),
            1,
            "the buff it already had is the only one"
        );
        assert!(!ctx
            .blob
            .log
            .contains(&format!("{{card {ADAPTATRON}}} is buffed")));
    }

    #[test]
    fn the_other_seat_cannot_answer_the_prompt_and_a_unit_is_not_a_gear() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        let item = target_item(&ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            play::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Vi is a unit, not a gear"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt is still open");
        assert!(ctx.on_board(THEIR_GEAR));
        assert_eq!(buffs(&ctx, ADAPTATRON), 0);
    }

    #[test]
    fn a_conquer_the_adaptatron_did_not_join_triggers_nothing() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(adaptatron(ADAPTATRON, fixtures::BASE, 0));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, GEAR_NAME, 2));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.chain.is_empty(),
            "376.4.b.1: when I conquer needs me at the battlefield"
        );
        assert!(ctx.on_board(THEIR_GEAR));
        assert_eq!(buffs(&ctx, ADAPTATRON), 0);
    }
}
