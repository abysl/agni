use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power, Static};

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};

pub const MIGHT_BONUS: i16 = 1;
pub const LEVEL: u8 = 3;
pub const LEVEL_BONUS: i16 = 1;

pub static LEVEL_TEXT: &[Grant] = &[Grant::Might(LEVEL_BONUS)];

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Static(Static::Level(LEVEL, LEVEL_TEXT)),
    Grant::Might(MIGHT_BONUS),
];

pub static CARD: Card = with_statics(
    gear("Soul Sword", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::{Ctx, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, attach, play as play_engine, priority, statics};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SWORD: u32 = 90;
    const EQUIP_INDEX: u8 = 0;

    fn sword(seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Calm".into()],
            ..fixtures::gear(SWORD, fixtures::BASE, seat, "Soul Sword", 1)
        }
    }

    fn armed(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sword(0));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SWORD).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, SWORD, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        resolve_chain(ctx);
    }

    #[test]
    fn the_script_is_a_calm_equipment_whose_effect_text_is_a_level_three_bonus_and_plus_one() {
        assert!(std::ptr::eq(script_of("Soul Sword").unwrap(), &CARD));
        assert_eq!(CARD.name, "Soul Sword");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[0];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(
            !CARD.has_static(Static::Level(0, &[])),
            "the Level is the wearer's, not the loose sword's"
        );
        let [Grant::Static(Static::Level(level, granted)), Grant::Might(1)] =
            CARD.attached_grants()
        else {
            panic!("the effect text is the Level line then the bonus");
        };
        assert_eq!(*level, LEVEL);
        assert!(matches!(granted, [Grant::Might(1)]));
        assert_eq!((MIGHT_BONUS, LEVEL, LEVEL_BONUS), (1, 3, 1));
    }

    #[test]
    fn equipping_pays_a_calm_rune_gives_plus_one_and_hands_the_wearer_the_level_static() {
        let mut fixture = armed(0);
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, SWORD), Some(fixtures::VI));
        assert_eq!(ctx.runes_of(0).len(), 3, "the Calm rune recycled");
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(ctx.location(SWORD), Some(Location::Base(0)));
        assert!(
            attach::has_static(&ctx, fixtures::VI, Static::Level(LEVEL, &[])),
            "the wearer carries the Level as a granted static"
        );
        assert!(
            !statics::level_active(&ctx, fixtures::VI, LEVEL),
            "824.1 · no XP, no Level"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +1 Might while {card 90} is attached".to_string()));
        ctx.detach(SWORD);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(!attach::has_static(
            &ctx,
            fixtures::VI,
            Static::Level(LEVEL, &[])
        ));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = armed(3);
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert!(statics::level_active(&ctx, fixtures::VI, LEVEL));
        assert!(!statics::level_active(&ctx, fixtures::THEIR_UNIT, LEVEL));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "the granted Level line projects at 3 XP"
        );
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_a_missing_calm_rune_and_an_attached_sword_are_refused() {
        let mut fixture = armed(0);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, SWORD, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, SWORD, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = armed(0);
        {
            let held = broke.table.card_mut(42).unwrap();
            held.domain = vec!["Fury".into()];
            held.name = "Fury Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SWORD, EQUIP_INDEX),
            Err(Refusal::NoPowerOf)
        );
        drop(ctx);

        let mut fixture = armed(0);
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, SWORD, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }

    #[test]
    fn at_three_xp_the_wearer_has_an_additional_plus_one() {
        let mut fixture = armed(3);
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        ctx.detach(SWORD);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        drop(ctx);

        let mut fixture = armed(2);
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 4, "two XP is not Level 3");
        ctx.score_xp(0, 1);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
    }
}
