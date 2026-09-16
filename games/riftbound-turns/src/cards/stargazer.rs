use super::prelude::{unit, with_statics};
use super::{Card, Cost, Static};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::engine::statics;
use crate::state::{ChainItem, ItemKind, Leave, Origin};

pub const REDUCTION: u8 = 2;
pub const MINIMUM: u8 = 1;

pub fn flowing_from_your_trash(ctx: &Ctx, item: &ChainItem, gazer: u32) -> bool {
    let ItemKind::Spell { card } = item.kind else {
        return false;
    };
    item.origin
        == Origin::Trash {
            leave: Leave::Banish,
        }
        && ctx
            .script(card)
            .is_some_and(|script| script.flow_cost().is_some())
        && statics::in_play(ctx, gazer)
        && ctx.controller(gazer) == item.controller
}

fn gazers_before(ctx: &Ctx, item: &ChainItem, me: u32) -> u8 {
    let earlier = ctx
        .table
        .cards
        .iter()
        .filter(|held| held.id < me)
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| flowing_from_your_trash(ctx, item, held.id))
        .count();
    u8::try_from(earlier).unwrap_or(u8::MAX)
}

pub fn flow_discount(ctx: &Ctx, item: &ChainItem, gazer: u32) -> Cost {
    if !flowing_from_your_trash(ctx, item, gazer) {
        return Cost::FREE;
    }
    let base = cost::base_of_item(ctx, item).energy;
    let room = base
        .saturating_sub(MINIMUM)
        .saturating_sub(gazers_before(ctx, item, gazer).saturating_mul(REDUCTION));
    Cost {
        energy: room.min(REDUCTION),
        power: &[],
    }
}

