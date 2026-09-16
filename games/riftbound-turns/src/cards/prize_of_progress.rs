use super::prelude::{done, might_this_turn, on_activated, unit, when};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const BONUS: i16 = 1;

pub fn an_activated_ability_of_a_gear(ctx: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::Activated { source, .. } if ctx.is_gear(*source))
}

fn progress(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    might_this_turn(ctx, item, me, BONUS, None);
    ctx.narrate(format!("{{card {me}}} gets +{BONUS} Might this turn"));
    done()
}

pub static CARD: Card = unit(
    "Prize of Progress",
    &[],
    &[when(
        on_activated(&[], progress),
        an_activated_ability_of_a_gear,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{activated, exhausting_self, gear, named};
    use crate::cards::{script_of, Cost, Timing, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const PRIZE: u32 = 90;
    const WHISTLE: u32 = 91;
    const THEIR_WHISTLE: u32 = 92;
    const BELL: u32 = 93;

    static WHISTLE_CARD: Card = gear(
        "Whistle",
        &[],
        &[named(
            exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], |_, _, _| {
                done()
            })),
            "toot",
        )],
    );

    static BELL_CARD: Card = unit(
        "Bell",
        &[],
        &[named(
            exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], |_, _, _| {
                done()
            })),
            "ring",
        )],
    );

    fn prize(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(PRIZE, zone, 0, "Prize of Progress", 3)
        }
    }

    fn workshop(prize_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(prize(prize_zone));
        fixture
            .table
            .cards
            .push(fixtures::gear(WHISTLE, fixtures::BASE, 0, "Whistle", 1));
        fixture.table.cards.push(fixtures::gear(
            THEIR_WHISTLE,
            fixtures::BASE,
            1,
            "Whistle",
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(BELL, fixtures::BASE, 0, "Bell", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(WHISTLE, &WHISTLE_CARD)
            .with_script(THEIR_WHISTLE, &WHISTLE_CARD)
            .with_script(BELL, &BELL_CARD);
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn its_trigger_is_queued(ctx: &Ctx) -> bool {
        ctx.blob.chain.iter().any(
            |item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == PRIZE),
        )
    }

    #[test]
    fn the_script_watches_its_controllers_activations_for_a_gear_source() {
        assert!(std::ptr::eq(script_of("Prize of Progress").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activation { of: Who::You });
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_some());
        assert_eq!(BONUS, 1);
        let mut fixture = workshop(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(PRIZE).unwrap(), &CARD));
        let source = Source {
            card: PRIZE,
            ability: 0,
        };
        let activated = |card: u32, controller: u8| Event::Activated {
            item: 1,
            source: card,
            index: 0,
            controller,
        };
        assert!(an_activated_ability_of_a_gear(
            &ctx,
            &activated(WHISTLE, 0),
            source
        ));
        assert!(
            !an_activated_ability_of_a_gear(&ctx, &activated(BELL, 0), source),
            "a unit's ability is not a gear's"
        );
        assert!(!an_activated_ability_of_a_gear(
            &ctx,
            &Event::Drew { seat: 0, nth: 1 },
            source
        ));
    }

    #[test]
    fn using_a_gears_ability_gives_it_one_might_this_turn_when_the_trigger_resolves() {
        let mut fixture = workshop(fixtures::BASE);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, WHISTLE, 0).unwrap();
        assert!(ctx.card(WHISTLE).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            !its_trigger_is_queued(&ctx),
            "the ability has to resolve first"
        );
        resolve_chain(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(its_trigger_is_queued(&ctx));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(ctx.current_might(PRIZE), 3, "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(PRIZE), 3 + i32::from(BONUS));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PRIZE}}} gets +1 Might this turn")));
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(PRIZE), 3, "this turn only");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_units_ability_and_an_opponents_gear_trigger_nothing() {
        let mut fixture = workshop(fixtures::BASE);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, BELL, 0).unwrap();
        resolve_chain(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "a unit's ability is not a gear's"
        );
        assert_eq!(ctx.current_might(PRIZE), 3);
        drop(ctx);

        let mut fixture = workshop(fixtures::BASE);
        fixture.blob.core_mut().unwrap().advance();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        activate::activate(&mut ctx, 1, THEIR_WHISTLE, 0).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "the opponent's activation is not one you use"
        );
        assert_eq!(ctx.current_might(PRIZE), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn in_hand_it_watches_nothing() {
        let mut fixture = workshop(fixtures::HAND);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, WHISTLE, 0).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "384.1 · only on the board");
        assert!(ctx.fault.is_none());
    }
}
