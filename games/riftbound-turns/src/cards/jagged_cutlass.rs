use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power, Static};
use crate::engine::ctx::Ctx;
use crate::engine::march;
use crate::state::ChainItem;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};

pub const MIGHT_BONUS: i16 = 2;

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Static(Static::NoMoveByEnemy),
    Grant::Might(MIGHT_BONUS),
];

pub fn cannot_be_moved_by_enemy_items(ctx: &Ctx, unit: u32) -> bool {
    ctx.has_static(unit, Static::NoMoveByEnemy)
}

pub fn item_cannot_move(ctx: &Ctx, item: &ChainItem, unit: u32) -> bool {
    march::item_cannot_move(ctx, item, unit)
}

pub static CARD: Card = with_statics(
    gear("Jagged Cutlass", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, GEAR};
    use crate::cards::prelude::{attach_gear, attached_to, is_attached, FRIENDLY_UNIT};
    use crate::cards::Filter;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::{Cause, Killed, Location, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, march, priority, targets};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;

    const EQUIP_INDEX: u8 = 0;
    const BODY_RUNE: u32 = 46;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Jagged Cutlass", 3, "Body"));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GEAR).unwrap(), &CARD));
        fixture
    }

    fn their_spell() -> ChainItem {
        ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        )
    }

    fn my_spell() -> ChainItem {
        ChainItem::new(
            8,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        )
    }

    #[test]
    fn the_script_is_a_body_equipment_with_plus_two_and_no_ability_beyond_the_equip() {
        assert!(std::ptr::eq(script_of("Jagged Cutlass").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert!(CARD.is_equipment());
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(
            !CARD.has_static(Static::NoMoveByEnemy),
            "the veto is the wearer's, not the loose cutlass's"
        );
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Static(Static::NoMoveByEnemy), Grant::Might(2)]
        ));
        assert_eq!(MIGHT_BONUS, 2);
    }

    #[test]
    fn equipping_costs_a_body_rune_and_the_wearer_reads_plus_two_until_the_cutlass_leaves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(attached_to(&ctx, GEAR), Some(fixtures::VI));
        assert!(
            ctx.card(BODY_RUNE).unwrap().zone != Some(fixtures::RUNE_POOL),
            "the Body rune is recycled for the power"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(
            ctx.location(GEAR),
            Some(Location::Battlefield(fixtures::BF1)),
            "421.4 · the cutlass stands where the wearer stands"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +2 Might while {card 90} is attached".to_string()));
        assert_eq!(ctx.kill(GEAR, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert!(!is_attached(&ctx, GEAR));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_veto_reads_the_wearer_against_enemy_items_only_and_never_an_unarmed_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(!cannot_be_moved_by_enemy_items(&ctx, fixtures::VI));
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert!(cannot_be_moved_by_enemy_items(&ctx, fixtures::VI));
        assert!(!cannot_be_moved_by_enemy_items(&ctx, fixtures::THEIR_UNIT));
        assert!(item_cannot_move(&ctx, &their_spell(), fixtures::VI));
        assert!(
            !item_cannot_move(&ctx, &my_spell(), fixtures::VI),
            "your own spells and abilities may still move the wearer"
        );
        assert!(
            !item_cannot_move(&ctx, &their_spell(), fixtures::THEIR_UNIT),
            "their own unit is theirs to move"
        );
        assert!(ctx.set_controller(fixtures::VI, 1, fixtures::SPRITE));
        assert!(
            item_cannot_move(&ctx, &my_spell(), fixtures::VI),
            "under a new controller the old owner's items are the enemy's"
        );
        assert!(!item_cannot_move(&ctx, &their_spell(), fixtures::VI));
        ctx.detach(GEAR);
        assert!(!cannot_be_moved_by_enemy_items(&ctx, fixtures::VI));
        assert!(!item_cannot_move(&ctx, &my_spell(), fixtures::VI));
    }

    #[test]
    fn a_friendly_effect_still_moves_the_wearer_and_the_bonus_survives_the_move() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert!(
            ctx.projected_statics(fixtures::VI)
                .iter()
                .any(|held| held.same_kind(Static::NoMoveByEnemy)),
            "the wearer projects the veto"
        );
        assert!(!item_cannot_move(&ctx, &my_spell(), fixtures::VI));
        assert_eq!(
            march::effect_move(&mut ctx, &my_spell(), fixtures::VI, Location::Base(0)),
            Some(Moved::Moved)
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "the bonus survives the move"
        );
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Base(0),
                Location::Battlefield(fixtures::BF1)
            ),
            Ok(()),
            "the wearer's own marches are never bound"
        );
    }

    #[test]
    fn the_other_seat_a_missing_body_rune_and_an_enemy_wearer_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, GEAR, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, GEAR));
        assert_eq!(ctx.runes_of(0).len(), 5, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = armed();
        broke.table.cards.retain(|card| card.id != BODY_RUNE);
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX),
            Err(Refusal::NoPowerOf)
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }

    #[test]
    fn an_enemy_spell_cannot_move_the_wearer_while_a_friendly_one_still_can() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert!(ctx
            .projected_statics(fixtures::VI)
            .iter()
            .any(|held| !held.same_kind(Static::NoMoveToBase)));
        assert!(item_cannot_move(&ctx, &their_spell(), fixtures::VI));
        assert_eq!(
            march::effect_move(&mut ctx, &their_spell(), fixtures::VI, Location::Base(0)),
            None
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} can't be moved by {card 80}".to_string()));
        assert!(
            !targets::matches(
                &ctx,
                &their_spell(),
                &Filter::Movable,
                TargetRef::Card(fixtures::VI)
            ),
            "an enemy spell choosing a movable unit skips the wearer"
        );
        assert!(targets::matches(
            &ctx,
            &my_spell(),
            &Filter::Movable,
            TargetRef::Card(fixtures::VI)
        ));
        assert_eq!(
            march::effect_move(&mut ctx, &my_spell(), fixtures::VI, Location::Base(0)),
            Some(Moved::Moved)
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
    }
}
