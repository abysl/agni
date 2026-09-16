use super::prelude::{
    activated, ask_discard, disempowering_self, done, draw, legend, named, on_you_banish,
};
use super::{Card, Cost, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub use super::ambessa_matriarch_of_war::empower_me;

pub const DRAWS: usize = 1;
const STAGE_DRAW: u8 = 1;

fn living_shadow(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 != STAGE_DRAW {
        if let Some(ask) = ask_discard(ctx, item, STAGE_DRAW) {
            return Flow::Ask(ask);
        }
        ctx.narrate(format!(
            "{{seat {seat}}} has nothing to discard · {{card {}}} draws anyway",
            item.kind.source()
        ));
    }
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = legend(
    "Zed - Master of Shadows",
    &[],
    &[
        named(
            disempowering_self(activated(Timing::Action, Cost::FREE, &[], living_shadow)),
            "discard 1, then draw 1",
        ),
        on_you_banish(&[], empower_me),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger, Who};
    use crate::engine::ctx::{Event, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, triggers};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::{CounterInfo, Target};

    const ZED: u32 = fixtures::LEGEND_CARD;

    fn shadows(empowered: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(ZED).unwrap().name = CARD.name.into();
        if empowered {
            fixture.table.counters.push(CounterInfo {
                target: Target::Card(ZED),
                counter: COUNTER_EMPOWERED,
                value: 1,
            });
            fixture.table.counters.sort();
        }
        fixture.resolve();
        fixture
    }

    fn resolve_all(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_is_one_free_action_exhaust_gated_on_empowered_and_a_you_banish_trigger() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[1];
        assert_eq!(empower.trigger, Trigger::Banished(Who::You));
        assert!(empower.targets.is_empty());
        let shadow = &CARD.abilities[0];
        assert_eq!(shadow.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(shadow.cost, Some(Cost::FREE));
        assert_eq!(shadow.self_cost, SelfCost::Disempower);
        assert!(shadow.usable.is_none());
        assert!(shadow.targets.is_empty());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn the_trigger_reads_a_card_you_own_that_you_banish_and_not_the_opponents_or_their_banish() {
        let mut fixture = shadows(false);
        let ctx = fixture.ctx();
        let banished = |card: u32, owner: u8, by: u8, token: bool| {
            triggers::matches(
                &ctx,
                Trigger::Banished(Who::You),
                &Event::Banished {
                    card,
                    owner,
                    by,
                    token,
                },
                ZED,
            )
        };
        assert_eq!(banished(fixtures::HAND_SPELL, 0, 0, false), Some(0));
        assert_eq!(
            banished(fixtures::THEIR_HAND_CARD, 1, 0, false),
            None,
            "a card you own"
        );
        assert_eq!(
            banished(fixtures::HAND_SPELL, 0, 1, false),
            None,
            "you banish it, not the opponent"
        );
        assert_eq!(
            banished(fixtures::SPRITE, 0, 0, true),
            None,
            "185 · a token you own is not a card"
        );
        assert_eq!(
            triggers::matches(
                &ctx,
                Trigger::Banished(Who::You),
                &Event::Burned {
                    seat: 0,
                    card: fixtures::HAND_SPELL
                },
                ZED
            ),
            None
        );
    }

    #[test]
    fn empowered_he_asks_for_the_discard_as_the_ability_resolves_then_draws_one() {
        let mut fixture = shadows(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::offers(&ctx, 0)
                .iter()
                .map(|offer| (offer.label.clone(), offer.enabled))
                .collect::<Vec<_>>(),
            [(
                format!("{{card {ZED}}}: discard 1, then draw 1 (exhaust)"),
                true
            )]
        );
        let held = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, ZED, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target, no confirm");
        assert!(ctx.card(ZED).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_all(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: STAGE_DRAW
            }),
            "the discard is asked as the ability resolves"
        );
        assert_eq!(
            ctx.hand_of(0).len(),
            held,
            "nothing drawn before the discard"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(0).len(), held, "one out, one in");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_hand_he_draws_without_asking() {
        let mut fixture = shadows(true);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.hand_of(0).is_empty());
        activate::activate(&mut ctx, 0, ZED, 0).unwrap();
        resolve_all(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.hand_of(0).len(), 1);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} has nothing to discard · {{card {ZED}}} draws anyway"
        )));
    }

    #[test]
    fn unempowered_he_is_neither_offered_nor_activatable_and_exhausted_he_is_refused() {
        let mut fixture = shadows(false);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, ZED, 0),
            Err(Refusal::Illegal(Reason::NotEmpowered)),
            "disempower me cannot be paid while not Empowered"
        );
        assert!(!ctx.card(ZED).unwrap().exhausted);
        drop(ctx);
        let mut spent = shadows(true);
        spent.table.card_mut(ZED).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, ZED, 0),
            Err(Refusal::Exhausted)
        );
        drop(ctx);
        let mut theirs = shadows(true);
        theirs.blob.core_mut().unwrap().player = 1;
        let mut ctx = theirs.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, ZED, 0),
            Err(Refusal::NotYourTurn),
            "an Action outside a showdown needs your turn"
        );
    }

    #[test]
    fn banishing_a_card_you_own_empowers_him() {
        let mut fixture = shadows(false);
        let mut ctx = fixture.ctx();
        assert!(ctx.banish(fixtures::HAND_SPELL));
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "his trigger waits on the chain");
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(ZED));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Empowered { card, .. } if *card == ZED)));
        assert!(ctx.banish(fixtures::THEIR_HAND_CARD));
        crate::engine::settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the opponent's card is not yours"
        );
        assert!(ctx.banish_by(fixtures::HAND_UNIT, 1));
        crate::engine::settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the opponent banishing your card is not you banishing it"
        );
    }

    #[test]
    fn banishing_a_token_you_own_does_not_empower_him() {
        let mut fixture = shadows(false);
        fixture.table.card_mut(fixtures::SPRITE).unwrap().owner = 0;
        let mut ctx = fixture.ctx();
        assert!(ctx.is_token(fixtures::SPRITE));
        assert!(ctx.banish_by(fixtures::SPRITE, 0));
        assert!(ctx.card(fixtures::SPRITE).is_none());
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "185 · a token is not a card");
        assert!(!ctx.is_empowered(ZED));
    }

    #[test]
    fn the_activation_disempowers_him_as_its_cost_before_the_chain() {
        let mut fixture = shadows(true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, ZED, 0).unwrap();
        assert!(!ctx.is_empowered(ZED), "the cost is paid at finalization");
        resolve_all(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert!(!ctx.is_empowered(ZED));
    }
}
