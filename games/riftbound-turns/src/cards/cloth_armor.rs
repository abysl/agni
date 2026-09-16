use super::long_sword::quick_draw;
use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power};

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Mind)],
};

pub const SHIELD: u8 = 2;
pub const MIGHT_BONUS: i16 = 0;

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Keyword(Keyword::Shield(SHIELD)),
    Grant::Might(MIGHT_BONUS),
];

pub static CARD: Card = with_statics(
    gear(
        "Cloth Armor",
        &[Keyword::QuickDraw, Keyword::Reaction, Keyword::Equip(EQUIP)],
        &[quick_draw(), equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::long_sword::{QUICK_DRAW_QUESTION, WEARER};
    use crate::cards::prelude::{attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger};
    use crate::engine::ctx::{Ctx, Location, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, play as play_engine, priority};
    use crate::state::{ItemKind, PromptWhy, FLAG_DEFENDER};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const ARMOR: u32 = 90;
    const QUICK_DRAW: u8 = 0;
    const EQUIP_INDEX: u8 = 1;

    fn armor(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::gear(ARMOR, zone, seat, "Cloth Armor", 1)
        }
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(armor(zone, 0));
        {
            let held = fixture.table.card_mut(42).unwrap();
            held.domain = vec!["Mind".into()];
            held.name = "Mind Rune".into();
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ARMOR).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn might_counters(ctx: &Ctx, card: u32) -> i32 {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Counter {
                    target: Target::Card(held),
                    counter,
                    delta,
                } if *held == card && *counter == COUNTER_MIGHT => Some(*delta),
                _ => None,
            })
            .sum()
    }

    #[test]
    fn the_script_is_a_quick_draw_mind_equipment_whose_effect_text_is_shield_two_with_no_bonus() {
        assert!(std::ptr::eq(script_of("Cloth Armor").unwrap(), &CARD));
        assert_eq!(CARD.name, "Cloth Armor");
        assert_eq!(
            CARD.keywords,
            [Keyword::QuickDraw, Keyword::Reaction, Keyword::Equip(EQUIP)]
        );
        assert!(
            !CARD.has_keyword(Keyword::Shield(SHIELD)),
            "Shield is the wearer's"
        );
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(CARD.abilities.len(), 2);
        let draw = &CARD.abilities[usize::from(QUICK_DRAW)];
        assert_eq!(draw.trigger, Trigger::Play);
        assert!(draw.targets.is_empty());
        assert_eq!(draw.question, Some(QUICK_DRAW_QUESTION));
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Keyword(Keyword::Shield(2)), Grant::Might(0)]
        ));
        assert_eq!((SHIELD, MIGHT_BONUS), (2, 0));
    }

    #[test]
    fn played_it_attaches_to_a_chosen_unit_that_then_has_shield_two_while_defending() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ARMOR).unwrap();
        assert_eq!(ctx.location(ARMOR), Some(Location::Base(0)));
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: QUICK_DRAW } if source == ARMOR
        ));
        resolve_chain(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume { stage: WEARER, .. })
        ));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(attached_to(&ctx, ARMOR), Some(fixtures::VI));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Shield(SHIELD)));
        assert_eq!(ctx.current_might(fixtures::VI), 3, "+0 outside a defence");
        assert_eq!(might_counters(&ctx, fixtures::VI), 0);
        ctx.set_flag(fixtures::VI, FLAG_DEFENDER, true);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "804 · Shield 2 while the wearer is a defender"
        );
        ctx.detach(ARMOR);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Shield(SHIELD)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_loose_armor_equips_for_a_mind_rune_and_the_wrong_seat_wearer_and_rune_are_refused() {
        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, ARMOR, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, ARMOR, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.runes_of(0).len(), 3, "the Mind rune recycled");
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, ARMOR), Some(fixtures::VI));
        assert_eq!(
            activate::activate(&mut ctx, 0, ARMOR, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
        drop(ctx);

        let mut broke = Fixture::enforced();
        broke.table.cards.push(armor(fixtures::BASE, 0));
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, ARMOR, EQUIP_INDEX),
            Err(Refusal::NoPowerOf),
            "seat 0 holds no Mind rune"
        );
    }
}
