use super::confront::units_enter_ready_this_turn;
use super::prelude::{done, play, spawn_gold, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const GOLD_ARRIVES_READY: bool = false;

fn bushwhack(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    units_enter_ready_this_turn(ctx, seat);
    spawn_gold(ctx, seat, GOLD_ARRIVES_READY);
    done()
}

pub static CARD: Card = spell("Bushwhack", &[Keyword::Hidden], &[play(&[], bushwhack)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, TOKEN_GOLD};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, settle};
    use crate::state::{Origin, Priority};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BUSHWHACK: u32 = 90;

    fn bushwhack_card(seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Fury".into()],
            ..fixtures::spell(BUSHWHACK, fixtures::HAND, seat, "Bushwhack", 2, 1)
        }
    }

    fn ambush() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bushwhack_card(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BUSHWHACK).unwrap(),
            &CARD
        ));
        fixture
    }

    fn gold_of<'c>(ctx: &'c Ctx, seat: u8) -> Vec<&'c CardInfo> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .collect()
    }

    #[test]
    fn the_script_is_a_hidden_spell_with_no_targets() {
        assert!(std::ptr::eq(script_of("Bushwhack").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
    }

    #[test]
    fn from_hand_it_announces_the_grant_and_plays_an_exhausted_gold() {
        let mut fixture = ambush();
        let mut ctx = fixture.ctx();
        assert!(gold_of(&ctx, 0).is_empty());
        fixtures::play_from_hand(&mut ctx, 0, BUSHWHACK).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(gold_of(&ctx, 0).is_empty(), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let gold = gold_of(&ctx, 0);
        assert_eq!(gold.len(), 1);
        assert!(gold[0].exhausted, "played exhausted");
        assert_eq!(gold[0].zone, Some(fixtures::BASE));
        assert!(gold_of(&ctx, 1).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s units enter ready this turn".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert_eq!(ctx.card(BUSHWHACK).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_facedown_it_reacts_for_free_and_still_makes_the_gold() {
        let mut fixture = ambush();
        fixture.table.card_mut(BUSHWHACK).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(BUSHWHACK).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BUSHWHACK, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            BUSHWHACK,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.runes_of(0)
                .iter()
                .all(|rune| rune.exhausted == (rune.id == fixtures::RUNE_A)),
            "a hidden card reacts for free: {:?}",
            ctx.effects
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(gold_of(&ctx, 0).len(), 1);
        assert!(gold_of(&ctx, 0)[0].exhausted);
        assert_eq!(ctx.card(BUSHWHACK).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_hand_it_is_a_sorcery_refused_while_the_chain_is_closed() {
        let mut fixture = ambush();
        fixture.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: BUSHWHACK,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "Hidden alone is not Reaction from hand"
        );
    }

    #[test]
    fn a_friendly_unit_played_after_bushwhack_enters_ready() {
        let mut fixture = ambush();
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BUSHWHACK).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::HAND_UNIT).unwrap().zone,
            Some(fixtures::BASE)
        );
        assert!(
            !ctx.card(fixtures::HAND_UNIT).unwrap().exhausted,
            "Bushwhack's grant overrides the exhausted entry"
        );
    }
}
