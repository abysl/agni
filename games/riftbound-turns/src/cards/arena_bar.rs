use super::prelude::{a_card, activated, buff, card_target, done, exhausting_self, gear, named};
use super::{Card, Cost, Filter, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const EXHAUSTED_FRIENDLY_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::Exhausted]);

fn serve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        }
    }
    done()
}

pub static CARD: Card = gear(
    "Arena Bar",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            Cost::FREE,
            &[a_card(
                EXHAUSTED_FRIENDLY_UNIT,
                "an exhausted friendly unit to buff",
            )],
            serve,
        )),
        "buff an exhausted friendly unit",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::{activate, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const BAR: u32 = 90;
    const TIRED: u32 = 91;

    fn bar(zone: u16) -> CardInfo {
        let mut card = fixtures::gear(BAR, zone, 0, "Arena Bar", 3);
        card.domain = vec!["Body".into()];
        card
    }

    fn tavern() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bar(fixtures::BASE));
        let mut tired = fixtures::unit(TIRED, fixtures::BF1, 0, "Pit Rookie", 2);
        tired.exhausted = true;
        fixture.table.cards.push(tired);
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn its_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == BAR)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_one_exhaust_activation_aimed_at_an_exhausted_friendly_unit() {
        assert!(std::ptr::eq(script_of("Arena Bar").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let serve = &CARD.abilities[0];
        assert_eq!(serve.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(serve.self_cost, SelfCost::Exhaust);
        assert_eq!(serve.cost, Some(Cost::FREE));
        assert_eq!(serve.targets.len(), 1);
        assert_eq!(serve.targets[0].filter, EXHAUSTED_FRIENDLY_UNIT);
        assert_eq!((serve.targets[0].min, serve.targets[0].max), (1, 1));
        assert_eq!(serve.label, Some("buff an exhausted friendly unit"));
    }

    #[test]
    fn exhausting_the_bar_buffs_the_chosen_exhausted_friendly_unit_when_it_resolves() {
        let mut fixture = tavern();
        let mut ctx = fixture.ctx();
        assert_eq!(
            its_offers(&ctx),
            [(
                format!("{{card {BAR}}}: buff an exhausted friendly unit (exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, BAR, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {TIRED}}}"), "cancel".to_string()],
            "the ready Vi and the exhausted enemy are not offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {TIRED}}}")).unwrap();
        assert!(
            ctx.card(BAR).unwrap().exhausted,
            "exhausting it is the cost"
        );
        assert!(!ctx.is_buffed(TIRED), "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(TIRED));
        assert_eq!(ctx.current_might(TIRED), 3);
        assert_eq!(
            ctx.table.counter(Target::Card(TIRED), COUNTER_BUFFED),
            Some(1)
        );
        assert!(ctx.card(TIRED).unwrap().exhausted, "buffed, not readied");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TIRED}}} is buffed")));
        assert_eq!(
            activate::activate(&mut ctx, 0, BAR, 0),
            Err(Refusal::Exhausted),
            "one round per ready"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_already_buffed_unit_is_still_a_legal_choice_that_gains_no_second_buff() {
        let mut fixture = tavern();
        let mut ctx = fixture.ctx();
        ctx.buff(TIRED);
        activate::activate(&mut ctx, 0, BAR, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TIRED}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.table.counter(Target::Card(TIRED), COUNTER_BUFFED),
            Some(1),
            "426.1.c · chosen but not buffed again"
        );
        assert!(!ctx
            .blob
            .log
            .contains(&format!("{{card {TIRED}}} is buffed")));
    }

    #[test]
    fn a_unit_readied_before_the_ability_resolves_is_skipped() {
        let mut fixture = tavern();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, BAR, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TIRED}}}")).unwrap();
        assert!(ctx.ready(TIRED));
        resolve_chain(&mut ctx);
        assert!(
            !ctx.is_buffed(TIRED),
            "356.3.e · no longer an exhausted unit"
        );
        assert!(ctx.card(BAR).unwrap().exhausted, "the cost stays paid");
    }

    #[test]
    fn a_ready_unit_an_enemy_unit_and_an_empty_board_are_refused() {
        let mut fixture = tavern();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, BAR, 0).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Vi is ready"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an exhausted enemy is not friendly"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(!ctx.card(BAR).unwrap().exhausted, "taken back, unpaid");
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut fixture = tavern();
        fixture.table.card_mut(TIRED).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        assert!(its_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, BAR, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, BAR, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
    }
}
