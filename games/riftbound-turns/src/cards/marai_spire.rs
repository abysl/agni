use super::prelude::{battlefield, with_statics, Location};
use super::{Card, Cost, Keyword, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;
use crate::state::{ChainItem, ItemKind};

pub const REDUCTION: u8 = 1;

pub fn repeat_cost_of(ctx: &Ctx, card: u32) -> Option<Cost> {
    ctx.script(card)?
        .keywords
        .iter()
        .find_map(|held| match held {
            Keyword::Repeat(cost) => Some(*cost),
            _ => None,
        })
}

pub fn controls_it(ctx: &Ctx, spire: u32, seat: u8) -> bool {
    statics::in_play(ctx, spire)
        && matches!(ctx.location(spire), Some(Location::Battlefield(zone)) if ctx.holds(seat, zone))
}

pub fn repeating_under_the_spire(ctx: &Ctx, item: &ChainItem, spire: u32) -> bool {
    let ItemKind::Spell { card } = item.kind else {
        return false;
    };
    item.repeated()
        && repeat_cost_of(ctx, card).is_some()
        && controls_it(ctx, spire, item.controller)
}

fn spires_before(ctx: &Ctx, item: &ChainItem, me: u32) -> u8 {
    let earlier = ctx
        .table
        .cards
        .iter()
        .filter(|held| held.id < me)
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| repeating_under_the_spire(ctx, item, held.id))
        .count();
    u8::try_from(earlier).unwrap_or(u8::MAX)
}

