use super::long_sword::quick_draw;
use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power};

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};

pub const MIGHT_BONUS: i16 = 3;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

pub static CARD: Card = with_statics(
    gear(
        "Sterak's Gage",
        &[Keyword::QuickDraw, Keyword::Reaction, Keyword::Equip(EQUIP)],
        &[quick_draw(), equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::long_sword::{QUICK_DRAW_QUESTION, WEARER};
    use crate::cards::prelude::{attached_to, is_attached, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger};
    use crate::engine::ctx::{Cause, Ctx, Killed, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, play as play_engine, priority};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const GAGE: u32 = 90;
    const QUICK_DRAW: u8 = 0;
    const EQUIP_INDEX: u8 = 1;

    fn gage(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            power: Some(2),
            domain: vec!["Calm".into()],
            ..fixtures::gear(GAGE, zone, seat, "Sterak's Gage", 3)
        }
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(gage(zone, 0));
        for rune in [41, 43] {
            let held = fixture.table.card_mut(rune).unwrap();
            held.domain = vec!["Calm".into()];
            held.name = "Calm Rune".into();
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GAGE).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_quick_draw_calm_equipment_with_plus_three() {
        assert!(std::ptr::eq(script_of("Sterak's Gage").unwrap(), &CARD));
        assert_eq!(CARD.name, "Sterak's Gage");
        assert_eq!(
            CARD.keywords,
            [Keyword::QuickDraw, Keyword::Reaction, Keyword::Equip(EQUIP)]
        );
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(CARD.abilities.len(), 2);
        let draw = &CARD.abilities[usize::from(QUICK_DRAW)];
        assert_eq!(draw.trigger, Trigger::Play);
        assert!(draw.targets.is_empty());
        assert!(draw.candidates.is_some());
        assert_eq!(draw.question, Some(QUICK_DRAW_QUESTION));
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(CARD.attached_grants(), [Grant::Might(3)]));
        assert_eq!(MIGHT_BONUS, 3);
    }

    #[test]
    fn played_for_three_energy_and_two_calm_it_attaches_to_a_chosen_unit_for_plus_three() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GAGE).unwrap();
        assert_eq!(ctx.location(GAGE), Some(Location::Base(0)));
        assert_eq!(ctx.runes_of(0).len(), 2, "two Calm runes recycled");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: QUICK_DRAW } if source == GAGE
        ));
        resolve_chain(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume { stage: WEARER, .. })
        ));
        assert_eq!(fixtures::labels(&ctx), ["{card 50}"]);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, GAGE), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +3 Might while {card 90} is attached".to_string()));
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert_eq!(attached_to(&ctx, GAGE), None);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_loose_gage_equips_for_a_calm_rune_and_the_wrong_seat_wearer_or_an_attached_gage_is_refused(
    ) {
        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, GAGE, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, GAGE, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.runes_of(0).len(), 3);
        assert!(!is_attached(&ctx, GAGE));
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, GAGE), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert_eq!(
            activate::activate(&mut ctx, 0, GAGE, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
        ctx.detach(GAGE);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        drop(ctx);

        let mut broke = Fixture::enforced();
        broke.table.cards.push(gage(fixtures::BASE, 0));
        broke.table.card_mut(42).unwrap().domain = vec!["Fury".into()];
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, GAGE, EQUIP_INDEX),
            Err(Refusal::NoPowerOf)
        );
    }
}
