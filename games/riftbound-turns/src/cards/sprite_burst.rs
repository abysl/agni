use super::prelude::{a_play_location, done, play, spawn, spell, zone_target, Location, Token};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

const SPRITE_ARRIVES_READY: bool = true;
pub const SPRITES: usize = 2;

fn burst(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    for index in 0..SPRITES {
        let at = zone_target(item, index)
            .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            .unwrap_or(Location::Base(seat));
        if let Some(sprite) = spawn(ctx, seat, Token::Sprite, at, SPRITE_ARRIVES_READY) {
            ctx.narrate(format!(
                "{{seat {seat}}} plays {{card {sprite}}} to {}",
                describe(at)
            ));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Sprite Burst",
    &[],
    &[play(
        &[
            a_play_location("where the first Sprite is played"),
            a_play_location("where the second Sprite is played"),
        ],
        burst,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::TOKEN_SPRITE;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::PromptWhy;

    const BURST: u32 = 90;

    #[test]
    fn two_ready_sprites_enter_at_the_chosen_held_locations() {
        let mut fixture = Fixture::enforced();
        let mut burst = fixtures::spell(BURST, fixtures::HAND, 0, "Sprite Burst", 2, 0);
        burst.domain = vec!["Fury".into()];
        fixture.table.cards.push(burst);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BURST).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{zone 8}", "{zone 9}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        let sprites: Vec<&agni_plugin_sdk::table::CardInfo> = ctx
            .table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SPRITE && card.owner == 0)
            .collect();
        assert_eq!(sprites.len(), SPRITES);
        assert!(sprites.iter().all(|sprite| !sprite.exhausted));
        assert!(sprites.iter().all(|sprite| ctx.is_temporary(sprite.id)));
        assert_eq!(sprites[0].zone, Some(fixtures::BF1));
        assert_eq!(sprites[1].zone, Some(fixtures::BASE));
    }
}
