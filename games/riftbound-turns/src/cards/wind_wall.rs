use super::prelude::{a_spell, counter_spell, done, play, spell};
use super::{Card, Keyword};

pub static CARD: Card = spell(
    "Wind Wall",
    &[Keyword::Reaction],
    &[play(&[a_spell("a spell to counter")], |ctx, item, _| {
        counter_spell(ctx, item, 0);
        done()
    })],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::SPELL_ON_CHAIN;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::{Ctx, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play as play_engine, priority, prompts, settle, targets};
    use crate::state::{ChainItem, ItemKind, ItemStatus, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const WIND_WALL: u32 = 90;
    const CALM_RUNES: [u32; 5] = [46, 47, 48, 49, 51];

    fn wind_wall(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(WIND_WALL, fixtures::HAND, seat, "Wind Wall", 3, 2);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(wind_wall(1));
        for rune in CALM_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Calm", false));
        }
        fixture.table.card_mut(fixtures::HAND_SPELL).unwrap().energy = Some(6);
        fixture.table.card_mut(fixtures::HAND_SPELL).unwrap().power = Some(0);
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = false;
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        for id in [52, 53] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WIND_WALL).unwrap(),
            &CARD
        ));
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn finalized(id: u16, kind: ItemKind, controller: u8) -> ChainItem {
        let mut item = ChainItem::new(id, kind, controller, Origin::Hand);
        item.status = ItemStatus::Finalized;
        item
    }

    #[test]
    fn the_script_is_a_reaction_over_any_one_spell_on_the_chain() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Wind Wall").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, SPELL_ON_CHAIN);
        assert_eq!(ability.targets[0].kind, TargetKind::Item);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn wind_wall_counters_an_expensive_spell_that_defy_could_not_reach() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, WIND_WALL)),
            Err(Refusal::NotYourTurn),
            "an empty chain on the other seat's turn gives nothing to react to"
        );
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, WIND_WALL)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 0 holds priority first"
        );
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, WIND_WALL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 71} on the chain", "cancel"],
            "a six-energy spell is within Wind Wall's reach"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 2, spec: 0 }),
            "{card 90}: choose a spell to counter (0 of 1)"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 2, 0, &[2]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Wind Wall never counters itself"
        );
        fixtures::choose(&mut ctx, 1, "{card 71} on the chain").unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Item(1)]);
        let recycled = ctx
            .effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Move {
                        zone: fixtures::RUNE_DECK,
                        seat: 1,
                        index: BOTTOM,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(
            recycled, 2,
            "two Calm runes pay the power: {:?}",
            ctx.effects
        );
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.card(WIND_WALL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{card 71} is countered".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line == "{card 71} resolves"));
        assert!(ctx.blob.log.contains(&"{card 90} resolves".to_string()));
        let played: Vec<&Event> = ctx
            .events
            .iter()
            .filter(|event| matches!(event, Event::PlayedSpell { .. }))
            .collect();
        assert_eq!(
            played,
            [&Event::PlayedSpell {
                item: 2,
                controller: 1,
                nth: 1
            }],
            "the countered spell was never played"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn wind_wall_offers_only_spells_and_never_an_ability_on_the_chain() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        ctx.blob.chain.push(finalized(
            1,
            ItemKind::Trigger {
                source: fixtures::VI,
                index: 0,
            },
            0,
        ));
        let wall = ChainItem::new(3, ItemKind::Spell { card: WIND_WALL }, 1, Origin::Hand);
        let spec = &CARD.abilities[0].targets[0];
        assert_eq!(
            targets::candidates(&ctx, &wall, spec),
            Vec::<TargetRef>::new(),
            "an ability is not a spell"
        );
        ctx.blob.chain.push(finalized(
            2,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
        ));
        assert_eq!(
            targets::candidates(&ctx, &wall, spec),
            [TargetRef::Item(2)],
            "its controller's own spell is a legal choice"
        );
        ctx.blob.chain[1].status = ItemStatus::Resolving;
        assert_eq!(
            targets::candidates(&ctx, &wall, spec),
            Vec::<TargetRef>::new()
        );
    }

    #[test]
    fn with_only_an_ability_on_the_chain_the_play_can_only_be_taken_back() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.blob.chain.push(finalized(
            1,
            ItemKind::Trigger {
                source: fixtures::VI,
                index: 0,
            },
            0,
        ));
        ctx.blob.priority = Some(crate::state::Priority {
            active: 1,
            passes: 1,
        });
        settle(&mut ctx).unwrap();
        let hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 1, WIND_WALL).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["cancel"]);
        let Some(PromptWhy::Target { item, spec: 0 }) = ctx.blob.why else {
            panic!("the target prompt is open: {:?}", ctx.blob.why);
        };
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[1]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 1, "cancel").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.card(WIND_WALL).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(1).len(), hand);
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move {
                    zone: fixtures::RUNE_DECK,
                    ..
                }
            )),
            "a cancelled Wind Wall paid nothing"
        );
    }
}
