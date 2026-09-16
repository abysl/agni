use super::prelude::{deal, done, on_attack, unit, when};
use super::{Card, Event, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;
pub const RUNE_LIMIT: usize = 4;

pub fn four_or_fewer_runes(ctx: &Ctx, seat: u8) -> bool {
    ctx.runes_of(seat).len() <= RUNE_LIMIT
}

fn rage_fueled(ctx: &Ctx, _: &Event, source: Source) -> bool {
    four_or_fewer_runes(ctx, ctx.controller(source.card))
}

pub fn enemy_units_here(ctx: &Ctx, me: u32) -> Vec<u32> {
    let Some(here) = ctx.location(me) else {
        return Vec::new();
    };
    let seat = ctx.controller(me);
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat)
        .collect()
}

fn rampage(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    for unit in enemy_units_here(ctx, me) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {me}}} deals {DAMAGE} to {{card {unit}}}"));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Renekton, Rage Fueled",
    &[Keyword::Accelerate],
    &[when(on_attack(&[], rampage), rage_fueled)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const RENEKTON: u32 = 90;
    const BRUTE: u32 = 91;
    const ALLY: u32 = 92;
    const FAR_ENEMY: u32 = 93;
    const FIFTH_RUNE: u32 = 46;

    fn renekton() -> CardInfo {
        CardInfo {
            energy: Some(6),
            domain: vec!["Fury".into()],
            ..fixtures::unit(RENEKTON, fixtures::BF1, 0, "Renekton, Rage Fueled", 6)
        }
    }

    fn pit() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(renekton());
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(FAR_ENEMY, fixtures::BF2, 1, "Far Enemy", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RENEKTON).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        assert!(ctx.mark_attacker(RENEKTON));
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_prints_accelerate_with_one_conditional_targetless_attack_trigger() {
        assert!(std::ptr::eq(
            script_of("Renekton, Rage Fueled").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!((DAMAGE, RUNE_LIMIT), (2, 4));
        let mut fixture = pit();
        let ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert!(four_or_fewer_runes(&ctx, 0));
        assert!(four_or_fewer_runes(&ctx, 1), "two runes");
        let mut here = enemy_units_here(&ctx, RENEKTON);
        here.sort_unstable();
        assert_eq!(here, [fixtures::THEIR_UNIT, BRUTE]);
    }

    #[test]
    fn with_four_runes_the_attack_deals_two_to_every_enemy_unit_here_and_only_here() {
        let mut fixture = pit();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == RENEKTON
        ));
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.damage_on(BRUTE), 0, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(BRUTE), 2);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: 2,
            source: Cause::Item(1)
        }));
        assert!(
            !ctx.on_board(fixtures::THEIR_UNIT),
            "2 damage on 2 Might is lethal at the cleanup"
        );
        assert_eq!(ctx.damage_on(ALLY), 0, "a friendly unit is spared");
        assert_eq!(ctx.damage_on(FAR_ENEMY), 0, "an enemy elsewhere is spared");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {RENEKTON}}} deals 2 to {{card {BRUTE}}}")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_fifth_rune_stops_the_trigger_when_he_attacks_but_not_once_it_is_on_the_chain() {
        let mut fixture = pit();
        fixture
            .table
            .cards
            .push(fixtures::rune(FIFTH_RUNE, 0, "Fury", true));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 5);
        attacks(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "the intervening if fails as he attacks"
        );
        assert_eq!(ctx.damage_on(BRUTE), 0);
        drop(ctx);

        let mut fixture = pit();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        ctx.table
            .cards
            .push(fixtures::rune(FIFTH_RUNE, 0, "Fury", true));
        assert_eq!(ctx.runes_of(0).len(), 5);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.damage_on(BRUTE),
            i32::from(DAMAGE),
            "383.2.a.1 · the if is part of the Condition, not the Effect"
        );
    }
}
