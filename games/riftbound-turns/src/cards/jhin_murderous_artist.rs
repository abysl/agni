use super::prelude::{done, on_move, unit};
use super::{Card, Cost, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;

pub const DEFLECT: u8 = 1;
pub const ADDS: Cost = Cost {
    energy: 1,
    power: &[Power::Rainbow],
};

fn curtain_call(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    ctx.add_to_pool(item.controller, item.kind.source(), &ADDS);
    done()
}

pub static CARD: Card = unit(
    "Jhin - Murderous Artist",
    &[Keyword::Deflect(DEFLECT), Keyword::Ganking],
    &[on_move(&[], curtain_call)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Where, Who};
    use crate::engine::cost;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{act, legal, phases, priority, settle};
    use crate::state::{ItemKind, Pool, Pooled};
    use agni_plugin_sdk::table::CardInfo;

    const JHIN: u32 = 90;

    fn jhin(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(JHIN, zone, seat, "Jhin - Murderous Artist", 4)
        }
    }

    fn stage(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jhin(zone, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn march(fixture: &mut Fixture, to: u16) -> Ctx<'_> {
        let action = fixtures::move_action(JHIN, to, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        ctx
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_deflect_one_and_ganking_and_one_move_trigger_of_his_own() {
        assert!(std::ptr::eq(
            script_of("Jhin - Murderous Artist").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Jhin - Murderous Artist");
        assert_eq!(CARD.keywords, [Keyword::Deflect(1), Keyword::Ganking]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let moved = &CARD.abilities[0];
        assert_eq!(
            moved.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Any
            }
        );
        assert!(!moved.optional);
        assert!(moved.targets.is_empty());
        assert!(moved.cost.is_none());
        assert!(moved.condition.is_none());
        assert_eq!(ADDS.energy, 1);
        assert_eq!(ADDS.power, [Power::Rainbow]);
        assert_eq!(Pool::of(&ADDS, &[]).label(), "1 energy and 1 rainbow");
    }

    #[test]
    fn a_march_to_a_battlefield_banks_one_energy_and_one_rainbow_when_the_trigger_resolves() {
        let mut fixture = stage(fixtures::BASE);
        let mut ctx = march(&mut fixture, fixtures::BF1);
        assert_eq!(
            ctx.location(JHIN),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == JHIN
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.prompt.is_none(), "he asks nothing");
        assert!(
            ctx.blob.seat(0).pool.is_empty(),
            "the add waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.blob.seat(0).pool,
            Pool {
                energy: 1,
                power: vec![Pooled::Rainbow]
            }
        );
        assert!(ctx.blob.seat(1).pool.is_empty());
        assert!(ctx.blob.seat(0).promises.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} adds 1 energy and 1 rainbow to their rune pool".to_string()));
        let spark = cost::total(&ctx, fixtures::HAND_SPELL, false);
        assert_eq!(
            (spark.energy, spark.power.len()),
            (1, 0),
            "Spark costs 2 energy and 1 Fury: the add pays one of each"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn ganking_walks_battlefield_to_battlefield_and_each_move_adds_again() {
        let mut fixture = stage(fixtures::BF1);
        fixture.blob.seat_mut(0).pool = Pool {
            energy: 1,
            power: vec![Pooled::Rainbow],
        };
        let mut ctx = march(&mut fixture, fixtures::BF2);
        assert_eq!(
            ctx.location(JHIN),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.seat(0).pool,
            Pool {
                energy: 2,
                power: vec![Pooled::Rainbow, Pooled::Rainbow]
            }
        );
    }

    #[test]
    fn a_recall_is_not_a_move_and_an_enemy_march_is_not_his() {
        let mut fixture = stage(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.recall(JHIN, true);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(JHIN), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "434.1 · a recall is not a move");
        assert!(ctx.blob.seat(0).pool.is_empty());
        drop(ctx);
        let mut fixture = stage(fixtures::BASE);
        let mut ctx = fixture.ctx();
        ctx.actor = 1;
        ctx.move_unit(
            fixtures::THEIR_UNIT,
            Location::Battlefield(fixtures::BF2),
            crate::engine::ctx::MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.seat(0).pool.is_empty());
        assert!(ctx.blob.seat(1).pool.is_empty());
    }

    #[test]
    fn the_added_resources_pay_for_two_cards_and_are_gone_at_the_end_of_the_turn() {
        let mut fixture = stage(fixtures::BASE);
        let mut ctx = march(&mut fixture, fixtures::BF1);
        resolve_chain(&mut ctx);
        let spark = cost::total(&ctx, fixtures::HAND_SPELL, false);
        assert_eq!((spark.energy, spark.power.len()), (1, 0));
        let boots = cost::total(&ctx, fixtures::HAND_GEAR, false);
        assert_eq!(
            boots.energy, 1,
            "the pool pays one energy of the gear too when the spell took only the rainbow"
        );
        phases::end_turn(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.seat(0).pool.is_empty(),
            "unspent resources are lost at the end of the turn"
        );
    }
}
