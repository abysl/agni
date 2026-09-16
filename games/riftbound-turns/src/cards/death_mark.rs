use super::prelude::{
    a_play_location, burn, done, play, spawn, spell, zone_target, Location, Token,
};
use super::{Card, Cost, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

pub const BURN: usize = 3;
pub const FLOW: Cost = Cost {
    energy: 1,
    power: &[Power::Rainbow, Power::Rainbow],
};
const CLONE_ARRIVES_READY: bool = false;

fn mark(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    burn(ctx, seat, BURN);
    let at = zone_target(item, 0)
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .unwrap_or(Location::Base(seat));
    if let Some(clone) = spawn(ctx, seat, Token::ShadowClone, at, CLONE_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {clone}}} to {}",
            describe(at)
        ));
    }
    done()
}

pub static CARD: Card = spell(
    "Death Mark",
    &[Keyword::Flow(FLOW)],
    &[play(
        &[a_play_location("where the Shadow Clone is played")],
        mark,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, TOKEN_SHADOW_CLONE};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::{Leave, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const MARK: u32 = 90;
    const THEIR_MARK: u32 = 91;

    fn mark_card(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Death Mark", 2, 1);
        card.domain = vec!["Fury".into(), "Chaos".into()];
        card
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mark_card(MARK, zone, 0));
        fixture.table.cards.push(mark_card(THEIR_MARK, zone, 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn clones<'c>(ctx: &'c Ctx<'c>, seat: u8) -> Vec<&'c CardInfo> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SHADOW_CLONE && card.owner == seat)
            .collect()
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn sorted(mut cards: Vec<u32>) -> Vec<u32> {
        cards.sort_unstable();
        cards
    }

    fn burned(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Burned { seat: who, card } if *who == seat => Some(*card),
                _ => None,
            })
            .collect()
    }

    fn drag(ctx: &Ctx, seat: u8, card: u32, from: u16) -> EntryMove {
        EntryMove {
            card,
            from: Some(from),
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_flow_spell_that_asks_where_the_clone_is_played() {
        assert!(std::ptr::eq(script_of("Death Mark").unwrap(), &CARD));
        assert_eq!(CARD.name, "Death Mark");
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        assert_eq!(
            (FLOW.energy, FLOW.power),
            (1, &[Power::Rainbow, Power::Rainbow][..])
        );
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(
            ability.targets[0],
            a_play_location("where the Shadow Clone is played")
        );
        assert!(ability.candidates.is_none());
        assert_eq!(BURN, 3);
        let face = Token::ShadowClone.face();
        assert_eq!(face.name, TOKEN_SHADOW_CLONE);
        assert_eq!(face.might, Some(0));
    }

    #[test]
    fn three_are_burned_then_an_exhausted_zero_might_clone_enters_at_the_chosen_location() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23]);
        fixtures::play_from_hand(&mut ctx, 0, MARK).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{zone 8}", "{zone 9}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(clones(&ctx, 0).is_empty(), "nothing until it resolves");
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23]);
        let next = ctx.table.next_id;
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(burned(&ctx, 0), [23, 22, 21], "the top three, top first");
        assert_eq!(deck_of(&ctx, 0), [20]);
        assert_eq!(sorted(ctx.trash_of(0)), [21, 22, 23, MARK]);
        assert!(ctx.blob.log.contains(&"{seat 0} burns 3".to_string()));
        let spawned = clones(&ctx, 0);
        assert_eq!(spawned.len(), 1);
        let clone = spawned[0];
        assert_eq!(clone.id, next);
        assert_eq!(clone.zone, Some(fixtures::BF1));
        assert!(clone.exhausted, "it enters exhausted");
        assert_eq!(clone.might, Some(0));
        assert!(ctx.is_token(clone.id));
        assert!(ctx.is_unit(clone.id));
        assert_eq!(ctx.controller(clone.id), 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == next
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {next}}} to {{zone 9}}")));
        assert!(clones(&ctx, 1).is_empty());
        assert_eq!(deck_of(&ctx, 1), [24, 25]);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_thin_deck_burns_what_it_has_and_the_clone_still_comes() {
        let mut fixture = armed(fixtures::HAND);
        fixture
            .table
            .cards
            .retain(|card| ![20, 21, 22].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MARK).unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(burned(&ctx, 0), [23]);
        assert!(deck_of(&ctx, 0).is_empty());
        let spawned = clones(&ctx, 0);
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].zone, Some(fixtures::BASE));
        drop(ctx);
        let mut empty = armed(fixtures::HAND);
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.owner != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MARK).unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(burned(&ctx, 0).is_empty());
        assert!(!ctx.blob.log.iter().any(|line| line.contains("burns")));
        assert_eq!(clones(&ctx, 0).len(), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_the_trash_the_flow_play_offers_the_base_only_and_the_spell_is_banished() {
        let mut fixture = armed(fixtures::TRASH);
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, 0, MARK, fixtures::TRASH)),
            Ok(Intent::Play {
                card: MARK,
                origin: Origin::Trash {
                    leave: Leave::Banish
                },
                location: None,
                on_chain: true,
            })
        );
        let ready = ctx.ready_runes_of(0).len();
        play_engine::begin(
            &mut ctx,
            0,
            MARK,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "cancel"],
            "the base is the only location"
        );
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - usize::from(FLOW.energy),
            "one energy: one rune, whose pool carries the rainbows"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(burned(&ctx, 0), [23, 22, 21]);
        let spawned = clones(&ctx, 0);
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].zone, Some(fixtures::BASE));
        assert_eq!(ctx.banished_of(0), [MARK]);
        assert_eq!(
            sorted(ctx.trash_of(0)),
            [21, 22, 23],
            "the burned three, no Mark"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_is_refused_and_a_cancelled_mark_burns_nothing() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, 1, THEIR_MARK, fixtures::HAND)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, MARK).unwrap();
        assert!(fixtures::choose(&mut ctx, 1, "{zone 8}").is_err());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(MARK).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(burned(&ctx, 0).is_empty());
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23]);
        assert!(clones(&ctx, 0).is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
