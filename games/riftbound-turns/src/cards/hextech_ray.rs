use super::prelude::{a_unit_at_a_battlefield, card_target, deal, done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 3;

fn ray(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Hextech Ray",
    &[Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        ray,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play;
    use crate::state::PromptWhy;
    use crate::Refusal;

    const RAY: u32 = 90;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::spell(RAY, fixtures::HAND, 0, "Hextech Ray", 1, 1));
        fixture.resolve();
        fixture
    }

    #[test]
    fn hextech_ray_is_an_action_that_deals_three_to_a_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Hextech Ray").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "cancel"],
            "only the Sprite stands at a battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::SPRITE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(
            !ctx.on_board(fixtures::SPRITE),
            "three damage kills the 3-Might Sprite"
        );
        assert_eq!(ctx.card(RAY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_in_a_base_is_refused_and_a_target_that_left_is_not_hit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAY).unwrap();
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(
                crate::engine::legal::Reason::NotALegalTarget
            )),
            "a unit in a base is not at a battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        ctx.recall(fixtures::SPRITE, false);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
    }
}
