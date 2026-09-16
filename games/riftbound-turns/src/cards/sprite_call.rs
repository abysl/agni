use super::prelude::{a_play_location, done, play, spawn, spell, zone_target, Location, Token};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

pub const SPRITE_ARRIVES_READY: bool = true;

fn call(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let at = zone_target(item, 0)
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .unwrap_or(Location::Base(seat));
    if let Some(sprite) = spawn(ctx, seat, Token::Sprite, at, SPRITE_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {sprite}}} to {}",
            describe(at)
        ));
    }
    done()
}

pub static CARD: Card = spell(
    "Sprite Call",
    &[Keyword::Hidden, Keyword::Action],
    &[play(&[a_play_location("where the Sprite is played")], call)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{TargetKind, Trigger, TOKEN_SPRITE};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::engine::{hide, priority};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const SPRITE_CALL: u32 = 90;

    fn sprite_call(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(SPRITE_CALL, fixtures::HAND, seat, "Sprite Call", 3, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sprite_call(0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SPRITE_CALL).unwrap(),
            &CARD
        ));
        fixture
    }

    fn sprites_of<'c>(ctx: &'c Ctx, seat: u8) -> Vec<&'c CardInfo> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SPRITE && card.owner == seat)
            .collect()
    }

    #[test]
    fn the_script_is_a_hidden_action_that_picks_a_play_location() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Sprite Call").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].kind, TargetKind::Zone);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn from_hand_a_ready_temporary_sprite_enters_the_chosen_held_location() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(sprites_of(&ctx, 0).is_empty());
        fixtures::play_from_hand(&mut ctx, 0, SPRITE_CALL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "{zone 9}", "cancel"],
            "the base and the held battlefield, never the other seat's"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Zone(fixtures::BF1)]);
        assert!(sprites_of(&ctx, 0).is_empty(), "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let sprites = sprites_of(&ctx, 0);
        assert_eq!(sprites.len(), 1);
        let sprite = sprites[0];
        assert_eq!(sprite.zone, Some(fixtures::BF1));
        assert!(!sprite.exhausted, "it is played ready");
        assert_eq!(sprite.might, Some(3));
        assert!(ctx.is_temporary(sprite.id));
        assert_eq!(ctx.controller(sprite.id), 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == sprite.id
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {}}} to {{zone 9}}",
            sprite.id
        )));
        assert_eq!(ctx.card(SPRITE_CALL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_hidden_the_sprite_must_be_played_to_that_battlefield() {
        let mut fixture = armed();
        fixture.table.card_mut(SPRITE_CALL).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(SPRITE_CALL).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(hide::playable(&ctx, SPRITE_CALL));
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(SPRITE_CALL, chain, 0), 0)
            .unwrap();
        hide::play_from_facedown(&mut ctx, 0, SPRITE_CALL).unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "cancel"],
            "811.1.d.3: the base is closed, only the hiding battlefield is offered"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Zone(fixtures::BF1)]);
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                agni_plugin_sdk::decide::Effect::Annotate { key, .. } if key == "exhausted"
            )),
            "played from hidden it ignores its cost: {:?}",
            ctx.effects
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let sprites = sprites_of(&ctx, 0);
        assert_eq!(sprites.len(), 1);
        assert_eq!(sprites[0].zone, Some(fixtures::BF1));
        assert!(!sprites[0].exhausted);
    }

    #[test]
    fn an_action_is_refused_off_turn_and_short_of_energy() {
        let mut theirs = Fixture::enforced();
        theirs.table.cards.push(sprite_call(1));
        theirs.resolve();
        let ctx = theirs.ctx();
        let entry = EntryMove {
            card: SPRITE_CALL,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(legal::classify(&ctx, 1, &entry), Err(Refusal::NotYourTurn));
        let mut broke = armed();
        broke.table.card_mut(43).unwrap().exhausted = true;
        let ctx = broke.ctx();
        let entry = EntryMove {
            card: SPRITE_CALL,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 2
            })
        );
    }
}
