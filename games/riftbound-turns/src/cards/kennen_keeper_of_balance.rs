use super::prelude::{
    a_unit, card_target, done, on_attack, optional, play, stun, unit, with_cost, with_statics,
};
use super::{Card, Cost, Flow, Grant, Item, Keyword, Stage, Static, TargetSpec};
use crate::engine::ctx::Ctx;

pub const STUN_COST: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const BONUS: i16 = 2;
pub const TARGET: TargetSpec = a_unit("a unit to stun");

pub fn stunned_enemy_here(ctx: &Ctx, me: u32) -> bool {
    let Some(here) = ctx.location(me) else {
        return false;
    };
    let seat = ctx.controller(me);
    ctx.units_at(here)
        .into_iter()
        .any(|unit| ctx.controller(unit) != seat && ctx.is_stunned(unit))
}

fn balance(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        stun(ctx, unit);
    }
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Kennen, Keeper of Balance",
        &[Keyword::Hidden],
        &[
            optional(with_cost(play(&[TARGET], balance), STUN_COST)),
            optional(with_cost(on_attack(&[TARGET], balance), STUN_COST)),
        ],
    ),
    &[Static::While(stunned_enemy_here, &[Grant::Might(BONUS)])],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{move_unit, Location, Moved, UNIT};
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{prompts, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const KENNEN: u32 = 90;
    const BRUTE: u32 = 91;
    const MIGHT: u8 = 2;
    const EXTRA_RUNES: [u32; 2] = [100, 101];

    fn kennen(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Order".into()],
            ..fixtures::unit(KENNEN, zone, 0, "Kennen, Keeper of Balance", MIGHT)
        }
    }

    fn temple(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(kennen(zone));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(KENNEN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        assert!(ctx.mark_attacker(KENNEN));
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_hidden_with_the_pay_two_stun_on_play_and_on_attack_and_a_while_over_a_stunned_enemy(
    ) {
        assert!(std::ptr::eq(
            script_of("Kennen, Keeper of Balance").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert_eq!(CARD.abilities.len(), 2);
        let played = &CARD.abilities[0];
        assert_eq!(played.trigger, Trigger::Play);
        let attack = &CARD.abilities[1];
        assert_eq!(attack.trigger, Trigger::Attacks(Who::Me));
        for ability in [played, attack] {
            assert!(ability.optional, "you may");
            assert_eq!(ability.cost, Some(STUN_COST));
            assert_eq!(ability.targets, &[TARGET]);
            assert_eq!(ability.targets[0].filter, UNIT);
        }
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(
            CARD.statics[0],
            Static::While(_, [Grant::Might(BONUS)])
        ));
        assert_eq!(BONUS, 2);
    }

    #[test]
    fn attacking_asks_for_a_unit_then_two_energy_and_the_stun_lands_as_it_resolves() {
        let mut fixture = temple(fixtures::BF1);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {KENNEN}}}"),
                format!("{{card {BRUTE}}}"),
            ],
            "any unit anywhere"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("pay 2 energy for the {{card {KENNEN}}} trigger?")
        );
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 2, "two energy");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == KENNEN
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        assert!(!ctx.is_stunned(BRUTE), "nothing until it resolves");
        assert_eq!(ctx.current_might(KENNEN), i32::from(MIGHT));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(BRUTE));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BRUTE}}} is stunned")));
        assert!(stunned_enemy_here(&ctx, KENNEN));
        assert_eq!(
            ctx.current_might(KENNEN),
            i32::from(MIGHT) + 2,
            "+2 while a stunned enemy unit is here"
        );
        ctx.unstun(BRUTE);
        assert_eq!(ctx.current_might(KENNEN), i32::from(MIGHT));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_while_reads_only_a_stunned_enemy_at_his_own_location() {
        let mut fixture = temple(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(!stunned_enemy_here(&ctx, KENNEN));
        assert!(ctx.stun(fixtures::VI));
        assert!(
            !stunned_enemy_here(&ctx, KENNEN),
            "a stunned friendly unit is not an enemy"
        );
        assert!(ctx.stun(fixtures::SPRITE));
        assert!(
            !stunned_enemy_here(&ctx, KENNEN),
            "a stunned enemy elsewhere is not here"
        );
        assert_eq!(ctx.current_might(KENNEN), i32::from(MIGHT));
        assert!(ctx.stun(fixtures::THEIR_UNIT));
        assert!(stunned_enemy_here(&ctx, KENNEN));
        assert_eq!(ctx.current_might(KENNEN), i32::from(MIGHT) + 2);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "the While is his alone");
        assert_eq!(
            move_unit(&mut ctx, &fixtures::effect_of(0), KENNEN, Location::Base(0)),
            Some(Moved::Moved)
        );
        assert!(!stunned_enemy_here(&ctx, KENNEN), "left the battlefield");
        assert_eq!(ctx.current_might(KENNEN), i32::from(MIGHT));
    }

    #[test]
    fn playing_him_asks_the_same_and_declining_the_two_energy_stuns_nothing() {
        let mut fixture = temple(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, KENNEN).unwrap();
        assert_eq!(ctx.location(KENNEN), Some(Location::Base(0)));
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("pay 2 energy for the {{card {KENNEN}}} trigger?")
        );
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "nothing paid");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {KENNEN}}} trigger is removed · its cost is declined"
        )));
        assert!(!ctx.is_stunned(BRUTE));
        drop(ctx);

        let mut poor = temple(fixtures::BF1);
        for rune in [fixtures::RUNE_A, 41, 42, 43, 100] {
            poor.table.card_mut(rune).unwrap().exhausted = true;
        }
        poor.resolve();
        let mut ctx = poor.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "one energy cannot pay two");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {KENNEN}}} trigger is removed · its cost can't be paid"
        )));
        assert!(!ctx.is_stunned(BRUTE));
    }
}
