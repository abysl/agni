use super::prelude::{activated, buff, done, named, paying_with, spending_xp, unit};
use super::{Card, Cost, Flow, Item, Keyword, SelfCost, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const HUNT: u8 = 1;
pub const BUFF_XP: u8 = 2;

fn showboat(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        ctx.narrate(format!("{{card {me}}} has left the board · no buff"));
        return done();
    }
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    } else {
        ctx.narrate(format!("{{card {me}}} already has a buff"));
    }
    done()
}

pub static CARD: Card = unit(
    "Crowd Favorite",
    &[Keyword::Hunt(HUNT)],
    &[named(
        spending_xp(
            paying_with(
                activated(Timing::Sorcery, Cost::FREE, &[], showboat),
                SelfCost::Free,
            ),
            BUFF_XP,
        ),
        "buff me",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, IMPLICIT_HUNT};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, settle, triggers};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const FAVORITE: u32 = 90;

    fn favorite(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(FAVORITE, zone, seat, "Crowd Favorite", 3);
        card.energy = Some(3);
        card.domain = vec!["Body".into()];
        card
    }

    fn arena(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(favorite(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FAVORITE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == FAVORITE)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    #[test]
    fn the_script_prints_hunt_one_and_a_two_xp_buff_that_exhausts_nothing() {
        assert!(std::ptr::eq(script_of("Crowd Favorite").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hunt(1)]);
        assert_eq!(CARD.hunt(), 1);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.xp, BUFF_XP);
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.self_cost, SelfCost::Free, "no exhaust is printed");
        assert!(ability.targets.is_empty());
        assert_eq!(ability.label, Some("buff me"));
        let mut fixture = arena(2);
        let ctx = fixture.ctx();
        let price = cost::of_activation(&ctx, FAVORITE, 0);
        assert_eq!(price.xp, 2);
        assert_eq!(price.label(), "2 XP");
        assert!(!price.needs_runes());
    }

    #[test]
    fn the_offer_is_greyed_short_of_two_xp_and_a_direct_activation_is_refused() {
        let mut fixture = arena(1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            offers(&ctx),
            [(format!("{{card {FAVORITE}}}: buff me (2 XP)"), false)]
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, FAVORITE, 0),
            Err(Refusal::Illegal(Reason::NotEnoughXp))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, FAVORITE, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(!ctx.is_buffed(FAVORITE));
        assert_eq!(ctx.xp(0), 1);
    }

    #[test]
    fn two_xp_buy_a_buff_when_the_ability_resolves_and_the_unit_stays_ready() {
        let mut fixture = arena(2);
        fixture.table.card_mut(FAVORITE).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        assert_eq!(
            offers(&ctx),
            [(format!("{{card {FAVORITE}}}: buff me (2 XP)"), true)]
        );
        activate::activate(&mut ctx, 0, FAVORITE, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.xp(0), 0, "the XP lands with the cost");
        assert!(ctx.blob.log.contains(&"{seat 0} spends 2 XP".to_string()));
        assert!(
            !ctx.card(FAVORITE).unwrap().exhausted,
            "no exhaust is part of the cost"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == FAVORITE
        ));
        assert!(!ctx.is_buffed(FAVORITE), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(FAVORITE));
        assert_eq!(ctx.current_might(FAVORITE), 4);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FAVORITE}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_second_buff_changes_nothing_and_a_unit_gone_before_resolution_gets_none() {
        let mut fixture = arena(4);
        fixture.table.card_mut(FAVORITE).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        assert!(ctx.buff(FAVORITE));
        activate::activate(&mut ctx, 0, FAVORITE, 0).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), 2, "the XP is spent all the same");
        assert!(ctx.is_buffed(FAVORITE));
        assert_eq!(ctx.current_might(FAVORITE), 4, "one buff, not two");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FAVORITE}}} already has a buff")));
        activate::activate(&mut ctx, 0, FAVORITE, 0).unwrap();
        assert_eq!(ctx.xp(0), 0);
        ctx.kill(FAVORITE, Cause::Rule);
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(FAVORITE));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FAVORITE}}} has left the board · no buff")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn holding_hunts_one_xp_which_is_half_a_buff() {
        let mut fixture = arena(1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(FAVORITE), 1);
        ctx.raise(Event::Held {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![FAVORITE],
        });
        assert_eq!(triggers::collect(&mut ctx), 1);
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index } if source == FAVORITE && index == IMPLICIT_HUNT
        ));
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), 2);
        assert_eq!(
            offers(&ctx),
            [(format!("{{card {FAVORITE}}}: buff me (2 XP)"), true)],
            "the hunt paid for the buff"
        );
    }
}
