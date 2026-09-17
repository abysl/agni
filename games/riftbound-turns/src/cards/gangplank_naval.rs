use super::prelude::{empower, is_empowered, unit};
use super::{Card, Cost, Domain, Keyword, Power};
use crate::engine::ctx::Ctx;
use crate::engine::statics;
use crate::state::{ChainItem, Expiry, ItemKind, TargetRef};

pub const EMPOWER: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body), Power::Domain(Domain::Body)],
};
pub const BONUS: i16 = 3;
pub const EMPOWER_ABILITY: u8 = 0;

pub fn shrugs_off(ctx: &Ctx, item: &ChainItem, me: u32) -> bool {
    ctx.script(me)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
        && statics::in_play(ctx, me)
        && is_empowered(ctx, me)
        && matches!(
            item.kind,
            ItemKind::Spell { .. } | ItemKind::Ability { .. } | ItemKind::Trigger { .. }
        )
        && item.targets.contains(&TargetRef::Card(me))
}

pub fn plus_three_instead(ctx: &mut Ctx, item: &ChainItem, me: u32) -> bool {
    if !shrugs_off(ctx, item, me) {
        return false;
    }
    ctx.might(me, BONUS, Expiry::Permanent, None, item.id);
    ctx.narrate(format!(
        "{{card {me}}} shrugs it off · +{BONUS} Might instead"
    ));
    true
}

