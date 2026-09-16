use super::prelude::{buff, done, location_of, play, unit, Location};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn guard(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    }
    let Some(here @ Location::Battlefield(_)) = location_of(ctx, me) else {
        return done();
    };
    let seat = item.controller;
    for unit in ctx.units_at(here) {
        if unit == me || ctx.controller(unit) != seat {
            continue;
        }
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        }
    }
    done()
}

pub static CARD: Card = unit("Peak Guardian", &[], &[play(&[], guard)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{MoveCause, Moved, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as plays;
    use crate::engine::{chain, settle};
    use crate::state::{ChainItem, ItemKind, ItemStatus, Origin};
    use agni_plugin_sdk::table::Target;

    const GUARDIAN: u32 = 90;
    const ALLY: u32 = 91;
    const RAIDER: u32 = 92;
    const SPARE_RUNES: [u32; 3] = [46, 47, 48];

    fn peak() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut guardian = fixtures::unit(GUARDIAN, fixtures::HAND, 0, "Peak Guardian", 5);
        guardian.domain = vec!["Order".into()];
        guardian.energy = Some(6);
        guardian.power = Some(1);
        fixture.table.cards.push(guardian);
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn resolve_trigger_of_a_guardian_standing_at(zone: u16) -> Fixture {
        let mut fixture = peak();
        fixture.table.card_mut(GUARDIAN).unwrap().zone = Some(zone);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, zone, 1, "Raider", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: GUARDIAN,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.status = ItemStatus::Finalized;
        ctx.blob.chain.push(item);
        chain::resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture
    }

    fn buffs(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    fn play_to(ctx: &mut Ctx, at: Location) {
        plays::begin(ctx, 0, GUARDIAN, Origin::Hand, Some(at)).unwrap();
        settle(ctx).unwrap();
        assert!(ctx.on_board(GUARDIAN));
        assert_eq!(ctx.location(GUARDIAN), Some(at));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == GUARDIAN
        ));
        assert!(ctx.blob.prompt.is_none(), "the guardian asks nothing");
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_targetless_play_trigger() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Peak Guardian").unwrap(),
            &CARD
        ));
        let fixture = peak();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GUARDIAN).unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
    }

    #[test]
    fn played_to_a_held_battlefield_it_buffs_itself_and_every_other_friendly_unit_there() {
        let mut fixture = peak();
        let action = fixtures::move_action(GUARDIAN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert_eq!(buffs(&ctx, GUARDIAN), 0, "the buffs wait for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(GUARDIAN));
        assert!(ctx.is_buffed(fixtures::VI), "the friendly unit here");
        assert!(!ctx.is_buffed(ALLY), "not a friendly unit elsewhere");
        assert_eq!(ctx.current_might(GUARDIAN), 6);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {GUARDIAN}}} is buffed")));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is buffed", fixtures::VI)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_standing_here_is_never_buffed_and_the_base_counts_nobody_else() {
        let mut fixture = resolve_trigger_of_a_guardian_standing_at(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(ctx.is_buffed(GUARDIAN));
        assert!(ctx.is_buffed(fixtures::VI));
        assert!(!ctx.is_buffed(RAIDER), "never an enemy");
        assert!(!ctx.is_buffed(ALLY));
        drop(ctx);

        let mut fixture = resolve_trigger_of_a_guardian_standing_at(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(ctx.is_buffed(GUARDIAN));
        assert!(!ctx.is_buffed(ALLY), "a base is not a battlefield");
        assert!(!ctx.is_buffed(RAIDER));
        assert!(!ctx.is_buffed(fixtures::VI));
    }

    #[test]
    fn played_to_base_it_buffs_only_itself() {
        let mut fixture = peak();
        let action = fixtures::move_action(GUARDIAN, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to(&mut ctx, Location::Base(0));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(GUARDIAN));
        assert!(!ctx.is_buffed(ALLY), "a base is not a battlefield");
        assert!(!ctx.is_buffed(fixtures::VI));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| line.contains("is buffed"))
                .count(),
            1
        );
    }

    #[test]
    fn the_then_clause_reads_where_it_stands_when_the_trigger_resolves() {
        let mut fixture = peak();
        let action = fixtures::move_action(GUARDIAN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert_eq!(
            ctx.move_unit(GUARDIAN, Location::Base(0), MoveCause::Effect),
            Moved::Moved,
            "moved off the battlefield in response"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(GUARDIAN));
        assert!(
            !ctx.is_buffed(fixtures::VI),
            "383.2.b · the then is part of the effect"
        );
        assert!(!ctx.is_buffed(ALLY));
        drop(ctx);

        let mut fixture = peak();
        let action = fixtures::move_action(GUARDIAN, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to(&mut ctx, Location::Base(0));
        assert_eq!(
            ctx.move_unit(
                GUARDIAN,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved,
            "moved onto the battlefield in response"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(GUARDIAN));
        assert!(ctx.is_buffed(fixtures::VI));
        assert!(!ctx.is_buffed(ALLY));
    }

    #[test]
    fn an_already_buffed_ally_keeps_its_one_buff_and_a_guardian_that_left_buffs_nobody_here() {
        let mut fixture = peak();
        let action = fixtures::move_action(GUARDIAN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(ctx.buff(fixtures::VI));
        play_to(&mut ctx, Location::Battlefield(fixtures::BF1));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(buffs(&ctx, fixtures::VI), 1, "702.3 · one buff at a time");
        assert!(ctx.is_buffed(GUARDIAN));
        assert!(!ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is buffed", fixtures::VI)));
        drop(ctx);

        let mut fixture = peak();
        let action = fixtures::move_action(GUARDIAN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert!(ctx.bounce(GUARDIAN));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.is_buffed(GUARDIAN),
            "705 · nothing in hand holds a buff"
        );
        assert!(!ctx.is_buffed(fixtures::VI), "no location, no here");
        assert!(ctx.fault.is_none());
    }
}