pub static CARD: Card = with_statics(
    unit("Stargazer", &[], &[]),
    &[Static::PlayDiscount(flow_discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::IMPLICIT_FLOW;
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent};
    use crate::engine::{activate, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const GAZER: u32 = 90;
    const SECOND_GAZER: u32 = 91;
    const THEIR_GAZER: u32 = 92;
    const ONSLAUGHT: u32 = 93;
    const LACERATE: u32 = 94;
    const SPARK: u32 = 95;
    const ONSLAUGHT_FLOW: u8 = 4;

    fn gazer(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: None,
            domain: vec!["Chaos".into()],
            ..fixtures::unit(id, zone, seat, "Stargazer", 4)
        }
    }

    fn observatory(gazer_at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(gazer(GAZER, gazer_at, 0));
        fixture
            .table
            .cards
            .push(gazer(THEIR_GAZER, fixtures::BASE, 1));
        let mut onslaught = fixtures::spell(ONSLAUGHT, fixtures::TRASH, 0, "Onslaught", 4, 0);
        onslaught.domain = vec!["Body".into()];
        fixture.table.cards.push(onslaught);
        let mut lacerate = fixtures::spell(LACERATE, fixtures::TRASH, 0, "Lacerate", 3, 1);
        lacerate.domain = vec!["Order".into()];
        fixture.table.cards.push(lacerate);
        fixture
            .table
            .cards
            .push(fixtures::spell(SPARK, fixtures::TRASH, 0, "Spark", 2, 1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GAZER).unwrap(), &CARD));
        fixture
    }

    fn flow_of(card: u32, seat: u8) -> ChainItem {
        activate::flow_item(seat, card)
    }

    fn hand_play_of(card: u32, seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Spell { card }, seat, Origin::Hand)
    }

    fn drag_from_trash(ctx: &Ctx, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: Some(fixtures::TRASH),
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_keywordless_unit_carrying_one_spell_discount() {
        assert!(std::ptr::eq(script_of("Stargazer").unwrap(), &CARD));
        assert_eq!(CARD.name, "Stargazer");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::PlayDiscount(flow_discount)));
        assert_eq!((REDUCTION, MINIMUM), (2, 1));
    }

    #[test]
    fn the_discount_reads_a_flow_spell_played_from_your_own_trash_while_the_gazer_is_in_play() {
        let mut fixture = observatory(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(flowing_from_your_trash(&ctx, &flow_of(ONSLAUGHT, 0), GAZER));
        assert!(flowing_from_your_trash(&ctx, &flow_of(LACERATE, 0), GAZER));
        assert!(
            !flowing_from_your_trash(&ctx, &flow_of(SPARK, 0), GAZER),
            "Spark has no Flow"
        );
        assert!(
            !flowing_from_your_trash(&ctx, &hand_play_of(ONSLAUGHT, 0), GAZER),
            "from the hand it is not a trash play"
        );
        assert!(
            !flowing_from_your_trash(&ctx, &flow_of(ONSLAUGHT, 1), GAZER),
            "your spells, not the opponent's"
        );
        assert!(!flowing_from_your_trash(
            &ctx,
            &flow_of(ONSLAUGHT, 0),
            THEIR_GAZER
        ));
        let recycled = ChainItem::new(
            1,
            ItemKind::Spell { card: ONSLAUGHT },
            0,
            Origin::Trash {
                leave: Leave::Recycle,
            },
        );
        assert!(
            !flowing_from_your_trash(&ctx, &recycled, GAZER),
            "a recycle play from the trash pays no Flow cost"
        );
        assert_eq!(
            flow_discount(&ctx, &flow_of(ONSLAUGHT, 0), GAZER).energy,
            REDUCTION
        );
        assert_eq!(
            flow_discount(&ctx, &flow_of(LACERATE, 0), GAZER).energy,
            REDUCTION,
            "Lacerate's Flow is four energy and two Order: the energy has room"
        );
        assert_eq!(flow_discount(&ctx, &flow_of(SPARK, 0), GAZER), Cost::FREE);
        assert_eq!(
            flow_discount(&ctx, &hand_play_of(ONSLAUGHT, 0), GAZER),
            Cost::FREE
        );
        drop(ctx);
        let mut away = observatory(fixtures::HAND);
        let ctx = away.ctx();
        assert!(!flowing_from_your_trash(
            &ctx,
            &flow_of(ONSLAUGHT, 0),
            GAZER
        ));
        assert_eq!(
            flow_discount(&ctx, &flow_of(ONSLAUGHT, 0), GAZER),
            Cost::FREE,
            "a Stargazer in the hand discounts nothing"
        );
    }

    #[test]
    fn a_flow_play_costs_two_less_down_to_one_and_a_second_gazer_takes_what_room_is_left() {
        let mut fixture = observatory(fixtures::BASE);
        let ctx = fixture.ctx();
        assert_eq!(
            activate::flow_cost(&ctx, 0, ONSLAUGHT).energy,
            ONSLAUGHT_FLOW - REDUCTION
        );
        let lacerate = activate::flow_cost(&ctx, 0, LACERATE);
        assert_eq!(
            (lacerate.energy, lacerate.power.len()),
            (2, 2),
            "the two Order stay"
        );
        assert_eq!(
            cost::of_item(&ctx, &hand_play_of(ONSLAUGHT, 0), None).energy,
            4,
            "from the hand Onslaught is its printed four"
        );
        assert_eq!(
            flow_discount(&ctx, &flow_of(ONSLAUGHT, 1), GAZER),
            Cost::FREE,
            "your Stargazer never prices the opponent's play"
        );
        assert_eq!(
            flow_discount(&ctx, &flow_of(ONSLAUGHT, 1), THEIR_GAZER).energy,
            REDUCTION,
            "their own Stargazer does"
        );
        assert_eq!(
            activate::flow_cost(&ctx, 1, ONSLAUGHT).energy,
            ONSLAUGHT_FLOW - REDUCTION
        );
        drop(ctx);
        let mut two = observatory(fixtures::BASE);
        two.table.cards.push(gazer(SECOND_GAZER, fixtures::BF1, 0));
        two.blob.set_holder(fixtures::BF1, Some(0));
        two.resolve();
        let ctx = two.ctx();
        assert_eq!(flow_discount(&ctx, &flow_of(ONSLAUGHT, 0), GAZER).energy, 2);
        assert_eq!(
            flow_discount(&ctx, &flow_of(ONSLAUGHT, 0), SECOND_GAZER).energy,
            1,
            "four less two is three: one more keeps the minimum of one"
        );
        assert_eq!(
            activate::flow_cost(&ctx, 0, ONSLAUGHT).energy,
            MINIMUM,
            "356.4.e · the floor is the discount's own"
        );
    }

    #[test]
    fn the_discount_is_paid_through_the_engine_when_the_flow_play_is_taken() {
        let mut fixture = observatory(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        let offer = activate::flow_offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == ONSLAUGHT)
            .expect("Onslaught is offered from the trash");
        assert_eq!(offer.index, IMPLICIT_FLOW);
        assert!(offer.enabled, "two energy off three ready runes");
        assert_eq!(
            offer.label,
            format!("{{card {ONSLAUGHT}}}: play from your trash (2 energy)")
        );
        activate::activate(&mut ctx, 0, ONSLAUGHT, IMPLICIT_FLOW).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "two energy for the Flow play: two runes exhausted"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.banished_of(0), [ONSLAUGHT]);
        assert_eq!(ctx.current_might(fixtures::VI), 3 + 6);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn without_a_gazer_in_play_the_flow_play_is_refused_off_three_runes() {
        let mut fixture = observatory(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::flow_cost(&ctx, 0, ONSLAUGHT).energy,
            ONSLAUGHT_FLOW
        );
        let offer = activate::flow_offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == ONSLAUGHT)
            .unwrap();
        assert!(!offer.enabled, "four energy off three ready runes");
        assert_eq!(
            activate::activate(&mut ctx, 0, ONSLAUGHT, IMPLICIT_FLOW),
            Err(Refusal::NotEnoughRunes {
                needed: ONSLAUGHT_FLOW,
                ready: 3
            })
        );
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.trash_of(0).len(), 3, "Onslaught stays in the trash");
    }

    #[test]
    fn the_drag_from_the_trash_is_classified_at_the_discounted_price() {
        let mut cheap = observatory(fixtures::BASE);
        cheap.table.card_mut(43).unwrap().exhausted = true;
        cheap.resolve();
        let ctx = cheap.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 2);
        assert_eq!(activate::flow_legal(&ctx, 0, ONSLAUGHT), Ok(()));
        assert_eq!(
            legal::classify(&ctx, 0, &drag_from_trash(&ctx, ONSLAUGHT)),
            Ok(Intent::Play {
                card: ONSLAUGHT,
                origin: Origin::Trash {
                    leave: Leave::Banish
                },
                location: None,
                on_chain: true,
            })
        );
    }
}
