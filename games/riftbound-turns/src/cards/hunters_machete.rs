use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power};

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};

pub const MIGHT_BONUS: i16 = 2;
pub const HUNT: u8 = 1;

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Keyword(Keyword::Hunt(HUNT)),
    Grant::Might(MIGHT_BONUS),
];

pub static CARD: Card = with_statics(
    gear(
        "Hunter's Machete",
        &[Keyword::Equip(EQUIP)],
        &[equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{conquered, held};
    use crate::cards::prelude::{attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger, IMPLICIT_HUNT};
    use crate::engine::ctx::{Ctx, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, play as play_engine, priority, settle, triggers};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MACHETE: u32 = 90;
    const EQUIP_INDEX: u8 = 0;
    const BODY_RUNE: u32 = 42;

    fn machete(seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            ..fixtures::gear(MACHETE, fixtures::BASE, seat, "Hunter's Machete", 3)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(machete(0));
        {
            let held = fixture.table.card_mut(BODY_RUNE).unwrap();
            held.domain = vec!["Body".into()];
            held.name = "Body Rune".into();
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MACHETE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, MACHETE, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        resolve_chain(ctx);
    }

    #[test]
    fn the_script_is_a_body_equipment_whose_effect_text_is_hunt_and_plus_two() {
        assert!(std::ptr::eq(script_of("Hunter's Machete").unwrap(), &CARD));
        assert_eq!(CARD.name, "Hunter's Machete");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert!(!CARD.has_keyword(Keyword::Hunt(0)), "Hunt is the wearer's");
        assert_eq!(CARD.hunt(), 0);
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
            [Grant::Keyword(Keyword::Hunt(1)), Grant::Might(2)]
        ));
        assert_eq!((MIGHT_BONUS, HUNT), (2, 1));
    }

    #[test]
    fn the_wearer_gets_plus_two_and_hunt_and_its_conquer_gains_one_xp_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(fixtures::VI), 0);
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, MACHETE), Some(fixtures::VI));
        assert_eq!(ctx.runes_of(0).len(), 3, "the Body rune recycled");
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(ctx.location(MACHETE), Some(Location::Base(0)));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Hunt(1)));
        assert_eq!(
            ctx.hunt_value(fixtures::VI),
            1,
            "823.2 · granted Hunt counts"
        );
        assert_eq!(ctx.xp(0), 0);
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "the wearer's own implicit Hunt"
        );
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index: IMPLICIT_HUNT } if source == fixtures::VI
        ));
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "383.4.c.2.a · the hunt waits on the chain"
        );
        resolve_chain(&mut ctx);
        assert_eq!(ctx.xp(0), 1);
        assert_eq!(ctx.xp(1), 0);
        assert!(ctx.blob.chain.is_empty());
        ctx.raise(held(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 1, "a hold hunts too");
        settle(&mut ctx).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.xp(0), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn hunt_and_the_bonus_leave_with_the_machete() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        ctx.detach(MACHETE);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Hunt(1)));
        assert_eq!(ctx.hunt_value(fixtures::VI), 0);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 0, "no Hunt, no XP");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_a_missing_body_rune_and_an_attached_machete_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, MACHETE, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, MACHETE, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = Fixture::enforced();
        broke.table.cards.push(machete(0));
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, MACHETE, EQUIP_INDEX),
            Err(Refusal::NoPowerOf),
            "seat 0 holds no Body rune"
        );
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, MACHETE, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }
}
