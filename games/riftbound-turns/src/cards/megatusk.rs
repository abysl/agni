use super::prelude::{
    activated, done, grant_this_turn, location_of, named, paying_with, spending_xp, unit,
};
use super::{Card, Cost, Flow, Item, Keyword, SelfCost, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const RALLY_XP: u8 = 3;

pub fn friendly_units_here(ctx: &Ctx, me: u32) -> Vec<u32> {
    let Some(here) = location_of(ctx, me) else {
        return Vec::new();
    };
    let seat = ctx.controller(me);
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) == seat)
        .collect()
}

fn stampede(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let herd = friendly_units_here(ctx, me);
    if herd.is_empty() {
        ctx.narrate(format!("{{card {me}}} has no here · no Ganking"));
        return done();
    }
    for unit in herd {
        grant_this_turn(ctx, unit, Keyword::Ganking);
        ctx.narrate(format!("{{card {unit}}} gains Ganking this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Megatusk",
    &[],
    &[named(
        spending_xp(
            paying_with(
                activated(Timing::Sorcery, Cost::FREE, &[], stampede),
                SelfCost::Free,
            ),
            RALLY_XP,
        ),
        "give your units here Ganking this turn",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, march, settle};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MEGATUSK: u32 = 90;
    const CALF: u32 = 91;
    const STRAGGLER: u32 = 92;
    const RIVAL: u32 = 93;
    const PLAIN: u32 = 54;

    fn megatusk(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(MEGATUSK, zone, 0, "Megatusk", 6);
        card.energy = Some(6);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn tundra(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(megatusk(fixtures::BF1));
        fixture
            .table
            .cards
            .push(fixtures::unit(CALF, fixtures::BF1, 0, "Calf", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(RIVAL, fixtures::BF1, 1, "Rival", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(STRAGGLER, fixtures::BASE, 0, "Straggler", 1));
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MEGATUSK).unwrap(),
            &CARD
        ));
        fixture
    }

    fn offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == MEGATUSK)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn can_gank(ctx: &Ctx, unit: u32) -> Result<(), Refusal> {
        march::legal_destination(
            ctx,
            unit,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF3),
        )
    }

    #[test]
    fn the_script_is_a_keywordless_unit_with_one_three_xp_focus_ability_that_exhausts_nothing() {
        assert!(std::ptr::eq(script_of("Megatusk").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.xp, RALLY_XP);
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.self_cost, SelfCost::Free, "no exhaust is printed");
        assert!(ability.targets.is_empty());
        assert_eq!(
            ability.label,
            Some("give your units here Ganking this turn")
        );
        let mut fixture = tundra(3);
        let ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, MEGATUSK, 0).label(), "3 XP");
        assert_eq!(
            friendly_units_here(&ctx, MEGATUSK),
            [MEGATUSK, CALF],
            "the Rival is the other seat's and the Straggler is at the base"
        );
        assert_eq!(
            friendly_units_here(&ctx, STRAGGLER),
            [fixtures::VI, STRAGGLER]
        );
    }

    #[test]
    fn the_offer_is_greyed_short_of_three_xp_and_a_direct_activation_is_refused() {
        let mut fixture = tundra(2);
        let mut ctx = fixture.ctx();
        assert_eq!(
            offers(&ctx),
            [(
                format!("{{card {MEGATUSK}}}: give your units here Ganking this turn (3 XP)"),
                false
            )]
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, MEGATUSK, 0),
            Err(Refusal::Illegal(Reason::NotEnoughXp))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, MEGATUSK, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.xp(0), 2);
        assert_eq!(
            can_gank(&ctx, CALF),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
    }

    #[test]
    fn three_xp_give_every_friendly_unit_here_ganking_until_the_turn_ends() {
        let mut fixture = tundra(3);
        let mut ctx = fixture.ctx();
        assert_eq!(
            can_gank(&ctx, CALF),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        activate::activate(&mut ctx, 0, MEGATUSK, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.xp(0), 0, "the XP lands with the cost");
        assert!(ctx.blob.log.contains(&"{seat 0} spends 3 XP".to_string()));
        assert!(
            !ctx.card(MEGATUSK).unwrap().exhausted,
            "no exhaust is part of the cost"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == MEGATUSK
        ));
        assert!(
            !ctx.has_keyword(CALF, Keyword::Ganking),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.has_keyword(MEGATUSK, Keyword::Ganking));
        assert!(ctx.has_keyword(CALF, Keyword::Ganking));
        assert!(
            !ctx.has_keyword(RIVAL, Keyword::Ganking),
            "the enemy unit here is not yours"
        );
        assert!(
            !ctx.has_keyword(STRAGGLER, Keyword::Ganking),
            "a friendly unit elsewhere is not here"
        );
        assert_eq!(can_gank(&ctx, CALF), Ok(()));
        assert_eq!(can_gank(&ctx, MEGATUSK), Ok(()));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CALF}}} gains Ganking this turn")));
        assert!(ctx.fault.is_none());
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert!(!ctx.has_keyword(CALF, Keyword::Ganking), "this turn only");
        assert!(!ctx.has_keyword(MEGATUSK, Keyword::Ganking));
        assert_eq!(
            can_gank(&ctx, CALF),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
    }

    #[test]
    fn a_megatusk_gone_before_resolution_has_no_here_and_grants_nothing() {
        let mut fixture = tundra(3);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, MEGATUSK, 0).unwrap();
        ctx.kill(MEGATUSK, Cause::Rule);
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(MEGATUSK));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 0, "the XP is spent all the same");
        assert!(!ctx.has_keyword(CALF, Keyword::Ganking));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {MEGATUSK}}} has no here · no Ganking")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn at_the_base_the_herd_is_the_units_in_the_base_and_ganking_is_moot_there() {
        let mut fixture = tundra(3);
        fixture.table.card_mut(MEGATUSK).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            friendly_units_here(&ctx, MEGATUSK),
            [fixtures::VI, MEGATUSK, STRAGGLER]
        );
        activate::activate(&mut ctx, 0, MEGATUSK, 0).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert!(ctx.has_keyword(STRAGGLER, Keyword::Ganking));
        assert!(!ctx.has_keyword(CALF, Keyword::Ganking));
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Base(0),
                Location::Battlefield(fixtures::BF3)
            ),
            Ok(()),
            "a march from the base never needed Ganking"
        );
    }
}
