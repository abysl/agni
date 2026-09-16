use super::prelude::{activated, buff, done, named, paying_with, spending_xp, unit};
use super::{Card, Cost, Flow, Item, Keyword, SelfCost, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const HUNT: u8 = 1;
pub const BUFF_XP: u8 = 2;

fn enthrall(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
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
    "Enthralling Protector",
    &[Keyword::Hunt(HUNT)],
    &[named(
        spending_xp(
            paying_with(
                activated(Timing::Sorcery, Cost::FREE, &[], enthrall),
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
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, priority, settle, triggers};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const PROTECTOR: u32 = 90;

    fn protector(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(PROTECTOR, zone, seat, "Enthralling Protector", 2);
        card.energy = Some(2);
        card.domain = vec!["Order".into()];
        card
    }

    fn glade(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(protector(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PROTECTOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == PROTECTOR)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    #[test]
    fn the_script_prints_hunt_one_and_a_two_xp_buff_that_exhausts_nothing() {
        assert!(std::ptr::eq(
            script_of("Enthralling Protector").unwrap(),
            &CARD
        ));
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
        let mut fixture = glade(2);
        let ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, PROTECTOR, 0).label(), "2 XP");
    }

    #[test]
    fn the_offer_is_greyed_short_of_two_xp_and_the_chain_must_be_open() {
        let mut fixture = glade(1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            offers(&ctx),
            [(format!("{{card {PROTECTOR}}}: buff me (2 XP)"), false)]
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, PROTECTOR, 0),
            Err(Refusal::Illegal(Reason::NotEnoughXp))
        );
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.xp(0), 1);
        drop(ctx);
        let mut busy = glade(4);
        let mut ctx = busy.ctx();
        activate::activate(&mut ctx, 0, PROTECTOR, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            activate::activate(&mut ctx, 0, PROTECTOR, 0),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "a focus ability waits for the chain to clear"
        );
        assert_eq!(ctx.xp(0), 2, "the second activation spent nothing");
    }

    #[test]
    fn two_xp_buy_a_buff_when_the_ability_resolves_even_while_exhausted() {
        let mut fixture = glade(2);
        fixture.table.card_mut(PROTECTOR).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        assert_eq!(
            offers(&ctx),
            [(format!("{{card {PROTECTOR}}}: buff me (2 XP)"), true)],
            "an exhausted unit still spends XP"
        );
        activate::activate(&mut ctx, 0, PROTECTOR, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.xp(0), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} spends 2 XP".to_string()));
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == PROTECTOR
        ));
        assert!(!ctx.is_buffed(PROTECTOR), "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(PROTECTOR));
        assert_eq!(ctx.current_might(PROTECTOR), 3);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PROTECTOR}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn conquering_hunts_one_xp_toward_the_next_buff() {
        let mut fixture = glade(1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(PROTECTOR), 1);
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![PROTECTOR],
        });
        assert_eq!(triggers::collect(&mut ctx), 1);
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index } if source == PROTECTOR && index == IMPLICIT_HUNT
        ));
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), 2);
        assert_eq!(
            offers(&ctx),
            [(format!("{{card {PROTECTOR}}}: buff me (2 XP)"), true)]
        );
    }
}
