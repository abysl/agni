use super::prelude::{
    a_card, card_target, deal, done, equipment_of, on_attack, unit, ENEMY_UNIT_HERE,
};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const PER_EQUIPMENT: u8 = 2;
pub const TARGET: TargetSpec = a_card(
    ENEMY_UNIT_HERE,
    "an enemy unit here to deal 2 to for each Equipment attached to me",
);

pub fn shatter_damage(ctx: &Ctx, me: u32) -> u8 {
    let count = u8::try_from(equipment_of(ctx, me).len()).unwrap_or(u8::MAX);
    count.saturating_mul(PER_EQUIPMENT)
}

fn broken_wings(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !ctx.on_board(me) {
        return done();
    }
    let amount = shatter_damage(ctx, me);
    if amount == 0 {
        ctx.narrate(format!(
            "{{card {me}}} deals nothing to {{card {unit}}} · no Equipment is attached to her"
        ));
        return done();
    }
    if deal(ctx, item, unit, amount) {
        ctx.narrate(format!("{{card {me}}} deals {amount} to {{card {unit}}}"));
    }
    done()
}

pub static CARD: Card = unit(
    "Riven, Shattered",
    &[Keyword::Weaponmaster],
    &[on_attack(&[TARGET], broken_wings)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::WEAPONMASTER;
    use crate::cards::prelude::{attach_gear, attached_to};
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const RIVEN: u32 = 90;
    const BRUTE: u32 = 91;
    const SWORD: u32 = 92;
    const BOOTS: u32 = 93;
    const LOOT: u32 = 94;

    fn riven() -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(RIVEN, fixtures::BF1, 0, "Riven, Shattered", 3)
        }
    }

    fn ruins() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(riven());
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 6));
        fixture
            .table
            .cards
            .push(fixtures::gear(SWORD, fixtures::BASE, 0, "B.F. Sword", 3));
        fixture.table.cards.push(fixtures::gear(
            BOOTS,
            fixtures::BASE,
            0,
            "Boots of Swiftness",
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::gear(LOOT, fixtures::BASE, 0, "Loot", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(RIVEN).unwrap(), &CARD));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        assert!(ctx.mark_attacker(RIVEN));
        settle(ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
    }

    #[test]
    fn the_script_is_a_weaponmaster_with_the_shared_play_trigger_and_an_attack_trigger_aimed_here()
    {
        assert!(std::ptr::eq(script_of("Riven, Shattered").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Weaponmaster]);
        assert!(CARD.statics.is_empty());
        assert_eq!(
            CARD.abilities.len(),
            1,
            "821 · Weaponmaster is the engine's"
        );
        assert_eq!(WEAPONMASTER.trigger, Trigger::Play);
        assert_eq!(WEAPONMASTER.label, Some("weaponmaster"));
        assert!(WEAPONMASTER.optional);
        let attack = &CARD.abilities[0];
        assert_eq!(attack.trigger, Trigger::Attacks(Who::Me));
        assert!(!attack.optional);
        assert!(attack.cost.is_none());
        assert_eq!(attack.targets, &[TARGET]);
        assert_eq!(TARGET.filter, ENEMY_UNIT_HERE);
        assert_eq!(PER_EQUIPMENT, 2);
    }

    #[test]
    fn the_damage_is_two_per_attached_equipment_read_as_the_trigger_resolves() {
        let mut fixture = ruins();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, SWORD, RIVEN);
        attach_gear(&mut ctx, LOOT, RIVEN);
        assert_eq!(attached_to(&ctx, SWORD), Some(RIVEN));
        assert_eq!(attached_to(&ctx, LOOT), Some(RIVEN));
        assert_eq!(
            shatter_damage(&ctx, RIVEN),
            2,
            "Loot is a gear, not an Equipment"
        );
        attacks(&mut ctx);
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {BRUTE}}}")
            ],
            "the enemies here"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == RIVEN
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        attach_gear(&mut ctx, BOOTS, RIVEN);
        assert_eq!(shatter_damage(&ctx, RIVEN), 4);
        assert_eq!(ctx.damage_on(BRUTE), 0, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(BRUTE), 4, "two Equipment as it resolves");
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: 4,
            source: Cause::Item(1)
        }));
        assert!(ctx.on_board(BRUTE), "4 on 6 Might is not lethal");
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {RIVEN}}} deals 4 to {{card {BRUTE}}}")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn without_an_equipment_the_chosen_unit_takes_nothing_and_no_enemy_here_means_no_trigger() {
        let mut fixture = ruins();
        let mut ctx = fixture.ctx();
        assert_eq!(shatter_damage(&ctx, RIVEN), 0);
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {RIVEN}}} deals nothing to {{card {BRUTE}}} · no Equipment is attached to her"
        )));
        drop(ctx);

        let mut empty = ruins();
        for enemy in [fixtures::THEIR_UNIT, BRUTE] {
            empty.table.card_mut(enemy).unwrap().zone = Some(fixtures::BASE);
        }
        empty.resolve();
        let mut ctx = empty.ctx();
        assert!(ctx.mark_attacker(RIVEN));
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.chain.is_empty(),
            "no enemy unit here · the trigger is removed"
        );
        drop(ctx);

        let mut fled = ruins();
        let mut ctx = fled.ctx();
        attach_gear(&mut ctx, SWORD, RIVEN);
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.table.card_mut(BRUTE).unwrap().zone = Some(fixtures::BASE);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.damage_on(BRUTE),
            0,
            "an enemy that left the battlefield is no longer here"
        );
    }
}
