use super::prelude::{
    a_unit, activated, card_target, disempowering_self, done, legend, named, on_you_empower, ready,
    RAINBOW,
};
use super::{Card, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub fn empower_me(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ctx.empower_by(me, item.controller) {
        ctx.narrate(format!("{{card {me}}} is empowered"));
    } else {
        ctx.narrate(format!("{{card {me}}} stays as it is"));
    }
    done()
}

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if ready(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} readies"));
    }
    done()
}

pub static CARD: Card = legend(
    "Ambessa - Matriarch of War",
    &[],
    &[
        named(
            disempowering_self(activated(
                Timing::Sorcery,
                RAINBOW,
                &[a_unit("a unit to ready")],
                rally,
            )),
            "ready a unit",
        ),
        on_you_empower(&[], empower_me),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Source, Trigger};
    use crate::engine::ctx::{Event, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, triggers};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CounterInfo, Target};

    const AMBESSA: u32 = fixtures::LEGEND_CARD;
    const WOLF: u32 = 90;

    fn war_room(empowered: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(AMBESSA).unwrap().name = CARD.name.into();
        let mut wolf = fixtures::unit(WOLF, fixtures::BF1, 0, "Wolf", 3);
        wolf.exhausted = true;
        fixture.table.cards.push(wolf);
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        if empowered {
            fixture.table.counters.push(CounterInfo {
                target: Target::Card(AMBESSA),
                counter: COUNTER_EMPOWERED,
                value: 1,
            });
            fixture.table.counters.sort();
        }
        fixture.resolve();
        fixture
    }

    fn source() -> Source {
        Source {
            card: AMBESSA,
            ability: 0,
        }
    }

    fn trigger_of(source: u32, controller: u8) -> ChainItem {
        ChainItem::new(
            1,
            ItemKind::Trigger { source, index: 1 },
            controller,
            Origin::Board,
        )
    }

    fn empower_room() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(AMBESSA).unwrap().name = CARD.name.into();
        let mut tome = fixtures::gear(WOLF, fixtures::BASE, 0, "Questionable Tome", 3);
        tome.domain = vec!["Mind".into()];
        fixture.table.cards.push(tome);
        fixture.resolve();
        fixture
    }

    fn resolve_all(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_is_one_rainbow_exhaust_gated_on_empowered_and_a_you_empower_trigger() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[1];
        assert_eq!(empower.trigger, Trigger::YouEmpower);
        assert!(empower.targets.is_empty());
        assert!(!empower.optional);
        let rally = &CARD.abilities[0];
        assert_eq!(rally.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(rally.cost, Some(RAINBOW));
        assert_eq!(rally.self_cost, SelfCost::Disempower);
        assert!(rally.usable.is_none());
        assert_eq!(rally.targets.len(), 1);
        assert_eq!(rally.label, Some("ready a unit"));
    }

    #[test]
    fn the_trigger_reads_you_empowering_something_else_and_not_her_or_the_opponent() {
        let mut fixture = war_room(false);
        let ctx = fixture.ctx();
        let empowered = |card: u32, by: u8| {
            triggers::matches(
                &ctx,
                Trigger::YouEmpower,
                &Event::Empowered { card, by },
                source().card,
            )
        };
        assert_eq!(empowered(fixtures::VI, 0), Some(0));
        assert_eq!(empowered(AMBESSA, 0), None, "something else");
        assert_eq!(
            empowered(fixtures::THEIR_UNIT, 0),
            Some(0),
            "when you empower · your Profiteer on their unit is you"
        );
        assert_eq!(
            empowered(fixtures::THEIR_UNIT, 1),
            None,
            "when you empower · the opponent's empower is theirs"
        );
        assert_eq!(
            empowered(fixtures::VI, 1),
            None,
            "an opponent's Sanction on your unit is not you"
        );
        assert_eq!(
            triggers::matches(
                &ctx,
                Trigger::YouEmpower,
                &Event::Disempowered { card: fixtures::VI },
                source().card
            ),
            None
        );
    }

    #[test]
    fn the_run_empowers_its_source_once_and_says_so_when_it_already_is() {
        let mut fixture = war_room(false);
        {
            let mut ctx = fixture.ctx();
            assert!(ctx.set_controller(WOLF, 1, AMBESSA));
        }
        fixture.blob = crate::state::GameBlob::decode(&fixture.blob.encode()).unwrap();
        let mut ctx = fixture.ctx();
        assert_eq!(
            empower_me(&mut ctx, &trigger_of(WOLF, 0), Stage(0)),
            Flow::Done
        );
        assert!(ctx.is_empowered(WOLF));
        assert!(ctx.events.contains(&Event::Empowered { card: WOLF, by: 0 }));
        assert_eq!(
            triggers::matches(
                &ctx,
                Trigger::YouEmpower,
                &Event::Empowered { card: WOLF, by: 0 },
                source().card,
            ),
            Some(0),
            "the saved Empower actor remains seat 0"
        );
        assert_eq!(
            empower_me(&mut ctx, &trigger_of(WOLF, 0), Stage(0)),
            Flow::Done
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {WOLF}}} is empowered")));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {WOLF}}} stays as it is")));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Empowered { card, .. } if *card == WOLF))
                .count(),
            1
        );
    }

    #[test]
    fn a_saved_empower_activation_keeps_its_actor_when_source_control_changes() {
        let mut fixture = empower_room();
        let (table, blob) = {
            let mut ctx = fixture.ctx();
            activate::activate(&mut ctx, 0, WOLF, 0).unwrap();
            assert_eq!(ctx.blob.chain.len(), 1);
            assert_eq!(ctx.blob.chain[0].controller, 0);
            assert!(ctx.set_controller(WOLF, 1, AMBESSA));
            (
                ctx.table.clone(),
                crate::state::GameBlob::decode(&ctx.blob.encode()).unwrap(),
            )
        };
        fixture.table = table;
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.controller(WOLF), 1);
        assert_eq!(ctx.blob.chain[0].controller, 0);
        resolve_all(&mut ctx);
        resolve_all(&mut ctx);
        assert!(ctx.events.contains(&Event::Empowered { card: WOLF, by: 0 }));
        assert!(!ctx.events.contains(&Event::Empowered { card: WOLF, by: 1 }));
        assert_eq!(
            triggers::matches(
                &ctx,
                Trigger::YouEmpower,
                &Event::Empowered { card: WOLF, by: 0 },
                source().card,
            ),
            Some(0)
        );
        assert_eq!(
            triggers::matches(
                &ctx,
                Trigger::YouEmpower,
                &Event::Empowered { card: WOLF, by: 1 },
                source().card,
            ),
            None
        );
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn empowered_she_readies_the_chosen_unit_for_a_rainbow_and_the_exhaust() {
        let mut fixture = war_room(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::offers(&ctx, 0)
                .iter()
                .map(|offer| (offer.label.clone(), offer.enabled))
                .collect::<Vec<_>>(),
            [(
                format!("{{card {AMBESSA}}}: ready a unit (1 any power, exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, AMBESSA, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {WOLF}}}"),
                "cancel".to_string()
            ],
            "a unit · anyone's"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {WOLF}}}")).unwrap();
        assert!(ctx.card(AMBESSA).unwrap().exhausted);
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "one rune recycles for the rainbow"
        );
        assert!(
            ctx.card(WOLF).unwrap().exhausted,
            "nothing until it resolves"
        );
        resolve_all(&mut ctx);
        assert!(!ctx.card(WOLF).unwrap().exhausted);
        assert!(ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!("{{card {WOLF}}} readies")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn unempowered_she_is_neither_offered_nor_activatable_and_exhausted_she_is_refused() {
        let mut fixture = war_room(false);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, AMBESSA, 0),
            Err(Refusal::Illegal(Reason::NotEmpowered)),
            "disempower me cannot be paid while not Empowered"
        );
        assert!(!ctx.card(AMBESSA).unwrap().exhausted);
        drop(ctx);
        let mut spent = war_room(true);
        spent.table.card_mut(AMBESSA).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, AMBESSA, 0),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, AMBESSA, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
    }

    #[test]
    fn another_friendly_card_becoming_empowered_empowers_her() {
        let mut fixture = war_room(false);
        let mut ctx = fixture.ctx();
        ctx.empower(fixtures::VI);
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "her trigger waits on the chain");
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(AMBESSA));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Empowered { card, .. } if *card == AMBESSA)));
        assert_eq!(
            ctx.blob.chain.len(),
            0,
            "her own Empowered is not something else"
        );
    }

    #[test]
    fn the_activation_disempowers_her_as_its_cost_before_the_chain() {
        let mut fixture = war_room(true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, AMBESSA, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {WOLF}}}")).unwrap();
        assert!(
            !ctx.is_empowered(AMBESSA),
            "the cost is paid at finalization"
        );
        resolve_all(&mut ctx);
        assert!(!ctx.card(WOLF).unwrap().exhausted);
        assert!(!ctx.is_empowered(AMBESSA));
    }
}
