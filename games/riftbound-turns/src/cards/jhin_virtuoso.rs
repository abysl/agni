use super::prelude::{
    asking, banish_by, done, draw, legend, optional, triggered, when, with_candidates,
};
use super::revna_the_lorekeeper::spent_four_or_more_on;
use super::the_zero_drive::units_banished_with;
use super::{Card, Event, Flow, Item, Source, Stage, Trigger, KIND_SPELL};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const FOUR_SPELLS: usize = 4;
pub const CHANNELS: usize = 4;
pub const DRAWS: usize = 1;
pub const BANISH_QUESTION: &str = "the spell to banish with Jhin";
const STAGE_BANISH: u8 = 1;

pub fn played_spell_card_until_played_spell_carries_it(_: &Ctx, _: &Item) -> Option<u32> {
    None
}

fn a_spell_you_spent_four_or_more_on(ctx: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::PlayedSpell { .. }) && spent_four_or_more_on(ctx, event)
}

pub fn spells_banished_with(ctx: &Ctx, me: u32) -> Vec<u32> {
    units_banished_with(ctx, me)
        .into_iter()
        .filter(|card| ctx.kind_of(*card) == Some(KIND_SPELL))
        .collect()
}

pub fn curtain_call(ctx: &mut Ctx, seat: u8, me: u32, banished: &[u32]) -> bool {
    if banished.len() < FOUR_SPELLS {
        return false;
    }
    ctx.narrate(format!(
        "four spells are banished with {{card {me}}} · the curtain falls"
    ));
    for spell in banished {
        if ctx.in_banishment(*spell) {
            ctx.trash(*spell);
            ctx.narrate(format!("{{card {spell}}} is put in its trash"));
        }
    }
    let channelled = ctx.channel(seat, CHANNELS);
    ctx.narrate(format!("{{seat {seat}}} channels {channelled}"));
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    true
}

fn the_spell(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    played_spell_card_until_played_spell_carries_it(ctx, item)
        .map(TargetRef::Card)
        .into_iter()
        .collect()
}

fn virtuoso(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    match stage.0 {
        STAGE_BANISH => {
            if let Some(spell) = played_spell_card_until_played_spell_carries_it(ctx, item) {
                if ctx.picks().contains(&spell) && banish_by(ctx, spell, seat) {
                    ctx.narrate(format!("{{card {spell}}} is banished with {{card {me}}}"));
                }
            }
            let banished = spells_banished_with(ctx, me);
            curtain_call(ctx, seat, me, &banished);
            done()
        }
        _ => {
            if played_spell_card_until_played_spell_carries_it(ctx, item).is_none() {
                ctx.narrate(format!(
                    "{{card {me}}} · the spell can't be found once it has resolved · the engine owes a PlayedSpell payload"
                ));
                let banished = spells_banished_with(ctx, me);
                curtain_call(ctx, seat, me, &banished);
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_BANISH, 0, 1))
        }
    }
}

