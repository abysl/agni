use crate::engine::attach;
use crate::engine::ctx::Ctx;

pub fn stolen(ctx: &Ctx) -> Vec<(u32, u32)> {
    ctx.blob
        .cards
        .iter()
        .filter_map(|row| Some((row.id, row.control_source?)))
        .collect()
}

pub fn revert(ctx: &mut Ctx, card: u32) -> bool {
    let Some(row) = ctx.blob.card_state(card) else {
        return false;
    };
    if row.controlled_by.is_none() && row.control_source.is_none() {
        return false;
    }
    if attach::is_attached(ctx, card) {
        attach::detach(ctx, card);
    }
    {
        let row = ctx.state_mut(card);
        row.controlled_by = None;
        row.control_source = None;
    }
    let owner = ctx.owner(card);
    if ctx.on_board(card) {
        ctx.recall(card, false);
    }
    ctx.narrate(format!(
        "{{card {card}}} returns to {{seat {owner}}}'s control"
    ));
    true
}

pub fn sync(ctx: &mut Ctx) {
    for (card, source) in stolen(ctx) {
        if ctx.card(card).is_none() {
            ctx.blob.drop_card_state(card);
        } else if !ctx.is_pending_play(card) && !ctx.on_board(source) {
            revert(ctx, card);
        }
    }
    ctx.blob.cards.retain(|row| !row.is_default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{gear, unit};
    use crate::cards::Card;
    use crate::engine::ctx::{Cause, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{attach, cleanup, phases, settle};
    use agni_plugin_sdk::decide::{Effect, TOP};

    const THIEF: u32 = 90;
    const WEARER: u32 = 91;
    const LOOT: u32 = 92;

    static THIEF_CARD: Card = unit("Thief", &[], &[]);

    static LOOT_CARD: Card = gear("Loot", &[], &[]);

    fn heist() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(THIEF, fixtures::BASE, 0, "Thief", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(WEARER, fixtures::BF2, 1, "Wearer", 3));
        fixture
            .table
            .cards
            .push(fixtures::gear(LOOT, fixtures::BASE, 1, "Loot", 1));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(THIEF, &THIEF_CARD)
            .with_script(LOOT, &LOOT_CARD);
        fixture
    }

    fn steal(ctx: &mut Ctx) {
        assert!(ctx.set_controller(LOOT, 0, THIEF));
        assert_eq!(ctx.controller(LOOT), 0);
        assert_eq!(ctx.owner(LOOT), 1);
    }

    #[test]
    fn a_stolen_gear_sits_in_the_thiefs_base_and_survives_a_cleanup_with_nothing_else_on_its_row() {
        let mut fixture = heist();
        let mut ctx = fixture.ctx();
        steal(&mut ctx);
        assert_eq!(ctx.location(LOOT), Some(Location::Base(0)));
        assert_eq!(stolen(&ctx), [(LOOT, THIEF)]);
        cleanup::run(&mut ctx, None);
        assert_eq!(
            stolen(&ctx),
            [(LOOT, THIEF)],
            "the control row is not pruned while its source stands"
        );
        assert_eq!(ctx.controller(LOOT), 0);
        assert_eq!(ctx.location(LOOT), Some(Location::Base(0)));
        assert!(
            !ctx.blob.log.iter().any(|line| line.contains("returns to")),
            "{:?}",
            ctx.blob.log
        );
    }

    #[test]
    fn the_thief_dying_returns_the_gear_to_its_owners_base_as_it_stands() {
        let mut fixture = heist();
        let mut ctx = fixture.ctx();
        steal(&mut ctx);
        ctx.exhaust(LOOT);
        ctx.kill(THIEF, Cause::Rule);
        cleanup::run(&mut ctx, None);
        assert!(stolen(&ctx).is_empty());
        assert_eq!(ctx.controller(LOOT), 1);
        assert_eq!(ctx.location(LOOT), Some(Location::Base(1)));
        assert!(ctx.effects.contains(&Effect::Move {
            card: LOOT,
            zone: fixtures::BASE,
            seat: 1,
            index: TOP
        }));
        assert!(
            ctx.card(LOOT).unwrap().exhausted,
            "recalled as it stands, not readied and not exhausted"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {LOOT}}} returns to {{seat 1}}'s control")));
        assert!(
            ctx.state_of(LOOT).is_some_and(|row| row.flags != 0) || ctx.state_of(LOOT).is_none(),
            "no control fields linger"
        );
    }

    #[test]
    fn the_thief_bounced_returns_an_attached_gear_detached_and_ready() {
        let mut fixture = heist();
        let mut ctx = fixture.ctx();
        steal(&mut ctx);
        assert_eq!(ctx.attach(LOOT, THIEF), attach::Attached::Yes);
        assert!(attach::is_attached(&ctx, LOOT));
        ctx.bounce(THIEF);
        cleanup::run(&mut ctx, None);
        assert!(!attach::is_attached(&ctx, LOOT));
        assert_eq!(ctx.controller(LOOT), 1);
        assert_eq!(ctx.location(LOOT), Some(Location::Base(1)));
        assert!(!ctx.card(LOOT).unwrap().exhausted);
        assert!(stolen(&ctx).is_empty());
    }

    #[test]
    fn a_stolen_gear_readies_on_the_thiefs_awaken_and_not_on_its_owners() {
        let mut fixture = heist();
        let mut ctx = fixture.ctx();
        steal(&mut ctx);
        settle(&mut ctx).unwrap();
        ctx.exhaust(LOOT);
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.turn_player(), 1);
        assert!(
            ctx.card(LOOT).unwrap().exhausted,
            "the owner's Awaken does not ready a card they no longer control"
        );
        fixtures::pass_until_open(&mut ctx);
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.turn_player(), 0);
        assert!(
            !ctx.card(LOOT).unwrap().exhausted,
            "the thief's Awaken readies it"
        );
        assert_eq!(
            stolen(&ctx),
            [(LOOT, THIEF)],
            "two expirations later the control row still stands"
        );
    }

    #[test]
    fn a_stolen_card_that_left_the_table_drops_its_row_silently() {
        let mut fixture = heist();
        let mut ctx = fixture.ctx();
        steal(&mut ctx);
        ctx.table.cards.retain(|card| card.id != LOOT);
        sync(&mut ctx);
        assert!(stolen(&ctx).is_empty());
        assert!(ctx.state_of(LOOT).is_none());
        assert!(!ctx.blob.log.iter().any(|line| line.contains("returns to")));
    }

    #[test]
    fn a_revert_with_no_control_row_is_a_no_op() {
        let mut fixture = heist();
        let mut ctx = fixture.ctx();
        assert!(!revert(&mut ctx, LOOT));
        assert!(!revert(&mut ctx, 9999));
        sync(&mut ctx);
        assert!(ctx.effects.is_empty());
        assert!(ctx.blob.log.is_empty());
    }
}