pub static CARD: Card = unit(
    "Gangplank, Naval",
    &[Keyword::Empower(EMPOWER)],
    &[empower(EMPOWER)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{bounce, stun};
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::Origin;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const GANGPLANK: u32 = 90;
    const THEIR_STUPEFY: u32 = 91;
    const BODY_RUNES: [u32; 2] = [46, 47];

    fn gangplank(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(6),
            domain: vec!["Body".into()],
            ..fixtures::unit(GANGPLANK, zone, 0, "Gangplank, Naval", 6)
        }
    }

    fn harbour() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(gangplank(fixtures::BF1));
        let mut stupefy = fixtures::spell(THEIR_STUPEFY, fixtures::HAND, 1, "Stupefy", 1, 0);
        stupefy.domain = vec!["Mind".into()];
        fixture.table.cards.push(stupefy);
        for rune in BODY_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GANGPLANK).unwrap(),
            &CARD
        ));
        fixture
    }

    fn his_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == GANGPLANK)
            .collect()
    }

    fn empower_him(ctx: &mut Ctx) {
        activate::activate(ctx, 0, GANGPLANK, EMPOWER_ABILITY).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(GANGPLANK));
    }

    fn their_spell(targets: &[TargetRef]) -> ChainItem {
        let mut item = ChainItem::new(
            7,
            ItemKind::Spell {
                card: THEIR_STUPEFY,
            },
            1,
            Origin::Hand,
        );
        item.targets = targets.to_vec();
        item
    }

    #[test]
    fn the_champion_prints_a_two_body_empower_and_his_replacement_is_a_named_seam() {
        assert!(std::ptr::eq(script_of("Gangplank, Naval").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.replacement.is_none(),
            "Replacement is the kill path alone; stun, -Might and bounce have no hook"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[usize::from(EMPOWER_ABILITY)];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        assert_eq!(BONUS, 3);
    }

    #[test]
    fn empower_recycles_two_body_runes_and_is_refused_after_and_without_them() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        let offers = his_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {GANGPLANK}}}: empower (2 Body power)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, GANGPLANK, EMPOWER_ABILITY).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        for rune in BODY_RUNES {
            assert_ne!(
                ctx.card(rune).unwrap().zone,
                Some(fixtures::RUNE_POOL),
                "each Body rune is recycled for its power"
            );
        }
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "no energy: the Fury and Calm runes stay ready"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(is_empowered(&ctx, GANGPLANK));
        assert!(his_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, GANGPLANK, EMPOWER_ABILITY),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut short = harbour();
        short.table.cards.retain(|card| card.id != BODY_RUNES[1]);
        short.resolve();
        let mut ctx = short.ctx();
        assert!(!his_offers(&ctx)[0].enabled);
        assert_eq!(
            activate::activate(&mut ctx, 0, GANGPLANK, EMPOWER_ABILITY),
            Err(Refusal::NoPowerOf),
            "one Body rune cannot pay two Body power"
        );
        assert!(!ctx.is_empowered(GANGPLANK));
        drop(ctx);
        let mut spent = harbour();
        spent.table.card_mut(BODY_RUNES[1]).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert!(
            his_offers(&ctx)[0].enabled,
            "an exhausted rune is still recycled for its power"
        );
        activate::activate(&mut ctx, 0, GANGPLANK, EMPOWER_ABILITY).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert_ne!(
            ctx.card(BODY_RUNES[1]).unwrap().zone,
            Some(fixtures::RUNE_POOL)
        );
    }

    #[test]
    fn the_seam_reads_an_item_that_chose_him_only_while_he_is_empowered() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        let chose_him = their_spell(&[TargetRef::Card(GANGPLANK)]);
        let chose_vi = their_spell(&[TargetRef::Card(fixtures::VI)]);
        assert!(
            !shrugs_off(&ctx, &chose_him, GANGPLANK),
            "not empowered yet"
        );
        assert!(!plus_three_instead(&mut ctx, &chose_him, GANGPLANK));
        assert_eq!(ctx.current_might(GANGPLANK), 6);
        empower_him(&mut ctx);
        assert!(shrugs_off(&ctx, &chose_him, GANGPLANK));
        assert!(
            !shrugs_off(&ctx, &chose_vi, GANGPLANK),
            "an item that chose another unit"
        );
        assert!(
            !shrugs_off(&ctx, &chose_him, fixtures::VI),
            "Vi is not Gangplank"
        );
        let mut mine = chose_him.clone();
        mine.controller = 0;
        assert!(
            shrugs_off(&ctx, &mine, GANGPLANK),
            "his own controller's items are replaced too"
        );
        assert!(plus_three_instead(&mut ctx, &chose_him, GANGPLANK));
        assert_eq!(ctx.current_might(GANGPLANK), 6 + i32::from(BONUS));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| *line
                == format!("{{card {GANGPLANK}}} shrugs it off · +{BONUS} Might instead")));
        assert!(plus_three_instead(&mut ctx, &chose_him, GANGPLANK));
        assert_eq!(
            ctx.current_might(GANGPLANK),
            6 + 2 * i32::from(BONUS),
            "477.3.b · each replacement is its own permanent +3"
        );
        ctx.disempower(GANGPLANK);
        assert!(!shrugs_off(&ctx, &chose_him, GANGPLANK));
        assert_eq!(
            ctx.current_might(GANGPLANK),
            12,
            "the Might already given stays"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn today_a_stun_a_minus_might_and_a_bounce_land_on_him_empowered_or_not() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        empower_him(&mut ctx);
        let chose_him = their_spell(&[TargetRef::Card(GANGPLANK)]);
        assert!(shrugs_off(&ctx, &chose_him, GANGPLANK));
        assert!(
            stun(&mut ctx, GANGPLANK),
            "Ctx::stun consults no replacement"
        );
        assert!(ctx.is_stunned(GANGPLANK));
        crate::cards::prelude::might_this_turn(&mut ctx, &chose_him, GANGPLANK, -2, None);
        assert_eq!(ctx.current_might(GANGPLANK), 4);
        assert!(bounce(&mut ctx, GANGPLANK));
        assert!(!ctx.on_board(GANGPLANK));
    }

    #[test]
    #[ignore = "engine gap · game-rule statics: a replacement on the stun, -Might and bounce paths (today only the kill path has one); the engine owes Ctx::stun, Ctx::might and Ctx::bounce, when run for a chain item, consulting gangplank_naval::shrugs_off and running plus_three_instead in place of the effect, so the opponent's Stupefy leaves him unstunned, unshrunk and at 9 Might"]
    fn while_empowered_the_opponents_stupefy_gives_him_three_might_instead_of_the_minus_one() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        empower_him(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_STUPEFY).unwrap();
        fixtures::choose(&mut ctx, 1, &format!("{{card {GANGPLANK}}}")).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.current_might(GANGPLANK), 6 + i32::from(BONUS));
        assert!(!ctx.is_stunned(GANGPLANK));
        assert!(ctx.on_board(GANGPLANK));
        fixtures::pass_until_open(&mut ctx);
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(
            ctx.current_might(GANGPLANK),
            6 + i32::from(BONUS),
            "the +3 has no duration"
        );
    }
}
