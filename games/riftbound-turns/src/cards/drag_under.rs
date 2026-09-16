use super::prelude::{a_unit_at_a_battlefield, card_target, done, kill, play, spell, with_statics};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Static};
use crate::engine::ctx::{Ctx, Killed};
use crate::state::Origin;

pub const DISCOUNT: u8 = 2;

pub fn played_from_elsewhere_than_the_hand(ctx: &Ctx, card: u32) -> bool {
    if let Some(pending) = ctx
        .blob
        .queue
        .iter()
        .find(|pending| pending.item.kind.card() == Some(card))
    {
        return pending.item.origin != Origin::Hand;
    }
    if let Some(held) = ctx
        .blob
        .chain
        .iter()
        .find(|held| held.kind.card() == Some(card))
    {
        return held.origin != Origin::Hand;
    }
    match ctx.card(card).and_then(|held| held.zone) {
        Some(zone) => ctx.zones.hand != Some(zone),
        None => false,
    }
}

fn discount(ctx: &Ctx, card: u32, _: u8) -> Cost {
    Cost {
        energy: if played_from_elsewhere_than_the_hand(ctx, card) {
            DISCOUNT
        } else {
            0
        },
        power: &[],
    }
}

fn drag(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if kill(ctx, item, unit) == Killed::Yes {
            ctx.narrate(format!("{{card {unit}}} is dragged under"));
        }
    }
    done()
}

pub static CARD: Card = with_statics(
    spell(
        "Drag Under",
        &[Keyword::Action],
        &[play(
            &[a_unit_at_a_battlefield("a unit at a battlefield")],
            drag,
        )],
    ),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{cost, play as play_engine, priority, settle};
    use crate::state::{ChainItem, ItemKind, Leave, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const DRAG: u32 = 90;
    const THEIR_DRAG: u32 = 91;
    const ORDER_RUNE: u32 = 100;
    const EXTRA_RUNE: u32 = 101;

    fn drag(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Drag Under", 5, 1);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(drag(DRAG, 0));
        fixture.table.cards.push(drag(THEIR_DRAG, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(EXTRA_RUNE, 0, "Order", false));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn item_from(origin: Origin) -> ChainItem {
        ChainItem::new(7, ItemKind::Spell { card: DRAG }, 0, origin)
    }

    #[test]
    fn the_script_is_an_action_over_a_unit_at_a_battlefield_with_a_self_discount() {
        assert!(std::ptr::eq(script_of("Drag Under").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
    }

    #[test]
    fn from_the_hand_it_costs_the_printed_five_and_from_anywhere_else_two_less() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert!(!played_from_elsewhere_than_the_hand(&ctx, DRAG));
        assert_eq!(cost::total(&ctx, DRAG, false).energy, 5);
        assert_eq!(
            cost::of_item(&ctx, &item_from(Origin::Hand), None).energy,
            5
        );
        let flow = item_from(Origin::Trash {
            leave: Leave::Banish,
        });
        assert_eq!(
            cost::of_item(&ctx, &flow, None).energy,
            5,
            "the item alone does not tell the discount where the card is played from"
        );
        drop(ctx);
        let mut fixture = armed();
        fixture.table.card_mut(DRAG).unwrap().zone = Some(fixtures::TRASH);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(played_from_elsewhere_than_the_hand(&ctx, DRAG));
        assert_eq!(
            cost::total(&ctx, DRAG, false).energy,
            3,
            "priced where it lies, before an item exists"
        );
        let flow = item_from(Origin::Trash {
            leave: Leave::Banish,
        });
        assert_eq!(cost::of_item(&ctx, &flow, None).energy, 3);
        assert_eq!(
            cost::of_item(&ctx, &flow, None).power.len(),
            1,
            "the Order power is untouched"
        );
    }

    #[test]
    fn a_queued_play_reads_its_origin_rather_than_the_chain_zone() {
        let mut fixture = armed();
        fixture.table.card_mut(DRAG).unwrap().zone = Some(fixtures::TRASH);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(DRAG, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            DRAG,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert!(played_from_elsewhere_than_the_hand(&ctx, DRAG));
        let pending = ctx.blob.pending(1).unwrap().item.clone();
        assert_eq!(cost::of_item(&ctx, &pending, None).energy, 3);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 3,
            "three energy paid, two off the printed five"
        );
    }

    #[test]
    fn from_the_hand_the_full_five_is_paid_and_the_unit_at_the_battlefield_dies() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DRAG).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "Vi in the base is out of reach"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 5);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} is dragged under".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(DRAG).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn units_in_a_base_are_refused_and_the_action_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DRAG)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, DRAG).unwrap();
        for wrong in [fixtures::VI, fixtures::GROUNDS, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit at a battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(DRAG).unwrap().zone, Some(fixtures::HAND));
        drop(ctx);
        let mut poor = armed();
        poor.table.cards.retain(|card| card.id != EXTRA_RUNE);
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, DRAG)),
            Err(Refusal::NotEnoughRunes {
                needed: 5,
                ready: 4
            }),
            "four ready runes cannot pay five from the hand"
        );
    }
}
