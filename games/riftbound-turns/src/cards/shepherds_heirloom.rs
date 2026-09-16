use super::prelude::{done, equip, gain_xp, gear, play, spending_xp, while_attached, with_statics};
use super::{Card, Cost, Flow, Grant, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost::FREE;
pub const EQUIP_XP: u8 = 1;
pub const PLAY_XP: u8 = 1;
pub const MIGHT_BONUS: i16 = 2;
pub const ON_PLAY: u8 = 0;
pub const EQUIP_INDEX: u8 = 1;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

fn inherit(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, PLAY_XP);
    done()
}

pub static CARD: Card = with_statics(
    gear(
        "Shepherd's Heirloom",
        &[Keyword::Equip(EQUIP)],
        &[play(&[], inherit), spending_xp(equip(EQUIP), EQUIP_XP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, play as play_engine, priority};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const HEIRLOOM: u32 = 90;

    fn heirloom(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            power: Some(0),
            domain: vec!["Order".into()],
            ..fixtures::gear(HEIRLOOM, zone, seat, "Shepherd's Heirloom", 2)
        }
    }

    fn with_xp(zone: u16, xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(heirloom(zone, 0));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(HEIRLOOM).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, HEIRLOOM, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        resolve_chain(ctx);
    }

    #[test]
    fn the_script_is_an_order_equipment_that_gains_xp_on_play_and_equips_for_one_xp() {
        assert!(std::ptr::eq(
            script_of("Shepherd's Heirloom").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Shepherd's Heirloom");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(
            CARD.equip_cost(),
            Some(Cost::FREE),
            "an Equip paid in XP alone carries no resource cost"
        );
        assert!(CARD.is_equipment());
        assert_eq!(CARD.abilities.len(), 2);
        let play = &CARD.abilities[usize::from(ON_PLAY)];
        assert_eq!(play.trigger, Trigger::Play);
        assert!(play.targets.is_empty());
        assert!(!play.optional);
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(Cost::FREE));
        assert_eq!(equip.xp, EQUIP_XP);
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(CARD.attached_grants(), [Grant::Might(2)]));
        assert_eq!((PLAY_XP, EQUIP_XP, MIGHT_BONUS), (1, 1, 2));
    }

    #[test]
    fn playing_it_gains_one_xp_which_then_pays_the_equip() {
        let mut fixture = with_xp(fixtures::HAND, 0);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HEIRLOOM).unwrap();
        assert_eq!(ctx.location(HEIRLOOM), Some(Location::Base(0)));
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: ON_PLAY } if source == HEIRLOOM
        ));
        assert_eq!(ctx.xp(0), 0, "nothing until the trigger resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 1);
        assert_eq!(ctx.xp(1), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        let equip_cost = cost::of_activation(&ctx, HEIRLOOM, EQUIP_INDEX);
        assert_eq!(equip_cost.xp, 1);
        assert!(!equip_cost.needs_runes());
        assert_eq!(equip_cost.label(), "1 XP");
        let runes = ctx.runes_of(0).len();
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, HEIRLOOM), Some(fixtures::VI));
        assert_eq!(ctx.xp(0), 0, "the XP is spent at the pay stage");
        assert_eq!(ctx.runes_of(0).len(), runes, "no rune leaves for the Equip");
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert!(ctx.blob.log.contains(&"{seat 0} spends 1 XP".to_string()));
        ctx.detach(HEIRLOOM);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_xp_the_equip_is_refused_and_so_are_the_other_seat_an_enemy_wearer_and_a_worn_heirloom(
    ) {
        let mut fixture = with_xp(fixtures::BASE, 0);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, HEIRLOOM, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotEnoughXp)),
            "416.3 · a cost that can't be paid can't be started"
        );
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == HEIRLOOM && !offer.enabled));
        drop(ctx);

        let mut fixture = with_xp(fixtures::BASE, 1);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == HEIRLOOM && offer.enabled));
        assert_eq!(
            activate::activate(&mut ctx, 1, HEIRLOOM, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, HEIRLOOM, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 1, "a cancelled Equip spends nothing");
        equip_vi(&mut ctx);
        assert_eq!(ctx.xp(0), 0);
        assert_eq!(
            activate::activate(&mut ctx, 0, HEIRLOOM, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }
}
