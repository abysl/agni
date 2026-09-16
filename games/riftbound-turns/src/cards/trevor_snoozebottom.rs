use super::faithful_manufactor::recruits_playable_at;
use super::prelude::{done, location_of, on_hold_me, spawn, unit, Token};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

pub const SPRITE_ARRIVES_READY: bool = true;

fn snooze(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let me = item.kind.source();
    let Some(here) = location_of(ctx, me) else {
        ctx.narrate(format!("{{card {me}}} has left the board · no Sprite"));
        return done();
    };
    if !recruits_playable_at(ctx, here) {
        ctx.narrate(format!(
            "no Sprite · units can't be played at {}",
            describe(here)
        ));
        return done();
    }
    if let Some(sprite) = spawn(ctx, seat, Token::Sprite, here, SPRITE_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {sprite}}} to {}",
            describe(here)
        ));
    }
    done()
}

pub static CARD: Card = unit(
    "Trevor Snoozebottom",
    &[Keyword::Shield(1)],
    &[on_hold_me(&[], snooze)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::Location;
    use crate::cards::{script_of, Static, Trigger, Who, TOKEN_SPRITE};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::{ChainItem, ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const TREVOR: u32 = 90;

    static NO_PILLOW: Card = crate::cards::prelude::with_statics(
        crate::cards::prelude::battlefield("No Pillow", &[], &[]),
        &[Static::NoUnitsPlayedHere],
    );

    fn trevor(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(TREVOR, zone, seat, "Trevor Snoozebottom", 3);
        card.domain = vec!["Calm".into()];
        card.energy = Some(3);
        card
    }

    fn bedroom(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(trevor(zone, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(TREVOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn sprites_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SPRITE && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_shield_unit_with_one_targetless_hold_trigger() {
        assert!(std::ptr::eq(
            script_of("Trevor Snoozebottom").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Shield(1)]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(hold.targets.is_empty(), "the Sprite is played here");
        assert!(!hold.optional);
        assert!(hold.cost.is_none() && hold.condition.is_none());
    }

    #[test]
    fn holding_with_him_plays_a_ready_temporary_sprite_at_his_battlefield() {
        let mut fixture = bedroom(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![TREVOR]
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == TREVOR
        ));
        assert!(ctx.blob.prompt.is_none(), "he asks nothing");
        assert!(
            sprites_of(&ctx, 0).is_empty(),
            "the Sprite waits for the chain"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let sprites = sprites_of(&ctx, 0);
        assert_eq!(sprites, [next]);
        let sprite = sprites[0];
        assert_eq!(
            ctx.location(sprite),
            Some(Location::Battlefield(fixtures::BF1)),
            "here · where he holds"
        );
        assert!(ctx.is_token(sprite));
        assert_eq!(ctx.current_might(sprite), 3);
        assert!(ctx.is_temporary(sprite));
        assert!(!ctx.card(sprite).unwrap().exhausted, "played ready");
        assert_eq!(ctx.controller(sprite), 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == sprite
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {sprite}}} to {{zone 9}}")));
        assert_eq!(
            sprites_of(&ctx, 1),
            [fixtures::SPRITE],
            "the enemy keeps only the Sprite it had"
        );
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_hold_elsewhere_or_by_the_opponent_is_not_his() {
        let mut fixture = bedroom(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vi holds, he sits in base");
        assert!(sprites_of(&ctx, 0).is_empty());
        drop(ctx);
        let mut fixture = bedroom(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the enemy Sprite's hold is not his"
        );
    }

    #[test]
    fn a_battlefield_where_units_cannot_be_played_refuses_the_sprite() {
        let mut fixture = bedroom(fixtures::BF1);
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &NO_PILLOW);
        let mut ctx = fixture.ctx();
        assert!(!ctx.units_played_here(fixtures::BF1));
        let item = ChainItem::new(
            7,
            ItemKind::Trigger {
                source: TREVOR,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(snooze(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(sprites_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("no Sprite · units can't be played at")));
        ctx.kill(TREVOR, crate::engine::ctx::Cause::Rule);
        assert_eq!(snooze(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TREVOR}}} has left the board · no Sprite")));
    }
}