pub static CARD: Card = legend(
    "Jhin - Virtuoso",
    &[],
    &[asking(
        with_candidates(
            optional(when(
                triggered(Trigger::YouPlaySpell, &[], virtuoso),
                a_spell_you_spent_four_or_more_on,
            )),
            the_spell,
        ),
        BANISH_QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::revna_the_lorekeeper::SPENT_AT_LEAST;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_TARGET;
    use crate::engine::{chain, settle, triggers};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending};

    const JHIN: u32 = fixtures::LEGEND_CARD;
    const BIG_SPELL: u32 = 90;
    const BANISHED: [u32; 4] = [91, 92, 93, 94];
    const FURY_RUNES: [u32; 3] = [46, 47, 48];

    fn theatre() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(JHIN).unwrap().name = CARD.name.into();
        fixture.table.cards.push(fixtures::spell(
            BIG_SPELL,
            fixtures::HAND,
            0,
            "Fourth Shot",
            4,
            0,
        ));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
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

    fn with_banished_spells(count: usize) -> Fixture {
        let mut fixture = theatre();
        for spell in BANISHED.iter().take(count) {
            fixture.table.cards.push(fixtures::spell(
                *spell,
                fixtures::BANISHMENT,
                0,
                "Bullet Four",
                4,
                0,
            ));
        }
        fixture.resolve();
        fixture
    }

    fn his_trigger(ctx: &Ctx) -> bool {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .any(|held| matches!(held.kind, ItemKind::Trigger { source, index: 0 } if source == JHIN))
    }

    fn queue_the_trigger(ctx: &mut Ctx) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: JHIN,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.stage = STAGE_TARGET;
        item.subject = Some(TargetRef::Item(7));
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
        settle(ctx).unwrap();
    }

    #[test]
    fn the_legend_has_one_gated_may_you_play_spell_trigger_and_names_its_two_seams() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlaySpell);
        assert!(ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_some(), "four or more spent");
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(BANISH_QUESTION));
        assert!(ability.targets.is_empty());
        assert_eq!(SPENT_AT_LEAST, 4);
        assert_eq!(FOUR_SPELLS, 4);
        assert_eq!(CHANNELS, 4);
        assert_eq!(DRAWS, 1);
        let mut fixture = with_banished_spells(4);
        let ctx = fixture.ctx();
        assert!(
            spells_banished_with(&ctx, JHIN).is_empty(),
            "the Zero Drive link reads nothing until CardState carries it"
        );
        assert!(!spent_four_or_more_on(
            &ctx,
            &Event::PlayedSpell {
                item: 1,
                controller: 0,
                nth: 1
            }
        ));
        let me = Source {
            card: JHIN,
            ability: 0,
        };
        assert!(!a_spell_you_spent_four_or_more_on(
            &ctx,
            &Event::PlayedSpell {
                item: 1,
                controller: 0,
                nth: 1
            },
            me
        ));
        assert!(!a_spell_you_spent_four_or_more_on(
            &ctx,
            &Event::Played {
                card: BIG_SPELL,
                controller: 0,
                kind: KIND_SPELL.into(),
                origin: Origin::Hand,
                paid_additional: false
            },
            me
        ));
    }

    #[test]
    fn today_a_spell_played_for_four_queues_no_trigger_and_an_opponents_spell_is_not_yours() {
        let mut fixture = theatre();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BIG_SPELL).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "four energy paid");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!his_trigger(&ctx));
        assert_eq!(
            ctx.card(BIG_SPELL).unwrap().zone,
            Some(fixtures::TRASH),
            "the spell resolved to the trash, unbanished"
        );
        ctx.raise(Event::PlayedSpell {
            item: 9,
            controller: 1,
            nth: 1,
        });
        assert_eq!(triggers::collect(&mut ctx), 0);
        chain::proceed(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn queued_by_hand_the_trigger_names_the_payload_gap_and_banishes_nothing() {
        let mut fixture = theatre();
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "a free may asks nothing at the cost stage"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "no spell to offer, no ask");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {JHIN}}} · the spell can't be found once it has resolved · the engine owes a PlayedSpell payload"
        )));
        assert!(ctx.banished_of(0).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_curtain_call_trashes_four_banished_spells_channels_four_and_draws_one() {
        let mut fixture = with_banished_spells(4);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = ctx.table.held(fixtures::RUNE_POOL, 0).count();
        let deck = ctx.table.held(fixtures::RUNE_DECK, 0).count();
        assert_eq!(deck, 3);
        for spell in BANISHED {
            assert!(ctx.in_banishment(spell));
        }
        assert!(curtain_call(&mut ctx, 0, JHIN, &BANISHED));
        for spell in BANISHED {
            assert!(!ctx.in_banishment(spell));
            assert_eq!(ctx.card(spell).unwrap().zone, Some(fixtures::TRASH));
            assert!(ctx
                .blob
                .log
                .contains(&format!("{{card {spell}}} is put in its trash")));
        }
        assert_eq!(
            ctx.table.held(fixtures::RUNE_POOL, 0).count(),
            pool + deck,
            "430.3 · the rune deck holds three, so three are channelled"
        );
        assert_eq!(ctx.table.held(fixtures::RUNE_DECK, 0).count(), 0);
        assert!(ctx.blob.log.contains(&"{seat 0} channels 3".to_string()));
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn three_banished_spells_are_not_four_and_nothing_moves() {
        let mut fixture = with_banished_spells(3);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = ctx.table.held(fixtures::RUNE_POOL, 0).count();
        assert!(!curtain_call(&mut ctx, 0, JHIN, &BANISHED[..3]));
        for spell in &BANISHED[..3] {
            assert!(ctx.in_banishment(*spell));
        }
        assert_eq!(ctx.table.held(fixtures::RUNE_POOL, 0).count(), pool);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.effects.is_empty());
    }

    #[test]
    #[ignore = "engine gap · PlayedSpell carries no card and no paid cost (the Yordle Explorer row: forgotten_library::energy_spent_on_the_spell, played_spell_card_until_played_spell_carries_it) and CardState carries no banished-with link (the Zero Drive row: units_banished_with), so a four-energy spell is never offered to banish and the fourth never trashes the set"]
    fn a_fourth_spell_played_for_four_and_banished_puts_all_four_in_the_trash_channels_and_draws() {
        let mut fixture = with_banished_spells(3);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, BIG_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(his_trigger(&ctx), "when you play a spell · four spent");
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BIG_SPELL}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        for spell in BANISHED[..3].iter().chain([BIG_SPELL].iter()) {
            assert_eq!(ctx.card(*spell).unwrap().zone, Some(fixtures::TRASH));
        }
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + DRAWS);
        assert!(spells_banished_with(&ctx, JHIN).is_empty());
    }
}
