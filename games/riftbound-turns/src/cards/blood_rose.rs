use super::prelude::{
    a_unit, activated, card_target, done, exhausting_self, gain_xp, gear, named, on_you_play_card,
    optional, ready, spending_xp, when, with_cost, ONE_ENERGY,
};
use super::{Card, Cost, Flow, Item, Source, Stage, Timing, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};

pub const XP_GAINED: u8 = 1;
pub const XP_SPENT: u8 = 3;
pub const BLOOM: u8 = 0;
pub const ROUSE: u8 = 1;

pub fn a_unit_of_yours(_: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::Played { kind, .. } if kind == KIND_UNIT)
}

fn bloom(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, XP_GAINED);
    done()
}

fn rouse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if ready(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} readies"));
        }
    }
    done()
}

pub static CARD: Card = gear(
    "Blood Rose",
    &[],
    &[
        when(
            optional(with_cost(on_you_play_card(&[], bloom), ONE_ENERGY)),
            a_unit_of_yours,
        ),
        named(
            spending_xp(
                exhausting_self(activated(
                    Timing::Sorcery,
                    Cost::FREE,
                    &[a_unit("a unit to ready")],
                    rouse,
                )),
                XP_SPENT,
            ),
            "ready a unit",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ROSE: u32 = 90;

    fn rose(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            exhausted,
            ..fixtures::gear(ROSE, fixtures::BASE, 0, CARD.name, 1)
        }
    }

    fn garden(xp: i32, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rose(exhausted));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_an_optional_paid_unit_play_trigger_and_a_three_xp_exhaust_activation() {
        assert!(std::ptr::eq(script_of("Blood Rose").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let bloom = &CARD.abilities[usize::from(BLOOM)];
        assert_eq!(bloom.trigger, Trigger::YouPlayCard);
        assert!(bloom.optional, "you may pay");
        assert_eq!(bloom.cost, Some(ONE_ENERGY));
        assert_eq!(bloom.self_cost, SelfCost::Auto);
        assert!(bloom.condition.is_some(), "a unit, not a spell or gear");
        assert!(bloom.targets.is_empty());
        let rouse = &CARD.abilities[usize::from(ROUSE)];
        assert_eq!(rouse.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(rouse.self_cost, SelfCost::Exhaust);
        assert_eq!(rouse.cost, Some(Cost::FREE));
        assert_eq!(rouse.xp, XP_SPENT);
        assert_eq!(rouse.targets[0].filter, crate::cards::prelude::UNIT);
        assert_eq!(rouse.label, Some("ready a unit"));
        assert_eq!((XP_GAINED, XP_SPENT), (1, 3));
    }

    #[test]
    fn playing_a_token_unit_asks_for_the_energy_too() {
        let mut fixture = garden(0, false);
        let mut ctx = fixture.ctx();
        spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true);
        settle(&mut ctx).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "a Sprite played is a unit played · the rose asks for its energy"
        );
    }

    #[test]
    fn playing_a_unit_asks_to_pay_one_energy_and_yes_gains_one_xp_when_the_trigger_resolves() {
        let mut fixture = garden(0, false);
        let mut ctx = fixture.ctx();
        let ready_runes = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "392.2 · the may is the cost confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "pay 1 energy for the {{card {ROSE}}} trigger · {{card {}}}?",
                fixtures::HAND_UNIT
            )
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready_runes - 3,
            "two for the unit, one for the trigger"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == ROSE && index == BLOOM
        ));
        assert_eq!(ctx.xp(0), 0, "the XP waits for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), i32::from(XP_GAINED));
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        assert!(
            !ctx.card(ROSE).unwrap().exhausted,
            "the pay is energy, not an exhaust"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_energy_gains_nothing_and_a_spell_or_gear_never_asks() {
        let mut fixture = garden(0, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 0);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {ROSE}}} trigger is removed · its cost is declined"
        )));
        drop(ctx);
        let mut fixture = garden(0, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert!(ctx.on_board(fixtures::HAND_GEAR));
        assert!(ctx.blob.prompt.is_none(), "gear is not a unit");
        assert!(!ctx.blob.chain.iter().any(|item| item.kind.source() == ROSE));
    }

    #[test]
    fn with_three_xp_the_activation_spends_them_exhausts_the_rose_and_readies_a_unit() {
        let mut fixture = garden(4, false);
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == ROSE && offer.index == ROUSE)
            .unwrap();
        assert!(offer.enabled);
        assert_eq!(
            offer.label,
            format!("{{card {ROSE}}}: ready a unit (3 XP, exhaust)")
        );
        activate::activate(&mut ctx, 0, ROSE, ROUSE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.card(ROSE).unwrap().exhausted);
        assert_eq!(ctx.xp(0), 1, "three of the four XP are spent as the cost");
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "nothing until it resolves"
        );
        resolve_chain(&mut ctx);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == fixtures::VI
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_two_xp_a_spent_rose_or_the_other_seat_the_activation_is_refused() {
        let mut fixture = garden(2, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, ROSE, ROUSE),
            Err(Refusal::Illegal(Reason::NotEnoughXp))
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == ROSE && offer.index == ROUSE && offer.enabled));
        assert!(!ctx.card(ROSE).unwrap().exhausted);
        assert_eq!(ctx.xp(0), 2);
        drop(ctx);
        let mut spent = garden(4, true);
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, ROSE, ROUSE),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, ROSE, ROUSE),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(ctx.xp(0), 4);
        assert!(ctx.blob.queue.is_empty());
    }
}
