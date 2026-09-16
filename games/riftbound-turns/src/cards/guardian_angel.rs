use super::prelude::{
    attached_to, equip, gear, heal, recall, replaces, while_attached, with_replacement,
    with_statics,
};
use super::{Card, Cost, Domain, Grant, Keyword, Power, Source, WouldDie};
use crate::engine::ctx::{Cause, Ctx};

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};

pub const MIGHT_BONUS: i16 = 1;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

fn the_wearer_would_die(ctx: &Ctx, would: &WouldDie, source: Source) -> bool {
    attached_to(ctx, source.card) == Some(would.unit) && ctx.on_board(would.unit)
}

fn kill_the_angel_instead(ctx: &mut Ctx, would: &WouldDie, source: Source) {
    let me = would.unit;
    ctx.kill(source.card, Cause::Replacement);
    if heal(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is healed"));
    }
    recall(ctx, me, true);
    ctx.narrate(format!(
        "{{card {me}}} is recalled exhausted instead of dying"
    ));
}

pub static CARD: Card = with_replacement(
    with_statics(
        gear("Guardian Angel", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
        &[while_attached(EFFECT_TEXT)],
    ),
    replaces(the_wearer_would_die, kill_the_angel_instead),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, GEAR};
    use crate::cards::prelude::{attach_gear, deal, is_attached, Location};
    use crate::cards::{script_of, Item, Static};
    use crate::engine::ctx::{Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, kill, showdown};
    use crate::state::{ChainItem, ItemKind, Origin};

    const KILLER: u32 = 91;
    const ALLY: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Guardian Angel", 2, "Calm"));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn died(ctx: &Ctx, card: u32) -> bool {
        ctx.events
            .iter()
            .any(|event| matches!(event, Event::Died { card: dead, .. } if *dead == card))
    }

    fn spark() -> Item {
        ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            Origin::Hand,
        )
    }

    #[test]
    fn the_script_is_a_calm_equipment_with_one_might_whose_attached_text_is_a_replacement() {
        assert!(std::ptr::eq(script_of("Guardian Angel").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(
            CARD.abilities.len(),
            1,
            "Equip alone; a replacement is no ability"
        );
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(CARD.attached_grants(), [Grant::Might(1)]));
        assert!(CARD.replacement.is_some());
    }

    #[test]
    fn the_replacement_applies_to_the_wearer_alone_and_only_while_attached() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let source = Source {
            card: GEAR,
            ability: 0,
        };
        let would = |unit: u32| WouldDie {
            unit,
            cause: Cause::Combat,
        };
        assert!(kill::applicable(&ctx, &would(fixtures::VI)).is_empty());
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(kill::applicable(&ctx, &would(fixtures::VI)), [source]);
        assert!(
            kill::applicable(&ctx, &would(ALLY)).is_empty(),
            "another friendly unit"
        );
        assert!(kill::applicable(&ctx, &would(fixtures::THEIR_UNIT)).is_empty());
        assert!(
            kill::applicable(&ctx, &would(GEAR)).is_empty(),
            "the angel itself"
        );
    }

    #[test]
    fn lethal_spell_damage_kills_the_angel_instead_and_the_wearer_comes_home_healed_and_exhausted()
    {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let spark = spark();
        assert!(deal(&mut ctx, &spark, fixtures::VI, 4));
        assert_eq!(ctx.damage_on(fixtures::VI), 4);
        assert_eq!(ctx.kill(fixtures::VI, Cause::Item(7)), Killed::Replaced);
        assert!(ctx.on_board(fixtures::VI), "the wearer never died");
        assert!(!died(&ctx, fixtures::VI));
        assert!(died(&ctx, GEAR));
        assert_eq!(ctx.card(GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert!(!is_attached(&ctx, GEAR));
        assert_eq!(ctx.damage_on(fixtures::VI), 0, "healed");
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "recalled"
        );
        assert!(ctx.card(fixtures::VI).unwrap().exhausted, "exhausted");
        cleanup::run(&mut ctx, None);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "136.3.a · the bonus left with the angel"
        );
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} replaces the death of {card 50}".to_string()));
        assert!(ctx.blob.log.contains(&"{card 50} is healed".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} is recalled exhausted instead of dying".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_combat_death_is_replaced_once_and_the_second_death_is_real() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::unit(KILLER, fixtures::BF1, 1, "Jinx", 6));
        fixture.table.card_mut(ALLY).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        showdown::pass(&mut ctx, 1).unwrap();
        showdown::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.showdown.is_none(), "the combat resolved");
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(!died(&ctx, fixtures::VI));
        assert_eq!(ctx.card(GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
        assert_eq!(
            ctx.kill(fixtures::VI, Cause::Rule),
            Killed::Yes,
            "the angel is spent"
        );
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_unattached_angel_guards_nobody_and_another_units_death_leaves_it_be() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(GEAR));
        attach_gear(&mut ctx, GEAR, ALLY);
        assert_eq!(ctx.kill(fixtures::THEIR_UNIT, Cause::Rule), Killed::Yes);
        assert!(ctx.on_board(GEAR), "an enemy death does not spend it");
        assert_eq!(
            ctx.kill(GEAR, Cause::Rule),
            Killed::Yes,
            "the angel's own death is a death"
        );
        assert!(!ctx.on_board(GEAR));
        assert_eq!(
            ctx.kill(ALLY, Cause::Rule),
            Killed::Yes,
            "the angel is gone"
        );
    }
}