fn repeat_discount(ctx: &Ctx, item: &ChainItem, spire: u32) -> Cost {
    if !repeating_under_the_spire(ctx, item, spire) {
        return Cost::FREE;
    }
    let repeat = repeat_cost_of(ctx, item.kind.source())
        .map(|cost| cost.energy)
        .unwrap_or(0);
    let room = repeat.saturating_sub(spires_before(ctx, item, spire).saturating_mul(REDUCTION));
    Cost {
        energy: room.min(REDUCTION),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    battlefield("Marai Spire", &[], &[]),
    &[Static::PlayDiscount(repeat_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Domain};
    use crate::engine::cost::{self, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_REPEAT;
    use crate::engine::{legal, priority};
    use crate::state::{Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SPIRE: u32 = fixtures::GROUNDS;
    const SECOND_SPIRE: u32 = fixtures::ROCKFALL;
    const RUSH: u32 = 90;
    const BELLOWS: u32 = 91;

    fn rush() -> CardInfo {
        fixtures::spell(RUSH, fixtures::HAND, 0, "Blood Rush", 1, 1)
    }

    fn spire_held_by(seat: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(SPIRE).unwrap().name = "Marai Spire".into();
        fixture.table.cards.push(rush());
        let mut bellows = fixtures::spell(BELLOWS, fixtures::HAND, 0, "Bellows Breath", 1, 1);
        bellows.domain = vec!["Mind".into()];
        fixture.table.cards.push(bellows);
        fixture.blob.set_holder(fixtures::BF1, seat);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SPIRE).unwrap(), &CARD));
        fixture
    }

    fn play_of(card: u32, seat: u8, repeated: bool) -> ChainItem {
        let mut item = ChainItem::new(1, ItemKind::Spell { card }, seat, Origin::Hand);
        if repeated {
            item.set_slot(SLOT_REPEAT, 1);
        }
        item
    }

    fn entry(ctx: &Ctx, card: u32) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_spire_is_a_battlefield_carrying_one_spell_discount_and_no_abilities() {
        assert!(std::ptr::eq(script_of("Marai Spire").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::PlayDiscount(repeat_discount)));
        assert_eq!(REDUCTION, 1);
    }

    #[test]
    fn the_repeat_cost_is_read_off_the_spells_script_and_control_off_the_holder() {
        let mut fixture = spire_held_by(Some(0));
        let ctx = fixture.ctx();
        assert_eq!(
            repeat_cost_of(&ctx, RUSH),
            Some(crate::cards::blood_rush::REPEAT)
        );
        assert_eq!(
            repeat_cost_of(&ctx, BELLOWS),
            Some(crate::cards::bellows_breath::REPEAT)
        );
        assert_eq!(
            repeat_cost_of(&ctx, fixtures::HAND_SPELL),
            None,
            "Spark has no Repeat"
        );
        assert!(controls_it(&ctx, SPIRE, 0));
        assert!(!controls_it(&ctx, SPIRE, 1));
        assert!(
            controls_it(&ctx, fixtures::ROCKFALL, 1),
            "control is read off the holder of the battlefield's zone"
        );
        assert!(repeating_under_the_spire(
            &ctx,
            &play_of(RUSH, 0, true),
            SPIRE
        ));
        assert!(
            !repeating_under_the_spire(&ctx, &play_of(RUSH, 0, false), SPIRE),
            "a spell played once pays no Repeat cost to discount"
        );
        assert!(!repeating_under_the_spire(
            &ctx,
            &play_of(RUSH, 1, true),
            SPIRE
        ));
        assert!(!repeating_under_the_spire(
            &ctx,
            &play_of(fixtures::HAND_SPELL, 0, true),
            SPIRE
        ));
    }

    #[test]
    fn while_you_hold_it_a_repeated_spell_costs_one_energy_less_and_only_the_repeat_energy_is_touched(
    ) {
        let mut fixture = spire_held_by(Some(0));
        let ctx = fixture.ctx();
        let once = cost::of_item(&ctx, &play_of(RUSH, 0, false), None);
        assert_eq!((once.energy, once.power.len()), (1, 1));
        let twice = cost::of_item(&ctx, &play_of(RUSH, 0, true), None);
        assert_eq!(
            twice.energy, 1,
            "one printed and one Repeat energy, less one"
        );
        assert_eq!(twice.power, [Need::Domain(Domain::Fury)]);
        let bellows = cost::of_item(&ctx, &play_of(BELLOWS, 0, true), None);
        assert_eq!(
            (bellows.energy, bellows.power.len()),
            (1, 2),
            "Bellows Breath's Repeat is one energy and one Mind: the Mind stays"
        );
        assert_eq!(
            cost::of_item(&ctx, &play_of(RUSH, 1, true), None).energy,
            2,
            "friendly Repeat costs, not the opponent's"
        );
        drop(ctx);
        let mut theirs = spire_held_by(Some(1));
        let ctx = theirs.ctx();
        assert_eq!(cost::of_item(&ctx, &play_of(RUSH, 0, true), None).energy, 2);
        assert_eq!(
            cost::of_item(&ctx, &play_of(RUSH, 1, true), None).energy,
            1,
            "the holder is whoever controls it"
        );
        drop(ctx);
        let mut nobody = spire_held_by(None);
        let ctx = nobody.ctx();
        assert_eq!(cost::of_item(&ctx, &play_of(RUSH, 0, true), None).energy, 2);
    }

    #[test]
    fn two_spires_held_by_one_seat_never_take_more_than_the_repeat_energy() {
        let mut fixture = spire_held_by(Some(0));
        fixture.table.card_mut(SECOND_SPIRE).unwrap().name = "Marai Spire".into();
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        fixture.resolve();
        let ctx = fixture.ctx();
        let repeated = play_of(RUSH, 0, true);
        assert_eq!(repeat_discount(&ctx, &repeated, SPIRE).energy, 1);
        assert_eq!(
            repeat_discount(&ctx, &repeated, SECOND_SPIRE).energy,
            0,
            "the first Spire already took the whole one-energy Repeat"
        );
        assert_eq!(cost::of_item(&ctx, &repeated, None).energy, 1);
    }

    #[test]
    fn the_discount_is_paid_through_the_engine_when_the_repeat_is_taken() {
        let mut fixture = spire_held_by(Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "one energy for the repeated play: one rune exhausted, the Fury power recycled off the spent one"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!("{{card {RUSH}}} repeats")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        drop(ctx);
        let mut unheld = spire_held_by(Some(1));
        let mut ctx = unheld.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "without the Spire the repeat takes two energy"
        );
    }

    #[test]
    fn a_spell_played_once_pays_full_price_and_one_ready_rune_is_refused_for_the_undiscounted_repeat(
    ) {
        let mut fixture = spire_held_by(Some(0));
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.blob.chain[0].repeated());
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "one energy, no discount");
        drop(ctx);
        let mut short = spire_held_by(Some(1));
        for rune in [42, 43] {
            short.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = short.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        legal::classify(&ctx, 0, &entry(&ctx, RUSH)).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "the Repeat is skipped, not asked: {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.blob.chain[0].repeated());
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        drop(ctx);
        let mut broke = spire_held_by(Some(0));
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = broke.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, RUSH)),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            }),
            "the Spire discounts the Repeat, never the printed energy"
        );
    }

    #[test]
    fn one_ready_rune_is_offered_the_repeat_when_the_spire_pays_its_energy() {
        let mut fixture = spire_held_by(Some(0));
        for rune in [42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(cost::of_item(&ctx, &play_of(RUSH, 0, true), None).energy, 1);
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
    }
}
