use super::prelude::{done, lock_cards, play, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn thunder(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    for other in 0..ctx.players() {
        if other != seat {
            lock_cards(ctx, other);
        }
    }
    done()
}

pub static CARD: Card = unit("Brynhir Thundersong", &[], &[play(&[], thunder)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Trigger;
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, priority};
    use crate::state::{ItemKind, Phase, PlayLock};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const BRYNHIR: u32 = 90;
    const THEIR_SPELL: u32 = 91;
    const THEIR_UNIT_IN_HAND: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut brynhir = fixtures::unit(BRYNHIR, fixtures::HAND, 0, "Brynhir Thundersong", 5);
        brynhir.energy = Some(0);
        fixture.table.cards.push(brynhir);
        let mut premonition = fixtures::spell(THEIR_SPELL, fixtures::HAND, 1, "Premonition", 1, 0);
        premonition.domain = vec!["Mind".into()];
        fixture.table.cards.push(premonition);
        let mut unit = fixtures::unit(THEIR_UNIT_IN_HAND, fixtures::HAND, 1, "Jinx", 2);
        unit.energy = Some(1);
        unit.domain = vec!["Mind".into()];
        fixture.table.cards.push(unit);
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &crate::engine::ctx::Ctx, seat: u8, card: u32, to: Option<u16>) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to,
            to_seat: seat,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn her_play_trigger_locks_every_opponent_out_of_spells_for_the_turn() {
        assert!(std::ptr::eq(
            script_of("Brynhir Thundersong").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BRYNHIR).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == BRYNHIR
        ));
        assert!(
            ctx.blob.seat(1).play_lock.is_empty(),
            "nothing until it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, 1, THEIR_SPELL, ctx.zones.chain)),
            Ok(legal::Intent::Play {
                card: THEIR_SPELL,
                origin: crate::state::Origin::Hand,
                location: None,
                on_chain: true,
            }),
            "the opponent may still react to the trigger"
        );
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.blob.seat(1).play_lock, PlayLock::CARDS);
        assert!(
            ctx.blob.seat(0).play_lock.is_empty(),
            "her own side is not an opponent"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 1} can't play cards this turn"));
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, 1, THEIR_SPELL, ctx.zones.chain)),
            Err(Refusal::Illegal(Reason::NoSpells)),
            "the same reaction is refused once the trigger has resolved"
        );
        fixtures::pass_until_open(&mut ctx);
        phases::end_turn(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(
            ctx.blob.seat(1).play_lock.is_empty(),
            "the lock is for the turn"
        );
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, 1, THEIR_SPELL, ctx.zones.chain)),
            Ok(legal::Intent::Play {
                card: THEIR_SPELL,
                origin: crate::state::Origin::Hand,
                location: None,
                on_chain: true,
            })
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_countered_brynhir_locks_nobody() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BRYNHIR).unwrap();
        let trigger = ctx.blob.chain.last().unwrap().id;
        assert!(crate::engine::chain::counter(
            &mut ctx,
            trigger,
            crate::engine::ctx::CounterDest::Trash
        ));
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.seat(1).play_lock.is_empty());
        assert!(
            ctx.on_board(BRYNHIR),
            "she stays; only her trigger was countered"
        );
    }

    #[test]
    fn the_lock_also_refuses_the_opponents_reaction_units() {
        let mut fixture = armed();
        fixture.table.card_mut(THEIR_UNIT_IN_HAND).unwrap().name = "Shen - Kinkou".into();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BRYNHIR).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.seat(1).play_lock, PlayLock::CARDS);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, 1, THEIR_UNIT_IN_HAND, ctx.zones.base)),
            Err(Refusal::Illegal(Reason::NoUnits)),
            "a Reaction unit is a card too"
        );
    }
}
