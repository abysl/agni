use super::prelude::{a_play_location, done, play, spawn, spell, zone_target, Location, Token};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

pub const TENTACLES: usize = 2;
pub const FLOW: Cost = Cost {
    energy: 3,
    power: &[],
};
const TENTACLES_ARRIVE_READY: bool = false;

fn surface(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    for index in 0..TENTACLES {
        let at = zone_target(item, index)
            .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            .unwrap_or(Location::Base(seat));
        if let Some(tentacle) = spawn(ctx, seat, Token::Tentacle, at, TENTACLES_ARRIVE_READY) {
            ctx.narrate(format!(
                "{{seat {seat}}} plays {{card {tentacle}}} to {}",
                describe(at)
            ));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Up from the Deep",
    &[Keyword::Flow(FLOW)],
    &[play(
        &[
            a_play_location("where the first Tentacle is played"),
            a_play_location("where the second Tentacle is played"),
        ],
        surface,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TOKEN_TENTACLE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as play_engine;
    use crate::state::{Leave, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const DEEP: u32 = 90;

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut deep = fixtures::spell(DEEP, zone, 0, "Up from the Deep", 3, 0);
        deep.domain = vec!["Chaos".into()];
        fixture.table.cards.push(deep);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn tentacles<'c>(ctx: &'c Ctx<'c>) -> Vec<&'c CardInfo> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_TENTACLE && card.owner == 0)
            .collect()
    }

    #[test]
    fn two_exhausted_tentacles_enter_at_the_chosen_locations() {
        assert!(std::ptr::eq(script_of("Up from the Deep").unwrap(), &CARD));
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DEEP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{zone 8}", "{zone 9}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        let spawned = tentacles(&ctx);
        assert_eq!(spawned.len(), TENTACLES);
        assert!(spawned.iter().all(|tentacle| tentacle.exhausted));
        assert!(spawned.iter().all(|tentacle| tentacle.might == Some(1)));
        assert_eq!(spawned[0].zone, Some(fixtures::BF1));
        assert_eq!(spawned[1].zone, Some(fixtures::BASE));
        assert_eq!(ctx.card(DEEP).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn from_the_trash_only_the_base_is_offered_and_the_spell_is_banished() {
        let mut fixture = armed(fixtures::TRASH);
        fixture.blob.set_holder(fixtures::BF1, None);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            DEEP,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "cancel"],
            "the base is the only location"
        );
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        let spawned = tentacles(&ctx);
        assert_eq!(spawned.len(), TENTACLES);
        assert!(spawned
            .iter()
            .all(|tentacle| tentacle.zone == Some(fixtures::BASE)));
        assert_eq!(ctx.banished_of(0), [DEEP]);
        assert!(ctx.trash_of(0).is_empty());
    }
}
