use super::prelude::{card_target, done, on_move, once_each_turn, optional, ready, target, unit};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const SOMETHING_ELSE_EXHAUSTED: Filter = Filter::And(&[
    Filter::Or(&[Filter::Unit, Filter::Gear, Filter::Rune, Filter::Legend]),
    Filter::Exhausted,
    Filter::NotSelf,
]);

pub const RALLY: TargetSpec = target(
    SOMETHING_ELSE_EXHAUSTED,
    0,
    1,
    TargetKind::Card,
    "something else that's exhausted to ready",
);

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(card) = card_target(ctx, item, 0) {
        if ready(ctx, card) {
            ctx.narrate(format!("{{card {card}}} readies"));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Miss Fortune - Captain",
    &[Keyword::Accelerate, Keyword::Ganking],
    &[once_each_turn(optional(on_move(&[RALLY], rally)))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Once, Trigger, Where, Who};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{act, expiry, legal, play as play_engine, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef, FLAG_ONCE_USED};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const CAPTAIN: u32 = 90;
    const EXHAUSTED_GEAR: u32 = 91;

    fn captain(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Body".into()],
            ..fixtures::unit(CAPTAIN, zone, seat, "Miss Fortune - Captain", 5)
        }
    }

    fn crew(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(captain(zone, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        let mut gear = fixtures::gear(EXHAUSTED_GEAR, fixtures::BASE, 0, "Boots of Swiftness", 2);
        gear.exhausted = true;
        fixture.table.cards.push(gear);
        fixture
            .table
            .card_mut(fixtures::LEGEND_CARD)
            .unwrap()
            .exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn march(fixture: &mut Fixture, unit: u32, to: u16) -> Ctx<'_> {
        let action = fixtures::move_action(unit, to, 0);
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

    fn target_item(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the move asks for something exhausted, not {other:?}"),
        }
    }

    #[test]
    fn the_script_prints_accelerate_and_ganking_and_one_optional_once_a_turn_move_trigger() {
        assert!(std::ptr::eq(
            script_of("Miss Fortune - Captain").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Miss Fortune - Captain");
        assert_eq!(CARD.keywords, [Keyword::Accelerate, Keyword::Ganking]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let first_move = &CARD.abilities[0];
        assert_eq!(
            first_move.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert_eq!(first_move.once, Once::PerTurn, "the first time each turn");
        assert!(first_move.optional, "you may ready");
        assert!(first_move.cost.is_none());
        assert!(first_move.condition.is_none());
        assert_eq!(first_move.targets.len(), 1);
        let spec = &first_move.targets[0];
        assert_eq!(spec.filter, SOMETHING_ELSE_EXHAUSTED);
        assert_eq!((spec.min, spec.max), (0, 1), "the may is a 0-of-1 target");
        assert_eq!(spec.kind, TargetKind::Card);
    }

    #[test]
    fn her_first_march_offers_every_exhausted_thing_but_herself_and_readies_the_pick() {
        let mut fixture = crew(fixtures::BASE);
        let mut ctx = march(&mut fixture, CAPTAIN, fixtures::BF1);
        assert_eq!(
            ctx.location(CAPTAIN),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.card(CAPTAIN).unwrap().exhausted,
            "the march exhausts her"
        );
        let item = target_item(&ctx);
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::RUNE_A),
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::LEGEND_CARD),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {EXHAUSTED_GEAR}}}"),
                "skip".to_string()
            ],
            "an exhausted rune, unit, legend, enemy unit and gear; never the captain herself"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {CAPTAIN}}}: choose something else that's exhausted to ready (0 of 1)")
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[CAPTAIN]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "something else"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[41]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a ready rune is not exhausted"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CAPTAIN
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "the ready waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.effects.contains(&Effect::ready(fixtures::VI)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == fixtures::VI
        )));
        assert!(ctx.card(CAPTAIN).unwrap().exhausted, "she stays exhausted");
        assert!(ctx.has_flag(CAPTAIN, FLAG_ONCE_USED));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn readying_an_exhausted_rune_is_a_legal_pick() {
        let mut fixture = crew(fixtures::BASE);
        let mut ctx = march(&mut fixture, CAPTAIN, fixtures::BF1);
        target_item(&ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::RUNE_A)).unwrap();
        resolve_chain(&mut ctx);
        assert!(!ctx.card(fixtures::RUNE_A).unwrap().exhausted);
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
    }

    #[test]
    fn her_second_move_in_the_turn_asks_nothing_and_the_next_turn_asks_again() {
        let mut fixture = crew(fixtures::BASE);
        let mut ctx = march(&mut fixture, CAPTAIN, fixtures::BF1);
        target_item(&ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the trigger resolves with no target"
        );
        resolve_chain(&mut ctx);
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "skipped: nothing readies"
        );
        assert!(
            ctx.has_flag(CAPTAIN, FLAG_ONCE_USED),
            "383.1.b · declined or not, it triggered"
        );
        ctx.ready(CAPTAIN);
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        fixture.commit(table);
        let mut ctx = march(&mut fixture, CAPTAIN, fixtures::BF2);
        assert_eq!(
            ctx.location(CAPTAIN),
            Some(Location::Battlefield(fixtures::BF2)),
            "Ganking walks battlefield to battlefield"
        );
        assert!(
            ctx.blob.prompt.is_none() || !matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "the second move of the turn is not the first: {:?}",
            ctx.blob.why
        );
        assert!(
            !ctx.blob
                .chain
                .iter()
                .any(|item| item.kind.source() == CAPTAIN),
            "no second trigger this turn"
        );
        expiry::at_expiration(&mut ctx);
        assert!(
            !ctx.has_flag(CAPTAIN, FLAG_ONCE_USED),
            "the once resets at expiration"
        );
    }

    #[test]
    fn with_nothing_else_exhausted_the_move_asks_nothing() {
        let mut fixture = crew(fixtures::BASE);
        for card in fixture.table.cards.iter_mut() {
            if card.id != CAPTAIN {
                card.exhausted = false;
            }
        }
        fixture.resolve();
        let mut ctx = march(&mut fixture, CAPTAIN, fixtures::BF1);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::GroupMove { .. })));
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the trigger resolves with nothing to ready"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { .. })));
    }

    #[test]
    fn an_exhausted_captain_may_not_march_and_a_recall_is_not_a_move() {
        let mut fixture = crew(fixtures::BASE);
        fixture.table.card_mut(CAPTAIN).unwrap().exhausted = true;
        fixture.resolve();
        let action = fixtures::move_action(CAPTAIN, fixtures::BF1, 0);
        let ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        assert_eq!(legal::classify(&ctx, 0, &entry), Err(Refusal::Exhausted));
        drop(ctx);
        let mut fixture = crew(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.recall(CAPTAIN, true);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(CAPTAIN), Some(Location::Base(0)));
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "434.1 · a recall is not a move");
        assert!(!ctx.has_flag(CAPTAIN, FLAG_ONCE_USED));
    }
}
