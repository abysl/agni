use super::prelude::{done, might_this_turn, ready, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 1;

pub static CARD: Card = unit("Eclipse Herald", &[], &[]);

pub fn when_you_stun_an_enemy_unit(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    might_this_turn(ctx, item, me, BONUS, None);
    ctx.narrate(format!("{{card {me}}} gets +{BONUS} Might this turn"));
    done()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::stun;
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::triggers;
    use crate::state::{ItemKind, Origin};

    const HERALD: u32 = 90;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut herald = fixtures::unit(HERALD, fixtures::BF1, 0, "Eclipse Herald", 7);
        herald.exhausted = true;
        fixture.table.cards.push(herald);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn trigger_item(id: u16) -> Item {
        Item::new(
            id,
            ItemKind::Trigger {
                source: HERALD,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_stub_is_registered_and_the_owed_trigger_readies_him_with_one_might_for_the_turn() {
        assert!(std::ptr::eq(script_of("Eclipse Herald").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty() && CARD.abilities.is_empty());
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            when_you_stun_an_enemy_unit(&mut ctx, &trigger_item(7), Stage(0)),
            Flow::Done
        );
        assert!(!ctx.card(HERALD).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == HERALD
        )));
        assert_eq!(ctx.current_might(HERALD), 7 + i32::from(BONUS));
        assert_eq!(
            when_you_stun_an_enemy_unit(&mut ctx, &trigger_item(8), Stage(0)),
            Flow::Done
        );
        assert_eq!(
            ctx.current_might(HERALD),
            7 + 2 * i32::from(BONUS),
            "every stun is its own trigger; a ready unit stays ready"
        );
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(HERALD), 7);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn off_the_board_the_trigger_does_nothing() {
        let mut fixture = armed();
        fixture.table.card_mut(HERALD).unwrap().zone = Some(fixtures::TRASH);
        fixture.table.card_mut(HERALD).unwrap().seat = 0;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let before = ctx.effects.len();
        assert_eq!(
            when_you_stun_an_enemy_unit(&mut ctx, &trigger_item(7), Stage(0)),
            Flow::Done
        );
        assert_eq!(ctx.effects.len(), before);
        assert!(ctx.card(HERALD).unwrap().exhausted);
    }

    #[test]
    fn today_stunning_an_enemy_unit_wakes_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(stun(&mut ctx, fixtures::THEIR_UNIT));
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert_eq!(triggers::collect(&mut ctx), 0);
        assert!(ctx.card(HERALD).unwrap().exhausted);
        assert_eq!(ctx.current_might(HERALD), 7);
    }

    #[test]
    #[ignore = "engine gap · missing triggers: Ctx::stun raises no Stunned event and Trigger has no you-stun-an-enemy-unit variant; when_you_stun_an_enemy_unit is the body the owed ability runs"]
    fn stunning_an_enemy_unit_queues_his_trigger_and_a_friendly_stun_does_not() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(stun(&mut ctx, fixtures::THEIR_UNIT));
        assert_eq!(triggers::collect(&mut ctx), 1);
        crate::engine::settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(HERALD).unwrap().exhausted);
        assert_eq!(ctx.current_might(HERALD), 7 + i32::from(BONUS));
        assert!(stun(&mut ctx, fixtures::VI));
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "a friendly unit is not an enemy"
        );
    }
}
