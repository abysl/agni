use super::prelude::{done, might_this_turn, play, unit, HIDDEN};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 3;

fn scout(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ctx.on_board(me) {
        might_this_turn(ctx, item, me, MIGHT, None);
    }
    done()
}

pub static CARD: Card = unit("Teemo - Scout", HIDDEN, &[play(&[], scout)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Keyword, Trigger};
    use crate::engine::ctx::{Cause, Location, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, play as play_engine, priority, settle};
    use crate::state::{Expiry, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const TEEMO: u32 = 90;

    fn teemo(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            might: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::card(id, zone, seat, "Teemo - Scout", "Unit")
        }
    }

    fn brush() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(teemo(TEEMO, fixtures::HAND, 0));
        fixture.resolve();
        fixture
    }

    fn deploy(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, TEEMO, Origin::Hand, Some(Location::Base(0)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_hidden_champion_unit_with_a_targetless_play_trigger() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Teemo - Scout").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Teemo - Scout");
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert!(ability.targets.is_empty());
        assert_eq!(MIGHT, 3);
    }

    #[test]
    fn playing_teemo_gives_him_three_might_until_the_end_of_the_turn() {
        let mut fixture = brush();
        let action = fixtures::move_action(TEEMO, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        deploy(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == TEEMO
        ));
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert_eq!(
            ctx.current_might(TEEMO),
            1,
            "the bonus waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(TEEMO), 4);
        assert_eq!(might_counter(&ctx, TEEMO), 3);
        assert_eq!(
            ctx.state_of(TEEMO).unwrap().might[0].until,
            Expiry::EndOfTurn(ctx.turn())
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3, "only Teemo grows");
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(TEEMO), 1);
        assert_eq!(might_counter(&ctx, TEEMO), 0);
    }

    #[test]
    fn a_teemo_killed_in_response_leaves_no_might_mod_behind() {
        let mut fixture = brush();
        let action = fixtures::move_action(TEEMO, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        deploy(&mut ctx).unwrap();
        ctx.kill(TEEMO, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(TEEMO));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.state_of(TEEMO).is_none_or(|row| row.might.is_empty()));
        assert_eq!(might_counter(&ctx, TEEMO), 0);
        assert!(ctx.fault.is_none());
    }
}
