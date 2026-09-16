use super::prelude::{
    a_unit_at_a_battlefield, activated, card_target, disempowering_self, done, legend,
    might_this_turn, named, on_you_empower,
};
use super::{Card, Cost, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub use super::ambessa_matriarch_of_war::empower_me;

pub const MIGHT: i16 = -2;

fn reflect(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    might_this_turn(ctx, item, unit, MIGHT, None);
    ctx.narrate(format!("{{card {unit}}} gets {MIGHT} might this turn"));
    done()
}

pub static CARD: Card = legend(
    "Mel - Soul's Reflection",
    &[],
    &[
        named(
            disempowering_self(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[a_unit_at_a_battlefield(
                    "a unit at a battlefield to give -2 Might",
                )],
                reflect,
            )),
            "give a unit at a battlefield -2 Might",
        ),
        on_you_empower(&[], empower_me),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::{Event, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, triggers};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::{CounterInfo, Target};

    const MEL: u32 = fixtures::LEGEND_CARD;
    const BRUTE: u32 = 90;

    fn salon(empowered: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(MEL).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        if empowered {
            fixture.table.counters.push(CounterInfo {
                target: Target::Card(MEL),
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
    fn the_legend_is_one_free_exhaust_gated_on_empowered_and_a_you_empower_trigger() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[1];
        assert_eq!(empower.trigger, Trigger::YouEmpower);
        assert!(empower.targets.is_empty());
        let reflect = &CARD.abilities[0];
        assert_eq!(reflect.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(reflect.cost, Some(Cost::FREE));
        assert_eq!(reflect.self_cost, SelfCost::Disempower);
        assert!(reflect.usable.is_none());
        assert_eq!(reflect.targets.len(), 1);
        assert_eq!(MIGHT, -2);
        let mut fixture = salon(false);
        let ctx = fixture.ctx();
        let empowered = |card: u32, by: u8| {
            triggers::matches(
                &ctx,
                Trigger::YouEmpower,
                &Event::Empowered { card, by },
                MEL,
            )
        };
        assert_eq!(empowered(fixtures::VI, 0), Some(0));
        assert_eq!(empowered(MEL, 0), None, "something else");
        assert_eq!(
            empowered(BRUTE, 1),
            None,
            "the opponent's empower is theirs"
        );
        assert_eq!(
            empowered(BRUTE, 0),
            Some(0),
            "your empower of their unit is yours"
        );
    }

    #[test]
    fn empowered_she_gives_the_chosen_unit_at_a_battlefield_minus_two_for_the_turn() {
        let mut fixture = salon(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::offers(&ctx, 0)
                .iter()
                .map(|offer| (offer.label.clone(), offer.enabled))
                .collect::<Vec<_>>(),
            [(
                format!("{{card {MEL}}}: give a unit at a battlefield -2 Might (exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, MEL, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {BRUTE}}}"),
                "cancel".to_string()
            ],
            "units at battlefields · Vi and Jinx stand in bases"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.card(MEL).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert_eq!(ctx.current_might(BRUTE), 5, "nothing until it resolves");
        resolve_all(&mut ctx);
        assert_eq!(ctx.current_might(BRUTE), 3);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BRUTE}}} gets -2 might this turn")));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(BRUTE), 5, "the turn ends and it recovers");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn there_is_no_floor_and_a_one_might_unit_reads_as_zero() {
        let mut fixture = salon(true);
        fixture.table.card_mut(BRUTE).unwrap().might = Some(1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, MEL, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        resolve_all(&mut ctx);
        assert_eq!(
            ctx.current_might(BRUTE),
            0,
            "143.2.b · less than 0 reads as 0"
        );
        assert_eq!(
            ctx.state_of(BRUTE)
                .unwrap()
                .might
                .iter()
                .map(|held| held.delta)
                .collect::<Vec<i16>>(),
            [-2],
            "no minimum · the whole -2 lands"
        );
    }

    #[test]
    fn unempowered_she_is_neither_offered_nor_activatable_and_exhausted_she_is_refused() {
        let mut fixture = salon(false);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, MEL, 0),
            Err(Refusal::Illegal(Reason::NotEmpowered)),
            "disempower me cannot be paid while not Empowered"
        );
        drop(ctx);
        let mut spent = salon(true);
        spent.table.card_mut(MEL).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, MEL, 0),
            Err(Refusal::Exhausted)
        );
        drop(ctx);
        let mut nobody = salon(true);
        nobody
            .table
            .cards
            .retain(|card| card.id != BRUTE && card.id != fixtures::SPRITE);
        nobody.resolve();
        let mut ctx = nobody.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, MEL, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "no unit stands at a battlefield"
        );
    }

    #[test]
    fn another_friendly_card_becoming_empowered_empowers_her() {
        let mut fixture = salon(false);
        let mut ctx = fixture.ctx();
        ctx.empower(fixtures::VI);
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "her trigger waits on the chain");
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(MEL));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Empowered { card, .. } if *card == MEL)));
        assert!(
            ctx.blob.chain.is_empty(),
            "her own Empowered is not something else"
        );
    }

    #[test]
    fn the_activation_disempowers_her_as_its_cost_before_the_chain() {
        let mut fixture = salon(true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, MEL, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(!ctx.is_empowered(MEL), "the cost is paid at finalization");
        resolve_all(&mut ctx);
        assert_eq!(ctx.current_might(BRUTE), 3);
        assert!(!ctx.is_empowered(MEL));
    }
}
