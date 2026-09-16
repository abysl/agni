use super::prelude::{unit, with_statics, Location};
use super::{Card, Cost, Domain, Power, Static};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, ItemKind, Origin};

pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[Power::Domain(Domain::Chaos)],
};

pub static CARD: Card = with_statics(
    unit("Syndra - Transcendent", &[], &[]),
    &[Static::GrantsRepeat(your_spells_repeat)],
);

pub fn in_a_showdown(ctx: &Ctx, unit: u32) -> bool {
    let Some(showdown) = ctx.blob.showdown.as_ref() else {
        return false;
    };
    ctx.location(unit) == Some(Location::Battlefield(showdown.zone))
}

fn your_spells_repeat(ctx: &Ctx, item: &ChainItem, syndra: u32) -> Option<Cost> {
    let ItemKind::Spell { card } = item.kind else {
        return None;
    };
    (ctx.is_spell(card) && item.controller == ctx.controller(syndra) && in_a_showdown(ctx, syndra))
        .then_some(REPEAT)
}

pub fn grants_repeat(ctx: &Ctx, seat: u8, spell: u32) -> Option<Cost> {
    let item = ChainItem::new(0, ItemKind::Spell { card: spell }, seat, Origin::Hand);
    cost::granted_repeat(ctx, &item).map(|_| REPEAT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::spell;
    use crate::cards::{script_of, Keyword};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_REPEAT;
    use crate::engine::{cleanup, priority, showdown};
    use crate::state::PromptWhy;
    use agni_plugin_sdk::table::CardInfo;

    const SYNDRA: u32 = 90;
    const CHAOS_RUNE: u32 = 46;

    static QUICK_SPARK: Card = spell("Spark", &[Keyword::Action], &[]);

    fn syndra(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(SYNDRA, zone, seat, "Syndra - Transcendent", 6)
        }
    }

    fn contested(zone: u16, seat: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(syndra(zone, seat));
        if zone != fixtures::BF1 || seat != 0 {
            fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        }
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SYNDRA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn open_showdown(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        crate::engine::settle(ctx).unwrap();
        assert_eq!(
            ctx.blob.showdown.as_ref().map(|held| held.zone),
            Some(fixtures::BF1),
            "the contested battlefield opens a showdown"
        );
    }

    fn close_showdown(ctx: &mut Ctx) {
        for _ in 0..6 {
            let Some(held) = ctx.blob.showdown.clone() else {
                return;
            };
            assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
            if !ctx.blob.chain.is_empty() {
                let holder = priority::holder(ctx).unwrap();
                priority::pass(ctx, holder).unwrap();
                continue;
            }
            showdown::pass(ctx, held.focus()).unwrap();
        }
    }

    #[test]
    fn the_script_is_the_pool_name_and_grants_the_repeat_through_a_static() {
        assert!(std::ptr::eq(
            script_of("Syndra - Transcendent").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(CARD.statics[0], Static::GrantsRepeat(_)));
        assert!(CARD.replacement.is_none());
        assert_eq!(REPEAT.energy, 2);
        assert_eq!(REPEAT.power, &[Power::Domain(Domain::Chaos)]);
    }

    #[test]
    fn she_is_in_a_showdown_only_while_one_is_open_at_her_battlefield() {
        let mut fixture = contested(fixtures::BF1, 0);
        let mut ctx = fixture.ctx();
        assert!(!in_a_showdown(&ctx, SYNDRA), "nothing is open yet");
        assert_eq!(grants_repeat(&ctx, 0, fixtures::HAND_SPELL), None);
        open_showdown(&mut ctx);
        assert!(in_a_showdown(&ctx, SYNDRA));
        assert!(!in_a_showdown(&ctx, fixtures::VI), "Vi stands in the base");
        assert!(
            !in_a_showdown(&ctx, fixtures::SPRITE),
            "the Sprite stands at another battlefield"
        );
        assert_eq!(grants_repeat(&ctx, 0, fixtures::HAND_SPELL), Some(REPEAT));
        assert_eq!(
            grants_repeat(&ctx, 1, fixtures::HAND_SPELL),
            None,
            "your spells · the opponent's spells get nothing"
        );
        assert_eq!(
            grants_repeat(&ctx, 0, fixtures::HAND_UNIT),
            None,
            "a unit is not a spell"
        );
        close_showdown(&mut ctx);
        assert!(ctx.blob.showdown.is_none(), "the showdown closed");
        assert!(!in_a_showdown(&ctx, SYNDRA));
        assert_eq!(grants_repeat(&ctx, 0, fixtures::HAND_SPELL), None);
    }

    #[test]
    fn a_syndra_elsewhere_or_an_enemy_syndra_grants_nothing_to_seat_zero() {
        let mut home = contested(fixtures::BASE, 0);
        let mut ctx = home.ctx();
        open_showdown(&mut ctx);
        assert!(!in_a_showdown(&ctx, SYNDRA));
        assert_eq!(grants_repeat(&ctx, 0, fixtures::HAND_SPELL), None);
        drop(ctx);

        let mut theirs = contested(fixtures::BF1, 1);
        let mut ctx = theirs.ctx();
        open_showdown(&mut ctx);
        assert!(in_a_showdown(&ctx, SYNDRA));
        assert_eq!(grants_repeat(&ctx, 0, fixtures::HAND_SPELL), None);
        assert_eq!(
            grants_repeat(&ctx, 1, fixtures::THEIR_HAND_CARD),
            None,
            "a hidden hand card has no kind to read"
        );
        assert_eq!(
            ctx.location(SYNDRA),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }

    #[test]
    fn a_spell_played_while_she_is_in_a_showdown_is_offered_the_repeat_and_resolves_twice() {
        let mut fixture = contested(fixtures::BF1, 0);
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &QUICK_SPARK);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let item = ctx.blob.chain.last().map(|held| held.id).unwrap_or(1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item,
                cost: SLOT_REPEAT as u8
            }),
            "your spells have [Repeat] 2 energy and Chaos"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.chain.last().unwrap().repeated());
        while !ctx.blob.chain.is_empty() {
            let holder = priority::holder(&ctx).unwrap();
            priority::pass(&mut ctx, holder).unwrap();
        }
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} repeats", fixtures::HAND_SPELL)));
    }
}
