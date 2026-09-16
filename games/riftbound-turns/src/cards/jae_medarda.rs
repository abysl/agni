use super::prelude::{done, draw, on_chosen, unit, when};
use super::{Card, Event, Flow, Item, Source, Stage};
use crate::engine::ctx::Ctx;
use crate::state::ItemKind;

pub const CARDS: usize = 1;

pub fn chosen_by_your_spell(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Chosen { card, by, item } = event else {
        return false;
    };
    *card == source.card
        && *by == ctx.controller(source.card)
        && ctx
            .chain_item(*item)
            .is_some_and(|held| matches!(held.kind, ItemKind::Spell { .. }))
}

fn patronage(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, CARDS);
    done()
}

pub static CARD: Card = unit(
    "Jae Medarda",
    &[],
    &[when(on_chosen(&[], patronage), chosen_by_your_spell)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::{ChainItem, ItemStatus, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const JAE: u32 = 90;
    const THEIR_SPELL: u32 = 91;

    fn jae() -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(2),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(JAE, fixtures::BASE, 0, "Jae Medarda", 5)
        }
    }

    fn discipline(id: u32, seat: u8) -> CardInfo {
        let mut spell = fixtures::spell(id, fixtures::HAND, seat, "Discipline", 2, 0);
        spell.domain = vec!["Calm".into()];
        spell
    }

    fn court() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jae());
        let spell = fixture
            .table
            .cards
            .iter()
            .position(|card| card.id == fixtures::HAND_SPELL)
            .unwrap();
        fixture.table.cards[spell] = discipline(fixtures::HAND_SPELL, 0);
        fixture.table.cards.push(discipline(THEIR_SPELL, 1));
        fixture.resolve();
        fixture
    }

    fn cast(ctx: &mut Ctx, seat: u8, spell: u32) {
        fixtures::play_from_hand(ctx, seat, spell).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_conditional_chosen_trigger() {
        assert!(std::ptr::eq(script_of("Jae Medarda").unwrap(), &CARD));
        assert_eq!(CARD.name, "Jae Medarda");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let chosen = &CARD.abilities[0];
        assert_eq!(chosen.trigger, Trigger::Chosen);
        assert!(chosen.condition.is_some(), "with a spell, by you");
        assert!(chosen.targets.is_empty());
        assert!(!chosen.optional);
        assert_eq!(CARDS, 1);
    }

    #[test]
    fn choosing_him_with_your_own_spell_draws_one_when_the_trigger_resolves() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, 0, fixtures::HAND_SPELL);
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {JAE}}}")).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 0, .. } if *card == JAE
        )));
        assert_eq!(ctx.blob.chain.len(), 2, "the spell and his trigger");
        assert!(matches!(
            ctx.blob.chain[1].kind,
            ItemKind::Trigger { source, index: 0 } if source == JAE
        ));
        assert_eq!(ctx.blob.chain[1].controller, 0);
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "his trigger resolved first");
        assert_eq!(ctx.hand_of(0).len(), hand + CARDS);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + CARDS + 1,
            "then Discipline's own draw"
        );
        assert_eq!(ctx.current_might(JAE), 7, "Discipline lands afterwards");
        assert_eq!(ctx.blob.seat(1).draws, 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spell_of_yours_choosing_another_unit_draws_nothing_for_him() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, 0, fixtures::HAND_SPELL);
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the spell alone");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + 1,
            "Discipline's own draw alone"
        );
    }

    #[test]
    fn an_opponents_spell_choosing_him_draws_nothing() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, 0, fixtures::HAND_SPELL);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        let hand = ctx.hand_of(0).len();
        cast(&mut ctx, 1, THEIR_SPELL);
        fixtures::choose(&mut ctx, 1, &format!("{{card {JAE}}}")).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 1, .. } if *card == JAE
        )));
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "two spells and no trigger: an opponent chose him"
        );
        assert!(ctx
            .blob
            .chain
            .iter()
            .all(|held| matches!(held.kind, ItemKind::Spell { .. })));
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn an_ability_choosing_him_is_not_a_spell() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let mut ability = ChainItem::new(
            5,
            ItemKind::Trigger {
                source: fixtures::VI,
                index: 0,
            },
            0,
            Origin::Board,
        );
        ability.status = ItemStatus::Finalized;
        ctx.blob.chain.push(ability);
        ctx.raise(Event::Chosen {
            card: JAE,
            by: 0,
            item: 5,
        });
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "no trigger joins the ability");
        assert_eq!(ctx.hand_of(0).len(), hand);
        let source = Source {
            card: JAE,
            ability: 0,
        };
        assert!(!chosen_by_your_spell(
            &ctx,
            &Event::Chosen {
                card: JAE,
                by: 0,
                item: 5
            },
            source
        ));
        assert!(
            !chosen_by_your_spell(
                &ctx,
                &Event::Chosen {
                    card: JAE,
                    by: 0,
                    item: 99
                },
                source
            ),
            "an item that is not on the chain"
        );
    }
}
