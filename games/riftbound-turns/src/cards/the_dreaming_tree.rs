use super::prelude::{
    battlefield, done, draw, location_of, on_any_unit_chosen, once_per_seat_each_turn, when,
    Location,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::state::ItemKind;

const DRAWS: usize = 1;

fn here(ctx: &Ctx, source: Source) -> Option<Location> {
    match location_of(ctx, source.card) {
        at @ Some(Location::Battlefield(_)) => at,
        _ => None,
    }
}

pub fn chosen_here_by_a_spell(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Chosen { card, item, .. } = event else {
        return false;
    };
    let here = here(ctx, source);
    here.is_some()
        && ctx.location(*card) == here
        && ctx
            .chain_item(*item)
            .is_some_and(|held| matches!(held.kind, ItemKind::Spell { .. }))
}

pub fn any_player_choosing_their_own_unit_here(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Chosen { card, by, .. } = event else {
        return false;
    };
    ctx.controller(*card) == *by && ctx.is_unit(*card) && chosen_here_by_a_spell(ctx, event, source)
}

fn they_draw_one(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = battlefield(
    "The Dreaming Tree",
    &[],
    &[once_per_seat_each_turn(when(
        on_any_unit_chosen(&[], they_draw_one),
        chosen_here_by_a_spell,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, on_chosen, play, spell, unit};
    use crate::cards::{Keyword, Once, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, triggers};
    use crate::state::{once_by_seat, Origin, PromptWhy};

    const TREE: u32 = fixtures::GROUNDS;
    const BUFF: u32 = 90;
    const BUFF_AGAIN: u32 = 91;
    const SECOND: u32 = 92;
    const HOMEBODY: u32 = 93;

    static BUFF_CARD: Card = spell(
        "Buff",
        &[Keyword::Action],
        &[play(&[a_unit("a unit")], |_, _, _| Flow::Done)],
    );

    static WATCHER_CARD: Card = unit("Watcher", &[], &[on_chosen(&[], |_, _, _| Flow::Done)]);

    fn tree_held_by_me() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(TREE).unwrap().name = "The Dreaming Tree".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 0, "Jinx", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(HOMEBODY, fixtures::BASE, 0, "Caitlyn", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut buff = fixtures::spell(BUFF, fixtures::HAND, 0, "Buff", 1, 0);
        buff.domain = vec!["Fury".into()];
        fixture.table.cards.push(buff);
        let mut second = fixtures::spell(BUFF_AGAIN, fixtures::HAND, 0, "Buff", 1, 0);
        second.domain = vec!["Fury".into()];
        fixture.table.cards.push(second);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BUFF, &BUFF_CARD)
            .with_script(BUFF_AGAIN, &BUFF_CARD);
        assert!(std::ptr::eq(fixture.scripts.of_card(TREE).unwrap(), &CARD));
        fixture
    }

    fn cast_at(ctx: &mut Ctx, spell: u32, target: u32) {
        fixtures::play_from_hand(ctx, 0, spell).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(ctx, 0, &format!("{{card {target}}}")).unwrap();
    }

    fn tree_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == TREE => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn resolve_all(ctx: &mut Ctx) {
        for _ in 0..8 {
            if ctx.blob.chain.is_empty() {
                return;
            }
            priority::pass(ctx, 0).unwrap();
            priority::pass(ctx, 1).unwrap();
        }
    }

    #[test]
    fn the_tree_is_a_once_per_seat_chosen_friendly_trigger_with_a_spell_here_condition() {
        assert!(std::ptr::eq(
            crate::cards::script_of("The Dreaming Tree").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::ChosenFriendly(Who::Any));
        assert_eq!(ability.once, Once::PerSeatPerTurn);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
    }

    #[test]
    fn choosing_a_friendly_unit_here_with_a_spell_draws_one_the_first_time_each_turn() {
        let mut fixture = tree_held_by_me();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, BUFF, fixtures::VI);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 0, .. } if *card == fixtures::VI
        )));
        assert_eq!(tree_items(&ctx), [0]);
        assert!(ctx.has_flag(TREE, once_by_seat(0)));
        assert_eq!(ctx.blob.chain.len(), 2, "the spell and the tree's trigger");
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "not before it resolves");
        resolve_all(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand, "the spell left, a card came");
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, BUFF_AGAIN, SECOND);
        assert!(
            tree_items(&ctx).is_empty(),
            "the second time this turn is silent"
        );
        resolve_all(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_chosen_in_a_base_or_by_an_ability_draws_nothing() {
        let mut fixture = tree_held_by_me();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, BUFF, HOMEBODY);
        assert!(tree_items(&ctx).is_empty(), "not here");
        assert!(!ctx.has_flag(TREE, once_by_seat(0)));
        resolve_all(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        drop(ctx);
        let mut ability = tree_held_by_me();
        let ctx = ability.ctx();
        let source = Source {
            card: TREE,
            ability: 0,
        };
        let trigger = crate::state::ChainItem::new(
            9,
            ItemKind::Trigger {
                source: SECOND,
                index: 0,
            },
            0,
            Origin::Board,
        );
        ctx.blob.chain.push(trigger);
        assert!(
            !chosen_here_by_a_spell(
                &ctx,
                &Event::Chosen {
                    card: fixtures::VI,
                    by: 0,
                    item: 9,
                },
                source
            ),
            "chosen by an ability, not a spell"
        );
        let buff =
            crate::state::ChainItem::new(10, ItemKind::Spell { card: BUFF }, 0, Origin::Hand);
        ctx.blob.chain.push(buff);
        assert!(chosen_here_by_a_spell(
            &ctx,
            &Event::Chosen {
                card: fixtures::VI,
                by: 0,
                item: 10,
            },
            source
        ));
        assert!(!chosen_here_by_a_spell(
            &ctx,
            &Event::Chosen {
                card: HOMEBODY,
                by: 0,
                item: 10,
            },
            source
        ));
    }

    #[test]
    fn the_watchers_own_chosen_trigger_and_the_tree_form_one_batch_and_an_uncontrolled_tree_belongs_to_the_turn_player(
    ) {
        let mut fixture = tree_held_by_me();
        fixture.table.card_mut(SECOND).unwrap().name = "Watcher".into();
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BUFF, &BUFF_CARD)
            .with_script(SECOND, &WATCHER_CARD);
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BUFF, SECOND);
        assert_eq!(ctx.blob.why, Some(PromptWhy::OrderTriggers { seat: 0 }));
        drop(ctx);
        let mut nobody = tree_held_by_me();
        nobody.blob.set_holder(fixtures::BF1, None);
        let mut ctx = nobody.ctx();
        cast_at(&mut ctx, BUFF, fixtures::VI);
        assert_eq!(
            tree_items(&ctx),
            [0],
            "184.6: uncontrolled, the turn player's"
        );
    }

    #[test]
    fn the_opponent_choosing_their_own_unit_here_with_a_spell_draws_too() {
        let mut fixture = tree_held_by_me();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = fixture.ctx();
        let source = Source {
            card: TREE,
            ability: 0,
        };
        let spell =
            crate::state::ChainItem::new(11, ItemKind::Spell { card: BUFF_AGAIN }, 1, Origin::Hand);
        ctx.blob.chain.push(spell);
        let event = Event::Chosen {
            card: fixtures::THEIR_UNIT,
            by: 1,
            item: 11,
        };
        assert!(any_player_choosing_their_own_unit_here(
            &ctx, &event, source
        ));
        let found = triggers::find(&ctx, &event);
        assert!(
            found
                .iter()
                .any(|found| found.source == TREE && found.controller == 1),
            "Who::Any admits the chooser as the controller, whoever holds the tree: {found:?}"
        );
    }
}
