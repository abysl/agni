use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power};

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};

pub const MIGHT_BONUS: i16 = 1;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Keyword(Keyword::Tank), Grant::Might(MIGHT_BONUS)];

pub static CARD: Card = with_statics(
    gear("Doran's Shield", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger};
    use crate::engine::ctx::{Cause, Ctx, Killed, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, combat, play as play_engine, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SHIELD: u32 = 90;
    const ALLY: u32 = 95;
    const EQUIP_INDEX: u8 = 0;

    fn shield(seat: u8) -> CardInfo {
        let mut card = fixtures::gear(SHIELD, fixtures::BASE, seat, "Doran's Shield", 1);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shield(0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Wailer", 4));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SHIELD).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, SHIELD, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        resolve_chain(ctx);
    }

    #[test]
    fn the_script_is_a_calm_equipment_whose_effect_text_is_tank_and_plus_one() {
        assert!(std::ptr::eq(script_of("Doran's Shield").unwrap(), &CARD));
        assert_eq!(CARD.name, "Doran's Shield");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert!(!CARD.has_keyword(Keyword::Tank), "Tank is the wearer's");
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[0];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Keyword(Keyword::Tank), Grant::Might(1)]
        ));
        assert_eq!(MIGHT_BONUS, 1);
    }

    #[test]
    fn the_wearer_gets_plus_one_and_is_assigned_combat_damage_first() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            combat::ordered(&ctx, &[ALLY, fixtures::VI], false),
            [ALLY, fixtures::VI],
            "without Tank the defender's order is free"
        );
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, SHIELD), Some(fixtures::VI));
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "one Calm rune recycled for the power"
        );
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Tank));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(
            combat::ordered(&ctx, &[ALLY, fixtures::VI], false),
            [fixtures::VI],
            "815 · Tank: the wearer must be assigned combat damage first"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +1 Might while {card 90} is attached".to_string()));
        assert_eq!(ctx.location(SHIELD), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn tank_and_the_bonus_leave_with_the_shield() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        ctx.detach(SHIELD);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Tank));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert_eq!(attached_to(&ctx, SHIELD), None);
        assert_eq!(
            ctx.location(SHIELD),
            Some(Location::Base(0)),
            "the shield the dead wearer left behind stays home"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_a_missing_calm_rune_and_an_attached_shield_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, SHIELD, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, SHIELD, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = armed();
        {
            let held = broke.table.card_mut(42).unwrap();
            held.domain = vec!["Fury".into()];
            held.name = "Fury Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SHIELD, EQUIP_INDEX),
            Err(Refusal::NoPowerOf)
        );
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, SHIELD, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }
}
