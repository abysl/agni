use super::long_sword::quick_draw;
use super::prelude::{equip, gear, is_attached, while_attached, with_statics, RAINBOW};
use super::{Card, Cost, Grant, Keyword};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = RAINBOW;

pub const MIGHT_BONUS: i16 = 3;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

pub fn expires(ctx: &Ctx, axe: u32) -> bool {
    ctx.is_temporary(axe) && !is_attached(ctx, axe)
}

pub static CARD: Card = with_statics(
    gear(
        "Spinning Axe",
        &[
            Keyword::QuickDraw,
            Keyword::Reaction,
            Keyword::Equip(EQUIP),
            Keyword::Temporary,
        ],
        &[quick_draw(), equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::long_sword::{QUICK_DRAW_QUESTION, WEARER};
    use crate::cards::prelude::{attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger, IMPLICIT_TEMPORARY};
    use crate::engine::ctx::{Ctx, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, phases, play as play_engine, priority};
    use crate::state::{ItemKind, Phase, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const AXE: u32 = 90;
    const QUICK_DRAW: u8 = 0;
    const EQUIP_INDEX: u8 = 1;

    fn axe(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Fury".into(), "Chaos".into()],
            ..fixtures::gear(AXE, zone, seat, "Spinning Axe", 2)
        }
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(axe(zone, 0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(AXE).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn temporary_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| {
                matches!(
                    item.kind,
                    ItemKind::Trigger { source, index: IMPLICIT_TEMPORARY } if source == AXE
                )
            })
            .count()
    }

    #[test]
    fn the_script_is_a_quick_draw_temporary_equipment_with_a_rainbow_equip_and_plus_three() {
        assert!(std::ptr::eq(script_of("Spinning Axe").unwrap(), &CARD));
        assert_eq!(CARD.name, "Spinning Axe");
        assert_eq!(
            CARD.keywords,
            [
                Keyword::QuickDraw,
                Keyword::Reaction,
                Keyword::Equip(EQUIP),
                Keyword::Temporary
            ]
        );
        assert_eq!(CARD.equip_cost(), Some(RAINBOW));
        assert_eq!(CARD.abilities.len(), 2);
        let draw = &CARD.abilities[usize::from(QUICK_DRAW)];
        assert_eq!(draw.trigger, Trigger::Play);
        assert!(draw.targets.is_empty());
        assert_eq!(draw.question, Some(QUICK_DRAW_QUESTION));
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(RAINBOW));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(CARD.attached_grants(), [Grant::Might(3)]));
        assert_eq!(MIGHT_BONUS, 3);
        assert_eq!(
            crate::cards::generic::TEMPORARY.trigger,
            Trigger::BeginningPhase,
            "816 · the implicit trigger is the generic one; the unattached condition is triggers::sources' own"
        );
    }

    #[test]
    fn played_it_attaches_to_a_chosen_unit_for_plus_three_and_a_loose_axe_equips_for_any_rune() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, AXE).unwrap();
        assert_eq!(ctx.location(AXE), Some(Location::Base(0)));
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: QUICK_DRAW } if source == AXE
        ));
        resolve_chain(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume { stage: WEARER, .. })
        ));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(attached_to(&ctx, AXE), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert!(!expires(&ctx, AXE), "attached, the axe is safe");
        ctx.detach(AXE);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(expires(&ctx, AXE));
        drop(ctx);

        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, AXE, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, AXE, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.runes_of(0).len(), 3, "any one rune pays the rainbow");
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, AXE), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert_eq!(
            activate::activate(&mut ctx, 0, AXE, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }

    #[test]
    fn an_unattached_axe_dies_at_its_controllers_beginning_phase_and_an_attached_one_survives() {
        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(expires(&ctx, AXE));
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(temporary_items(&ctx), 1, "816.1.b · Temporary triggers");
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(AXE));
        assert_eq!(ctx.card(AXE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} is Temporary and dies".to_string()));
        drop(ctx);

        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, AXE, EQUIP_INDEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, AXE), Some(fixtures::VI));
        assert!(!expires(&ctx, AXE));
        phases::start_turn(&mut ctx);
        assert_eq!(
            temporary_items(&ctx),
            0,
            "722.2 · attached, its rules text is inactive and Temporary does not trigger"
        );
        assert!(ctx.on_board(AXE));
        assert_eq!(attached_to(&ctx, AXE), Some(fixtures::VI));
        assert!(
            ctx.has_keyword(AXE, Keyword::Temporary),
            "722.1 · it still has the keyword for effects that look for it"
        );
        assert!(ctx.fault.is_none());
    }
}
