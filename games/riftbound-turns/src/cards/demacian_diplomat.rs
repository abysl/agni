use super::prelude::{done, gain_xp, play, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const XP: u8 = 1;

fn negotiate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, XP);
    done()
}

pub static CARD: Card = unit("Demacian Diplomat", &[], &[play(&[], negotiate)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::rules::COUNTER_XP;
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const DIPLOMAT: u32 = 90;
    const THEIR_DIPLOMAT: u32 = 91;

    fn diplomat(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, fixtures::HAND, seat, "Demacian Diplomat", 2);
        card.domain = vec!["Body".into()];
        card
    }

    fn embassy() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(diplomat(DIPLOMAT, 0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DIPLOMAT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_play_trigger() {
        assert!(std::ptr::eq(script_of("Demacian Diplomat").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert_eq!(ability.xp, 0, "the XP is gained, not spent");
        assert_eq!(XP, 1);
    }

    #[test]
    fn playing_the_diplomat_gains_one_xp_when_the_trigger_resolves() {
        let mut fixture = embassy();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DIPLOMAT).unwrap();
        assert!(ctx.on_board(DIPLOMAT));
        assert!(ctx.blob.prompt.is_none(), "the trigger asks nothing");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DIPLOMAT
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(ctx.xp(0), 0, "the XP waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), i32::from(XP));
        assert_eq!(ctx.xp(1), 0, "only its controller gains it");
        assert!(ctx
            .effects
            .contains(&Effect::score(0, COUNTER_XP, i32::from(XP))));
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_diplomat_killed_in_response_still_gains_the_xp() {
        let mut fixture = embassy();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DIPLOMAT).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.kill(DIPLOMAT, crate::engine::ctx::Cause::Rule);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(DIPLOMAT));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), i32::from(XP), "the trigger is not the unit");
    }

    #[test]
    fn the_other_seat_cannot_play_theirs_on_this_turn_and_an_empty_pool_refuses_the_play() {
        let mut fixture = embassy();
        fixture.table.cards.push(diplomat(THEIR_DIPLOMAT, 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DIPLOMAT)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut broke = embassy();
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, DIPLOMAT),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 0
            })
        );
        assert!(ctx.blob.chain.is_empty(), "nothing triggered");
        assert_eq!(ctx.xp(0), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Played { card, .. } if *card == DIPLOMAT)));
    }
}
