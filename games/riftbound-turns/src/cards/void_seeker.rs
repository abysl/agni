use super::prelude::{a_unit_at_a_battlefield, card_target, deal, done, draw, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 4;
const DRAWS: usize = 1;

fn seek(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = spell(
    "Void Seeker",
    &[Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        seek,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::PromptWhy;

    const SEEKER: u32 = 90;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            SEEKER,
            fixtures::HAND,
            0,
            "Void Seeker",
            3,
            1,
        ));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    #[test]
    fn void_seeker_is_an_action_that_deals_four_at_a_battlefield_then_draws_one() {
        assert!(std::ptr::eq(script_of("Void Seeker").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SEEKER).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::SPRITE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the spell left the hand and one card came in"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_draw_still_happens_when_the_target_has_left_the_battlefield() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SEEKER).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        ctx.recall(fixtures::SPRITE, false);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "359.3.e.5 · the draw still happens"
        );
    }
}
