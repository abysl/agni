use super::prelude::{deal, done, enemy_units, in_combat, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;

pub fn enemy_units_in_combat(ctx: &Ctx, seat: u8) -> Vec<u32> {
    enemy_units(ctx, seat)
        .into_iter()
        .filter(|unit| in_combat(ctx, *unit))
        .collect()
}

fn barrage(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let targets = enemy_units_in_combat(ctx, item.controller);
    if targets.is_empty() {
        ctx.narrate(format!(
            "{{card {}}} finds no enemy unit in combat",
            item.kind.source()
        ));
    }
    for unit in targets {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Cannon Barrage",
    &[Keyword::Reaction],
    &[play(&[], barrage)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use agni_plugin_sdk::table::CardInfo;

    const BARRAGE: u32 = 90;
    const RAIDER: u32 = 91;
    const BYSTANDER: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Body".into()],
            ..fixtures::spell(BARRAGE, fixtures::HAND, 0, "Cannon Barrage", 2, 1)
        });
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Body", false));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF2, 1, "Raider", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(BYSTANDER, fixtures::BF1, 1, "Bystander", 2));
        fixture.resolve();
        fixture
    }

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt {
                    card,
                    n,
                    source: Cause::Item(1),
                } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn cannon_barrage_is_a_reaction_without_targets() {
        assert!(std::ptr::eq(script_of("Cannon Barrage").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
    }

    #[test]
    fn every_enemy_unit_designated_in_combat_takes_two_and_the_rest_are_untouched() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(fixtures::VI));
        assert!(ctx.mark_defender(fixtures::SPRITE));
        assert!(ctx.mark_defender(RAIDER));
        assert_eq!(
            enemy_units_in_combat(&ctx, 0),
            [fixtures::SPRITE, RAIDER],
            "740.2.c · designated units at the combat battlefield"
        );
        assert!(enemy_units_in_combat(&ctx, 1).contains(&fixtures::VI));
        fixtures::play_from_hand(&mut ctx, 0, BARRAGE).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            damage_events(&ctx),
            [(fixtures::SPRITE, DAMAGE), (RAIDER, DAMAGE)]
        );
        assert_eq!(ctx.damage_on(fixtures::VI), 0, "friendly units are spared");
        assert_eq!(
            ctx.damage_on(BYSTANDER),
            0,
            "an enemy at another battlefield is not in combat"
        );
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(!ctx.on_board(RAIDER), "two kills the 2-Might raider");
        assert!(ctx.on_board(fixtures::SPRITE), "two on three Might");
        assert_eq!(ctx.card(BARRAGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_combat_it_resolves_and_deals_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(enemy_units_in_combat(&ctx, 0).is_empty());
        fixtures::play_from_hand(&mut ctx, 0, BARRAGE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(damage_events(&ctx).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} finds no enemy unit in combat".to_string()));
        assert_eq!(ctx.card(BARRAGE).unwrap().zone, Some(fixtures::TRASH));
    }
}
