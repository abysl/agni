use super::prelude::{done, draw, play, spell};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;
pub const FLOW: Cost = Cost {
    energy: 2,
    power: &[],
};

fn dredge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = spell("Dredge Up", &[Keyword::Flow(FLOW)], &[play(&[], dredge)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::{Leave, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const DREDGE: u32 = 90;
    const THEIR_DREDGE: u32 = 91;

    fn dredge_card(id: u32, zone: u16, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Dredge Up", 2, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dredge_card(DREDGE, zone, 0));
        fixture.table.cards.push(dredge_card(THEIR_DREDGE, zone, 1));
        fixture.resolve();
        fixture
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

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_flow_spell_with_one_targetless_play_ability() {
        assert!(std::ptr::eq(script_of("Dredge Up").unwrap(), &CARD));
        assert_eq!(CARD.name, "Dredge Up");
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        assert!(CARD.has_keyword(Keyword::Flow(Cost::FREE)));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_none());
        assert_eq!(DRAWS, 1);
        assert_eq!((FLOW.energy, FLOW.power.len()), (2, 0));
    }

    #[test]
    fn from_hand_it_draws_one_and_lands_in_the_trash() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DREDGE).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(drew(&ctx, 0), 0, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert!(ctx.hand_of(0).contains(&23), "the top card of the deck");
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert_eq!(ctx.card(DREDGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.banished_of(0).is_empty());
        assert_eq!(drew(&ctx, 1), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_the_trash_the_flow_play_draws_one_and_is_banished() {
        let mut fixture = armed(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        let entry = drag(&ctx, 0, DREDGE, fixtures::TRASH);
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Ok(Intent::Play {
                card: DREDGE,
                origin: Origin::Trash {
                    leave: Leave::Banish
                },
                location: None,
                on_chain: true,
            })
        );
        let ready = ctx.ready_runes_of(0).len();
        let hand = ctx.hand_of(0).len();
        play_engine::begin(
            &mut ctx,
            0,
            DREDGE,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(DREDGE).unwrap().zone, Some(fixtures::CHAIN));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - usize::from(FLOW.energy),
            "the Flow cost is paid"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.banished_of(0), [DREDGE]);
        assert!(ctx.trash_of(0).is_empty());
        assert!(ctx.blob.log.contains(&"{card 90} is banished".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_empty_deck_draws_nothing_and_the_spell_still_resolves() {
        let mut fixture = armed(fixtures::HAND);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DREDGE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), 0);
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 0".to_string()));
        assert_eq!(ctx.card(DREDGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_is_refused_from_hand_and_from_the_trash_on_my_turn() {
        let mut fixture = armed(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, 1, THEIR_DREDGE, fixtures::HAND)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut trashed = armed(fixtures::TRASH);
        let ctx = trashed.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, 1, THEIR_DREDGE, fixtures::TRASH)),
            Err(Refusal::NotYourTurn),
            "Flow keeps the spell's own Sorcery timing"
        );
        assert!(legal::highlights(&ctx, 1)
            .iter()
            .all(|row| row.card != THEIR_DREDGE));
        drop(ctx);
        let mut renamed = armed(fixtures::TRASH);
        renamed.table.card_mut(DREDGE).unwrap().name = "Spark".into();
        renamed.resolve();
        let ctx = renamed.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, 0, DREDGE, fixtures::TRASH)),
            Err(Refusal::Illegal(Reason::TrashIsFinal)),
            "without Flow the trash is final"
        );
    }
}
