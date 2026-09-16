use super::prelude::{buff, done, on_you_play_card, unit, when};
use super::{Card, Flow, Item, Source, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};

pub fn another_unit_of_yours(_: &Ctx, event: &Event, source: Source) -> bool {
    matches!(
        event,
        Event::Played { card, kind, .. } if *card != source.card && kind == KIND_UNIT
    )
}

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    }
    done()
}

pub static CARD: Card = unit(
    "Cithria of Cloudfield",
    &[],
    &[when(on_you_play_card(&[], rally), another_unit_of_yours)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::Trigger;
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const CITHRIA: u32 = 90;
    const THEIR_RECRUIT: u32 = 92;

    fn cithria(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(CITHRIA, zone, 0, "Cithria of Cloudfield", 1);
        card.domain = vec!["Body".into()];
        card.energy = Some(2);
        card
    }

    fn cloudfield() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(cithria(fixtures::BASE));
        fixture.table.cards.push(fixtures::unit(
            THEIR_RECRUIT,
            fixtures::HAND,
            1,
            "Recruit",
            1,
        ));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn buffs(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_watching_the_cards_its_controller_plays() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Cithria of Cloudfield").unwrap(),
            &CARD
        ));
        let fixture = cloudfield();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CITHRIA).unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlayCard);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(
            ability.condition.is_some(),
            "another unit, not a spell or gear"
        );
    }

    #[test]
    fn playing_another_unit_buffs_cithria_when_her_trigger_resolves() {
        let mut fixture = cloudfield();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CITHRIA
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(buffs(&ctx, CITHRIA), 0, "the buff waits for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(CITHRIA));
        assert_eq!(ctx.current_might(CITHRIA), 2);
        assert!(
            !ctx.is_buffed(fixtures::HAND_UNIT),
            "the played unit is not the one buffed"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CITHRIA}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn her_own_play_a_spell_a_gear_and_the_opponents_unit_trigger_nothing() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(cithria(fixtures::HAND));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CITHRIA).unwrap();
        assert!(ctx.on_board(CITHRIA));
        assert!(ctx.blob.chain.is_empty(), "she is not another unit");
        assert!(!ctx.is_buffed(CITHRIA));
        drop(ctx);

        let mut fixture = cloudfield();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "a spell is not a unit");
        assert!(!ctx.is_buffed(CITHRIA));
        drop(ctx);

        let mut fixture = cloudfield();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert!(ctx.on_board(fixtures::HAND_GEAR));
        assert!(ctx.blob.chain.is_empty(), "a gear is not a unit");
        assert!(!ctx.is_buffed(CITHRIA));
        drop(ctx);

        let mut fixture = cloudfield();
        fixture.blob.core_mut().unwrap().advance();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        fixtures::play_from_hand(&mut ctx, 1, THEIR_RECRUIT).unwrap();
        assert!(ctx.on_board(THEIR_RECRUIT));
        assert!(
            ctx.blob.chain.is_empty(),
            "the opponent's unit is not one you play"
        );
        assert!(!ctx.is_buffed(CITHRIA));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_already_buffed_cithria_gains_no_second_buff_and_one_in_hand_watches_nothing() {
        let mut fixture = cloudfield();
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(CITHRIA));
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "she still triggers");
        resolve_chain(&mut ctx);
        assert_eq!(buffs(&ctx, CITHRIA), 1, "702.3 · one buff at a time");
        assert_eq!(ctx.current_might(CITHRIA), 2);
        assert!(!ctx
            .blob
            .log
            .contains(&format!("{{card {CITHRIA}}} is buffed")));
        drop(ctx);

        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(cithria(fixtures::HAND));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.blob.chain.is_empty(), "384.1 · only on the board");
        assert!(!ctx.is_buffed(CITHRIA));
    }

    #[test]
    fn a_token_unit_played_for_you_is_another_unit_you_play() {
        let mut fixture = cloudfield();
        let mut ctx = fixture.ctx();
        assert!(spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), false).is_some());
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert!(ctx.is_buffed(CITHRIA));
    }
}
