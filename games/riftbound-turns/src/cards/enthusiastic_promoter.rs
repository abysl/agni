use super::prelude::{buff, done, on_hold_me, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

fn promote(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(here) = ctx.location(me) else {
        return done();
    };
    let mut buffed = 0;
    for unit in ctx.units_at(here) {
        if buff(ctx, unit) {
            buffed += 1;
        }
    }
    ctx.narrate(format!(
        "{{card {me}}} buffs {buffed} unit{} here",
        if buffed == 1 { "" } else { "s" }
    ));
    done()
}

pub static CARD: Card = unit(
    "Enthusiastic Promoter",
    &[Keyword::Backline],
    &[on_hold_me(&[], promote)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::{Event, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const PROMOTER: u32 = 90;
    const ALLY: u32 = 91;
    const THEIR_GUEST: u32 = 92;

    fn promoter(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: None,
            domain: vec!["Calm".into()],
            ..fixtures::unit(PROMOTER, zone, seat, "Enthusiastic Promoter", 2)
        }
    }

    fn fair(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(promoter(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_GUEST, fixtures::BF1, 1, "Guest", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn hold(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn buffed(ctx: &Ctx, unit: u32) -> i32 {
        ctx.table
            .counter(Target::Card(unit), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_prints_backline_and_one_targetless_hold_trigger() {
        assert!(std::ptr::eq(
            script_of("Enthusiastic Promoter").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Enthusiastic Promoter");
        assert_eq!(CARD.keywords, [Keyword::Backline]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(hold.targets.is_empty());
        assert!(!hold.optional);
        assert!(hold.cost.is_none() && hold.condition.is_none());
    }

    #[test]
    fn holding_with_him_buffs_every_unit_here_friend_and_foe_alike_when_the_trigger_resolves() {
        let mut fixture = fair(fixtures::BF1);
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, .. } if *zone == fixtures::BF1
        )));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PROMOTER
        ));
        assert!(ctx.blob.prompt.is_none(), "he asks nothing");
        assert!(!ctx.is_buffed(ALLY), "the buffs wait for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for unit in [PROMOTER, ALLY, THEIR_GUEST] {
            assert!(ctx.is_buffed(unit), "{unit} is buffed");
            assert_eq!(buffed(&ctx, unit), 1);
        }
        assert_eq!(ctx.current_might(THEIR_GUEST), 2, "all units, theirs too");
        assert!(!ctx.is_buffed(fixtures::VI), "the base is not here");
        assert!(
            !ctx.is_buffed(fixtures::SPRITE),
            "another battlefield is not here"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PROMOTER}}} buffs 3 units here")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_already_buffed_keeps_the_one_it_has_and_is_not_counted_again() {
        let mut fixture = fair(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(ALLY));
        hold(&mut ctx);
        resolve_chain(&mut ctx);
        assert_eq!(buffed(&ctx, ALLY), 1, "buffs do not stack");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PROMOTER}}} buffs 2 units here")));
    }

    #[test]
    fn a_hold_elsewhere_and_a_conquer_with_him_buff_nothing() {
        let mut fixture = fair(fixtures::BASE);
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "the ally holds; he sits in the base"
        );
        assert!(!ctx.is_buffed(ALLY));
        drop(ctx);
        let mut fixture = fair(fixtures::BF1);
        fixture.table.cards.retain(|card| card.id != THEIR_GUEST);
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "469.2 · conquering is not holding"
        );
        assert!(!ctx.is_buffed(PROMOTER));
    }
}
