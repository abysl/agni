use super::prelude::{
    a_unit, activated, card_target, done, exhausting_self, gear, might_this_turn, named,
};
use super::{Card, Cost, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 3;
pub const CHILL: u8 = 0;

fn chill(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, BONUS, None);
        ctx.narrate(format!("{{card {unit}}} gets +{BONUS} Might this turn"));
    }
    done()
}

pub static CARD: Card = gear(
    "Heart of Dark Ice",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            Cost::FREE,
            &[a_unit("a unit to give +3 Might this turn")],
            chill,
        )),
        "give a unit +3 Might this turn",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, phases, priority, prompts};
    use crate::state::{Expiry, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const HEART: u32 = 90;

    fn heart(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Calm".into()],
            exhausted,
            ..fixtures::gear(HEART, zone, seat, "Heart of Dark Ice", 3)
        }
    }

    fn frozen(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(heart(zone, seat, exhausted));
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_gear_with_one_free_exhaust_activation_over_any_unit() {
        assert!(std::ptr::eq(script_of("Heart of Dark Ice").unwrap(), &CARD));
        let fixture = frozen(fixtures::BASE, 0, false);
        assert!(std::ptr::eq(fixture.scripts.of_card(HEART).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[usize::from(CHILL)];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!(ability.label, Some("give a unit +3 Might this turn"));
        assert_eq!(BONUS, 3);
    }

    #[test]
    fn exhausting_the_heart_gives_the_chosen_unit_three_might_until_the_turn_ends() {
        let mut fixture = frozen(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        let offers: Vec<(String, bool)> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == HEART)
            .map(|offer| (offer.label, offer.enabled))
            .collect();
        assert_eq!(
            offers,
            [(
                format!("{{card {HEART}}}: give a unit +3 Might this turn (exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, HEART, CHILL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let offered: Vec<Option<u32>> = prompts::offered(&ctx).iter().map(|opt| opt.card).collect();
        assert_eq!(
            offered,
            [
                Some(fixtures::VI),
                Some(fixtures::SPRITE),
                Some(fixtures::THEIR_UNIT),
                None
            ],
            "any unit on either side, then cancel"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(
            ctx.card(HEART).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "and nothing else is");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "nothing until it resolves"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 5);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 3);
        let mods = &ctx.state_of(fixtures::THEIR_UNIT).unwrap().might;
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].until, Expiry::EndOfTurn(ctx.turn()));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +3 Might this turn",
            fixtures::THEIR_UNIT
        )));
        assert_eq!(
            activate::activate(&mut ctx, 0, HEART, CHILL),
            Err(Refusal::Exhausted),
            "one use per readying"
        );
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "the bonus expired with the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_friendly_unit_is_as_good_a_target_and_a_unit_gone_before_resolution_gets_nothing() {
        let mut fixture = frozen(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, HEART, CHILL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        drop(ctx);
        let mut fixture = frozen(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, HEART, CHILL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        ctx.kill(fixtures::THEIR_UNIT, crate::engine::ctx::Cause::Rule);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("gets +3 Might this turn")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_exhausted_heart_the_other_seat_and_a_heart_still_in_hand_are_refused() {
        let mut fixture = frozen(fixtures::BASE, 0, true);
        let mut ctx = fixture.ctx();
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == HEART));
        assert_eq!(
            activate::activate(&mut ctx, 0, HEART, CHILL),
            Err(Refusal::Exhausted)
        );
        drop(ctx);
        let mut fixture = frozen(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, HEART, CHILL),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(!ctx.card(HEART).unwrap().exhausted);
        drop(ctx);
        let mut fixture = frozen(fixtures::HAND, 0, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, HEART, CHILL),
            Err(Refusal::Illegal(Reason::NotInPlay))
        );
    }
}
