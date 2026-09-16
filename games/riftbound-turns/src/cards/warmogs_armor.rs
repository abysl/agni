use super::prelude::{buff, done, equip, gear, on_conquer_me, while_attached, with_statics};
use super::{Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, GRANTED};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};

pub const MIGHT_BONUS: i16 = 1;
pub const ON_CONQUER: u8 = GRANTED;

fn regenerate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        ctx.narrate(format!("{{card {me}}} is off the board · no buff"));
        return done();
    }
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    } else {
        ctx.narrate(format!("{{card {me}}} already has a buff"));
    }
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [on_conquer_me(&[], regenerate)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear("Warmog's Armor", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{conquered, equipment, queue_granted, GEAR};
    use crate::cards::prelude::{attach_gear, detach_gear};
    use crate::cards::{script_of, Resolved, Static, Trigger, Who};
    use crate::engine::ctx::{Cause, Killed, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{settle, triggers};
    use crate::state::{GameBlob, TargetRef};
    use agni_plugin_sdk::table::Target;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Warmog's Armor", 1, "Body"));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_body_equipment_with_one_might_and_a_wearer_conquer_listener() {
        assert!(std::ptr::eq(script_of("Warmog's Armor").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.abilities.len(), 1);
        let listener = &WEARER_TEXT[0];
        assert_eq!(listener.trigger, Trigger::Conquer(Who::Me));
        assert!(listener.condition.is_none());
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(1), Grant::Ability(_)]
        ));
    }

    #[test]
    fn the_wearers_conquer_buffs_the_wearer_once() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "the +1 bonus and the +1 buff"
        );
        assert!(ctx.blob.log.contains(&"{card 50} is buffed".to_string()));
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.table
                .counter(Target::Card(fixtures::VI), COUNTER_BUFFED),
            Some(1),
            "702.3 · one buff at a time"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} already has a buff".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_detach_before_the_trigger_resolves_still_buffs_the_wearer_and_a_dead_wearer_gets_nothing()
    {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(detach_gear(&mut ctx, GEAR));
        let saved_item = ctx.blob.chain[0].clone();
        let saved_table = ctx.table.clone();
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        fixture.table = saved_table;
        fixture.blob = GameBlob::decode(&saved_blob).unwrap();
        fixture.scripts = Resolved::of(&fixture.table);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.blob.chain[0], saved_item);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.is_buffed(fixtures::VI),
            "383.3 · the wearer's trigger is on the chain, the armor coming loose does not take it back"
        );
        assert!(!ctx.is_buffed(fixtures::THEIR_UNIT));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        assert_eq!(ctx.kill(fixtures::VI, Cause::Combat), Killed::Yes);
        let saved_item = ctx.blob.chain[0].clone();
        let saved_table = ctx.table.clone();
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        fixture.table = saved_table;
        fixture.blob = GameBlob::decode(&saved_blob).unwrap();
        fixture.scripts = Resolved::of(&fixture.table);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.blob.chain[0], saved_item);
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.is_buffed(fixtures::THEIR_UNIT));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} is off the board · no buff".to_string()));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "136.2.c · the effect text is the wearer's own"
        );
    }

    #[test]
    fn a_conquer_by_the_wearer_buffs_it_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(fixtures::VI));
    }
}
