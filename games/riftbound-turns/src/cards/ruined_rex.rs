use super::prelude::{an_enemy_unit, card_target, deal, deathknell, done, unit};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 4;

pub const PREY: TargetSpec = an_enemy_unit("an enemy unit to deal 4 to");

fn death_throes(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    match card_target(ctx, item, 0) {
        Some(unit) => {
            deal(ctx, item, unit, DAMAGE);
            ctx.narrate(format!("{{card {me}}} deals {DAMAGE} to {{card {unit}}}"));
        }
        None => ctx.narrate(format!("{{card {me}}}: the target is gone")),
    }
    done()
}

pub static CARD: Card = unit(
    "Ruined Rex",
    &[Keyword::Deathknell],
    &[deathknell(&[PREY], death_throes)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::ENEMY_UNIT;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const REX: u32 = 90;
    const THEIR_BRUTE: u32 = 91;

    fn rex(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(REX, zone, seat, "Ruined Rex", 6)
        }
    }

    fn wreck() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rex(fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 7));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_deathknell_with_one_death_ability_that_chooses_an_enemy_unit() {
        assert!(std::ptr::eq(script_of("Ruined Rex").unwrap(), &CARD));
        assert_eq!(CARD.name, "Ruined Rex");
        assert_eq!(CARD.keywords, [Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let death = &CARD.abilities[0];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(!death.optional);
        assert!(death.condition.is_none());
        assert_eq!(death.targets, [PREY]);
        assert_eq!((PREY.min, PREY.max), (1, 1));
        assert_eq!(PREY.kind, TargetKind::Card);
        assert_eq!(PREY.filter, ENEMY_UNIT);
        assert_eq!(DAMAGE, 4);
    }

    #[test]
    fn dying_asks_for_an_enemy_unit_and_the_pick_takes_four_when_the_deathknell_resolves() {
        let mut fixture = wreck();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(REX, Cause::Rule), Killed::Yes);
        assert!(ctx.in_trash(REX));
        settle(&mut ctx).unwrap();
        let item = ctx
            .blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the deathknell is pending");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_BRUTE}}}"),
            ],
            "every enemy unit anywhere · none of yours"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == REX
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_BRUTE)]);
        assert_eq!(
            ctx.damage_on(THEIR_BRUTE),
            0,
            "the damage waits for the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(THEIR_BRUTE), i32::from(DAMAGE));
        assert!(ctx.on_board(THEIR_BRUTE), "seven Might survives four");
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {REX}}} deals 4 to {{card {THEIR_BRUTE}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lethal_pick_dies_when_the_deathknell_resolves() {
        let mut fixture = wreck();
        fixture.table.card_mut(THEIR_BRUTE).unwrap().might = Some(4);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(REX, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.in_trash(THEIR_BRUTE));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_target_that_left_the_board_in_response_takes_nothing() {
        let mut fixture = wreck();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(REX, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        assert!(ctx.bounce(THEIR_BRUTE));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {REX}}}: the target is gone")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_an_enemy_unit_the_deathknell_fizzles_before_the_chain() {
        let mut fixture = wreck();
        fixture.table.cards.retain(|card| {
            ![THEIR_BRUTE, fixtures::THEIR_UNIT, fixtures::SPRITE].contains(&card.id)
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(REX, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {REX}}} trigger fizzles · no legal target")));
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.fault.is_none());
    }
}
