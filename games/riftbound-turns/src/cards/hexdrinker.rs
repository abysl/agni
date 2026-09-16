use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power};

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};

pub const DEFLECT: u8 = 1;
pub const MIGHT_BONUS: i16 = 1;

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Keyword(Keyword::Deflect(DEFLECT)),
    Grant::Might(MIGHT_BONUS),
];

pub static CARD: Card = with_statics(
    gear("Hexdrinker", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger};
    use crate::engine::ctx::{Ctx, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, play as play_engine, priority, targets};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const HEXDRINKER: u32 = 90;
    const EQUIP_INDEX: u8 = 0;

    fn hexdrinker(seat: u8) -> CardInfo {
        let mut card = fixtures::gear(HEXDRINKER, fixtures::BASE, seat, "Hexdrinker", 2);
        card.domain = vec!["Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(hexdrinker(0));
        {
            let held = fixture.table.card_mut(42).unwrap();
            held.domain = vec!["Body".into()];
            held.name = "Body Rune".into();
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(HEXDRINKER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, HEXDRINKER, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        resolve_chain(ctx);
    }

    fn their_spell() -> ChainItem {
        ChainItem::new(
            9,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        )
    }

    #[test]
    fn the_script_is_a_body_equipment_whose_effect_text_is_deflect_and_plus_one() {
        assert!(std::ptr::eq(script_of("Hexdrinker").unwrap(), &CARD));
        assert_eq!(CARD.name, "Hexdrinker");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert!(
            !CARD.has_keyword(Keyword::Deflect(DEFLECT)),
            "Deflect is the wearer's"
        );
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
            [Grant::Keyword(Keyword::Deflect(1)), Grant::Might(1)]
        ));
        assert_eq!((DEFLECT, MIGHT_BONUS), (1, 1));
    }

    #[test]
    fn the_wearer_gets_plus_one_and_opponents_pay_a_rainbow_to_choose_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(fixtures::VI), 0);
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, HEXDRINKER), Some(fixtures::VI));
        assert_eq!(ctx.runes_of(0).len(), 3, "the Body rune recycled");
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Deflect(DEFLECT)));
        assert_eq!(ctx.deflect_of(fixtures::VI), 1);
        let spell = their_spell();
        let with = cost::of_item(&ctx, &spell, Some(TargetRef::Card(fixtures::VI)));
        assert!(
            with.power.contains(&cost::Need::Rainbow),
            "807 · an opponent's spell choosing the wearer pays the rainbow"
        );
        assert!(
            targets::deflect_affordable(&ctx, &spell, TargetRef::Card(fixtures::VI)),
            "two Mind runes pay the deflect"
        );
        ctx.table.cards.retain(|card| ![44, 45].contains(&card.id));
        assert!(!targets::deflect_affordable(
            &ctx,
            &spell,
            TargetRef::Card(fixtures::VI)
        ));
        assert!(
            !targets::candidates(&ctx, &spell, &a_unit("a unit"))
                .contains(&TargetRef::Card(fixtures::VI)),
            "an unaffordable deflect hides the wearer from the opponent's spell"
        );
        assert_eq!(ctx.location(HEXDRINKER), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn deflect_and_the_bonus_leave_with_the_hexdrinker() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        ctx.detach(HEXDRINKER);
        assert_eq!(ctx.deflect_of(fixtures::VI), 0);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Deflect(DEFLECT)));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_a_missing_body_rune_and_an_attached_hexdrinker_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, HEXDRINKER, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, HEXDRINKER, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.runes_of(0).len(), 4);
        drop(ctx);

        let mut broke = Fixture::enforced();
        broke.table.cards.push(hexdrinker(0));
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, HEXDRINKER, EQUIP_INDEX),
            Err(Refusal::NoPowerOf)
        );
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, HEXDRINKER, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }
}
