use super::prelude::{done, draw, exhausting_self, gear, on_enemy_unit_dies, optional, when};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const DRAWS: usize = 1;

pub fn killer_of(_: &Ctx, _: u32) -> Option<u8> {
    None
}

pub fn died_stunned(_: &Ctx, _: &Event) -> bool {
    false
}

pub fn you_killed_a_stunned_enemy_unit(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Died {
        card,
        controller,
        unit: true,
        ..
    } = event
    else {
        return false;
    };
    let seat = ctx.controller(source.card);
    *controller != seat && died_stunned(ctx, event) && killer_of(ctx, *card) == Some(seat)
}

fn pray(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = gear(
    "Solari Shrine",
    &[],
    &[when(
        optional(exhausting_self(on_enemy_unit_dies(&[], pray))),
        you_killed_a_stunned_enemy_unit,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Trigger, Who};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::{Noted, PromptWhy};

    const SHRINE: u32 = 90;

    fn temple() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut shrine = fixtures::gear(SHRINE, fixtures::BASE, 0, "Solari Shrine", 3);
        shrine.domain = vec!["Calm".into()];
        fixture.table.cards.push(shrine);
        fixture.resolve();
        fixture
    }

    fn died(card: u32, controller: u8) -> Event {
        Event::Died {
            card,
            controller,
            unit: true,
            noted: Noted {
                zone: fixtures::BASE,
                might: 2,
                controller,
                alone: true,
                buffed: false,
            },
        }
    }

    fn quiet(ctx: &Ctx) -> bool {
        ctx.blob.prompt.is_none() && ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty()
    }

    #[test]
    fn the_script_is_an_optional_exhaust_trigger_on_enemy_deaths_gated_by_the_seam() {
        let fixture = temple();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SHRINE).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Solari Shrine");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitDies(Who::Enemy));
        assert!(ability.optional, "you may exhaust this");
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn the_seam_refuses_every_death_the_engine_cannot_attribute_or_read_as_stunned() {
        let mut fixture = temple();
        let ctx = fixture.ctx();
        let source = Source {
            card: SHRINE,
            ability: 0,
        };
        assert!(!you_killed_a_stunned_enemy_unit(
            &ctx,
            &died(fixtures::THEIR_UNIT, 1),
            source
        ));
        assert!(!you_killed_a_stunned_enemy_unit(
            &ctx,
            &died(fixtures::VI, 0),
            source
        ));
        assert_eq!(killer_of(&ctx, fixtures::THEIR_UNIT), None);
        assert!(!died_stunned(&ctx, &died(fixtures::THEIR_UNIT, 1)));
    }

    #[test]
    fn an_enemy_death_by_your_spell_while_stunned_does_not_yet_wake_the_shrine() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        ctx.stun(fixtures::THEIR_UNIT);
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        ctx.kill(fixtures::THEIR_UNIT, Cause::Item(7));
        settle(&mut ctx).unwrap();
        assert!(
            quiet(&ctx),
            "the seam is closed until the engine attributes kills"
        );
        assert!(!ctx.card(SHRINE).unwrap().exhausted);
        assert_eq!(ctx.hand_of(0).len(), 4);
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "Event::Died carries no killer and Noted no stunned flag (kill.rs drops the card state before the event); the shrine needs both to know you killed a stunned enemy"]
    fn killing_a_stunned_enemy_unit_asks_to_exhaust_the_shrine_and_draws_one() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        ctx.stun(fixtures::THEIR_UNIT);
        ctx.kill(fixtures::THEIR_UNIT, Cause::Item(7));
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(SHRINE).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), 5);
    }
}
