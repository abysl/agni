use super::prelude::unit;
use super::prepared_neophyte::spent_four_or_more_on_a_spell_this_turn;
use super::{Card, Cost, Domain, Keyword, Power};
use crate::engine::cost;
use crate::engine::ctx::Ctx;

pub const ALTERNATIVE: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Mind)],
};

pub fn plays_for_a_mind_rune(ctx: &Ctx, seat: u8) -> bool {
    spent_four_or_more_on_a_spell_this_turn(ctx, seat)
}

pub fn alternative_cost(ctx: &Ctx, me: u32, seat: u8) -> Option<cost::Cost> {
    plays_for_a_mind_rune(ctx, seat).then(|| cost::of_script(&ALTERNATIVE, &ctx.domains_of(me)))
}

pub static CARD: Card = unit("Jhin - Meticulous Killer", &[Keyword::Vision], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::Need;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const JHIN: u32 = 90;
    const BIG_SPELL: u32 = 91;
    const ENERGY: u8 = 4;
    const MIGHT: u8 = 4;
    const MIND_RUNE: u32 = 46;
    const FURY_RUNES: [u32; 3] = [47, 48, 49];

    fn jhin(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Mind".into()],
            ..fixtures::unit(JHIN, zone, seat, "Jhin - Meticulous Killer", MIGHT)
        }
    }

    fn stage() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jhin(fixtures::HAND, 0));
        fixture.table.cards.push(fixtures::spell(
            BIG_SPELL,
            fixtures::HAND,
            0,
            "Cataclysm",
            4,
            0,
        ));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        for rune in FURY_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(JHIN).unwrap(), &CARD));
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: JHIN }, seat, Origin::Hand)
    }

    #[test]
    fn the_script_prints_vision_and_names_the_alternative_cost_seam() {
        assert!(std::ptr::eq(
            script_of("Jhin - Meticulous Killer").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Vision]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(ALTERNATIVE.energy, 0);
        assert_eq!(ALTERNATIVE.power, [Power::Domain(Domain::Mind)]);
    }

    #[test]
    fn the_seam_offers_one_mind_power_only_once_four_or_more_went_into_a_spell_this_request() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        assert!(!plays_for_a_mind_rune(&ctx, 0));
        assert_eq!(alternative_cost(&ctx, JHIN, 0), None);
        fixtures::play_from_hand(&mut ctx, 0, BIG_SPELL).unwrap();
        assert!(plays_for_a_mind_rune(&ctx, 0));
        assert!(
            !plays_for_a_mind_rune(&ctx, 1),
            "the other seat spent nothing on spells"
        );
        let mind = alternative_cost(&ctx, JHIN, 0).expect("the alternative is open");
        assert_eq!(mind.energy, 0);
        assert_eq!(mind.power, [Need::Domain(Domain::Mind)]);
        assert_eq!(
            cost::of_item(&ctx, &item(0), None).energy,
            ENERGY,
            "the printed price stands until the engine offers the alternative"
        );
    }

    #[test]
    fn played_at_his_printed_four_he_lands_exhausted_and_short_he_is_refused() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 8);
        fixtures::play_from_hand(&mut ctx, 0, JHIN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(JHIN), Some(Location::Base(0)));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            4,
            "four energy off eight runes"
        );
        assert!(ctx.card(JHIN).unwrap().exhausted);
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })),
            "817 · his Vision looks at the top card"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut short = stage();
        short
            .table
            .cards
            .retain(|card| !FURY_RUNES.contains(&card.id) && card.id != MIND_RUNE);
        for rune in [41, 42] {
            short.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = short.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, JHIN),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: 2
            })
        );
        assert!(ctx.blob.queue.is_empty());
    }

    #[test]
    #[ignore = "engine gap · an alternative cost (one Mind power instead of the printed four) has no primitive: Static::SelfDiscount can strip energy but not add a power need, and the spell-spending it reads is the Prepared Neophyte counter gap"]
    fn after_four_went_into_a_spell_this_turn_he_plays_for_one_mind_power_alone() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BIG_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let price = cost::of_item(&ctx, &item(0), None);
        assert_eq!(price.energy, 0);
        assert_eq!(price.power, [Need::Domain(Domain::Mind)]);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, JHIN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(JHIN), Some(Location::Base(0)));
        assert_eq!(
            ctx.card(MIND_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Mind rune recycles for the power"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1, "no energy is paid");
    }
}
