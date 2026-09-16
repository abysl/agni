use super::prelude::{a_unit, card_target, done, play, ready, spell};
use super::sett_brawler::buffed_friendly_units;
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const IGNORED_COST: Cost = Cost::FREE;

pub fn buffs_that_could_pay(ctx: &Ctx, seat: u8) -> Vec<u32> {
    buffed_friendly_units(ctx, seat)
}

fn wallop(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if ready(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is readied"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Wallop",
    &[Keyword::Action],
    &[play(&[a_unit("a unit to ready")], wallop)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::{cost, pay};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const WALLOP: u32 = 90;
    const TIRED: u32 = 91;

    fn wallop_card() -> CardInfo {
        let mut card = fixtures::spell(WALLOP, fixtures::HAND, 0, "Wallop", 2, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn ring() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(wallop_card());
        let mut tired = fixtures::unit(TIRED, fixtures::BF1, 0, "Pit Rookie", 2);
        tired.exhausted = true;
        fixture.table.cards.push(tired);
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_an_action_that_readies_any_one_unit() {
        assert!(std::ptr::eq(script_of("Wallop").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(IGNORED_COST, Cost::FREE);
    }

    #[test]
    fn paid_in_full_it_readies_the_chosen_unit_of_either_side() {
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WALLOP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "{card 91}", "cancel"],
            "any unit on the board"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {TIRED}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(TIRED)]);
        assert!(
            ctx.card(TIRED).unwrap().exhausted,
            "nothing until it resolves"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "2 energy paid from the four ready runes"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(TIRED).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == TIRED
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TIRED}}} is readied")));
        assert_eq!(ctx.card(WALLOP).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WALLOP).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            !ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "an enemy unit is a unit"
        );
    }

    #[test]
    fn a_ready_unit_is_a_legal_choice_that_readies_nothing_and_a_gone_target_is_skipped() {
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WALLOP).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { .. })));
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        drop(ctx);
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WALLOP).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TIRED}}}")).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(TIRED, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Readied { .. })),
            "356.3.e · gone, untouched"
        );
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn a_card_in_hand_is_refused_as_a_target_and_the_play_can_be_taken_back() {
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WALLOP).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.effects.is_empty(), "nothing was paid");
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(WALLOP).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn buffs_that_could_pay_lists_the_seats_buffed_units() {
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        assert!(buffs_that_could_pay(&ctx, 0).is_empty());
        ctx.buff(TIRED);
        ctx.buff(fixtures::THEIR_UNIT);
        assert_eq!(buffs_that_could_pay(&ctx, 0), [TIRED]);
        assert_eq!(buffs_that_could_pay(&ctx, 1), [fixtures::THEIR_UNIT]);
    }

    #[test]
    #[ignore = "engine gap · spend a buff: an additional-cost kind that spends a friendly buff at the pay stage and ignores the printed cost; with no ready rune and a buffed unit, Wallop must be playable for the buff alone"]
    fn with_no_rune_ready_a_buffed_unit_pays_for_the_wallop_instead() {
        let mut fixture = ring();
        for id in [fixtures::RUNE_A, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        ctx.buff(fixtures::VI);
        let item = ChainItem::new(1, ItemKind::Spell { card: WALLOP }, 0, Origin::Hand);
        assert!(!pay::affordable(&ctx, 0, &cost::of_item(&ctx, &item, None)));
        fixtures::play_from_hand(&mut ctx, 0, WALLOP).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TIRED}}}")).unwrap();
        assert!(!ctx.is_buffed(fixtures::VI), "the buff is the whole price");
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(TIRED).unwrap().exhausted);
    }
}
