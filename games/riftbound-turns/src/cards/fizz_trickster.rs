use super::prelude::{asking, done, optional, play, unit, with_candidates};
use super::{Card, Filter, Flow, Item, Paying, Stage, TargetKind, TargetSpec, KIND_SPELL};
use crate::engine::ctx::Ctx;
use crate::engine::{cost, pay, play as play_engine, targets};
use crate::state::{ChainItem, ItemKind, Leave, Origin, TargetRef};

pub const ENERGY_AT_MOST: u8 = 3;
const STAGE_PICKED: u8 = 1;

pub const CHEAP_SPELL_IN_TRASH: TargetSpec = TargetSpec {
    filter: Filter::And(&[
        Filter::Kind(KIND_SPELL),
        Filter::InTrash,
        Filter::Friendly,
        Filter::EnergyAtMost(ENERGY_AT_MOST),
    ]),
    min: 0,
    max: 1,
    kind: TargetKind::Card,
    label: "a spell in your trash costing 3 or less",
    min_at_level: None,
};

fn cheap_spells(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    targets::candidates(ctx, item, &CHEAP_SPELL_IN_TRASH)
        .into_iter()
        .filter(|spell| match spell {
            TargetRef::Card(card) => {
                let again = replay(item.controller, *card);
                targets::first_spec_fillable(ctx, &again)
                    && pay::affordable_for(
                        ctx,
                        item.controller,
                        &cost::of_item(ctx, &again, None),
                        Paying::Item(&again),
                    )
            }
            _ => false,
        })
        .collect()
}

fn replay(seat: u8, spell: u32) -> ChainItem {
    ChainItem::new(
        0,
        ItemKind::Spell { card: spell },
        seat,
        Origin::Trash {
            leave: Leave::Recycle,
        },
    )
}

fn trick(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        STAGE_PICKED => {
            let offered = cheap_spells(ctx, item, stage);
            let Some(spell) = ctx
                .picks()
                .first()
                .copied()
                .filter(|spell| offered.contains(&TargetRef::Card(*spell)))
            else {
                return done();
            };
            let seat = item.controller;
            ctx.narrate(format!(
                "{{seat {seat}}} plays {{card {spell}}} from the trash · recycled after"
            ));
            let _ = play_engine::begin(
                ctx,
                seat,
                spell,
                Origin::Trash {
                    leave: Leave::Recycle,
                },
                None,
            );
            done()
        }
        _ => {
            if cheap_spells(ctx, item, Stage(STAGE_PICKED)).is_empty() {
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Fizz - Trickster",
    &[],
    &[asking(
        with_candidates(optional(play(&[], trick)), cheap_spells),
        "a spell in your trash costing 3 or less",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, might_this_turn, spell};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, priority};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::decide::BOTTOM;

    const FIZZ: u32 = 90;
    const CHEAP: u32 = 91;
    const DEAR: u32 = 92;
    const THEIRS: u32 = 93;

    static PUMP: Card = spell(
        "Pump",
        &[],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = crate::cards::prelude::card_target(ctx, item, 0) {
                might_this_turn(ctx, item, unit, 2, None);
            }
            Flow::Done
        })],
    );

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut fizz = fixtures::unit(FIZZ, fixtures::HAND, 0, "Fizz - Trickster", 3);
        fizz.energy = Some(0);
        fixture.table.cards.push(fizz);
        fixture
            .table
            .cards
            .push(fixtures::spell(CHEAP, fixtures::TRASH, 0, "Pump", 3, 0));
        fixture
            .table
            .cards
            .push(fixtures::spell(DEAR, fixtures::TRASH, 0, "Pump", 4, 0));
        fixture
            .table
            .cards
            .push(fixtures::spell(THEIRS, fixtures::TRASH, 1, "Pump", 1, 0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(CHEAP, &PUMP)
            .with_script(DEAR, &PUMP)
            .with_script(THEIRS, &PUMP);
        fixture
    }

    #[test]
    fn fizz_offers_the_cheap_friendly_spell_in_the_trash_plays_it_and_recycles_it() {
        assert!(std::ptr::eq(script_of("Fizz - Trickster").unwrap(), &CARD));
        assert!(CARD.abilities[0].optional);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FIZZ).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, stage: 1 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 91}", "skip"]);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert_eq!(ctx.card(CHEAP).unwrap().zone, Some(fixtures::CHAIN));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 3, spec: 0 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert!(!prompt.cancel, "the may was answered at the pick");
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let top = ctx.blob.chain.last().unwrap();
        assert!(matches!(top.kind, ItemKind::Spell { card } if card == CHEAP));
        assert_eq!(
            top.origin,
            Origin::Trash {
                leave: Leave::Recycle
            }
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(ctx.card(CHEAP).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert!(ctx
            .effects
            .contains(&agni_plugin_sdk::decide::Effect::Move {
                card: CHEAP,
                zone: fixtures::MAIN_DECK,
                seat: 0,
                index: BOTTOM
            }));
        assert!(ctx.banished_of(0).is_empty());
        assert_eq!(ctx.trash_of(0), [DEAR]);
    }

    static BOUNCE: Card = spell(
        "Bounce",
        &[],
        &[play(
            &[crate::cards::prelude::an_enemy_unit("an enemy unit")],
            |_, _, _| Flow::Done,
        )],
    );

    fn no_enemy_in_sight() -> Fixture {
        let mut fixture = armed();
        fixture.table.card_mut(CHEAP).unwrap().name = "Bounce".into();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::HAND);
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(CHEAP, &BOUNCE)
            .with_script(DEAR, &PUMP)
            .with_script(THEIRS, &PUMP);
        fixture
    }

    #[test]
    fn a_spell_without_a_legal_target_is_neither_offered_nor_played_from_the_trash() {
        let mut fixture = no_enemy_in_sight();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FIZZ).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "no playable spell, no question: {:?}",
            fixtures::labels(&ctx)
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.trash_of(0), [CHEAP, DEAR]);
        let mut forced = no_enemy_in_sight();
        let mut ctx = forced.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            CHEAP,
            Origin::Trash {
                leave: Leave::Recycle,
            },
            None,
        )
        .unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(CHEAP).unwrap().zone,
            Some(fixtures::TRASH),
            "an uncancellable play with no legal target goes back where it came from"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 91} can't be played · no legal target".to_string()));
    }

    #[test]
    fn a_spell_whose_cost_the_seat_cannot_pay_is_neither_offered_nor_played_from_the_trash() {
        let mut fixture = armed();
        fixture.table.card_mut(CHEAP).unwrap().power = Some(5);
        fixture.table.card_mut(DEAR).unwrap().power = Some(5);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(CHEAP, &PUMP)
            .with_script(DEAR, &PUMP)
            .with_script(THEIRS, &PUMP);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FIZZ).unwrap();
        let runes = ctx.runes_of(0).len();
        assert!(runes < 5, "five power cannot be paid from {runes} runes");
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "no affordable spell, no question: {:?}",
            fixtures::labels(&ctx)
        );
        assert!(ctx.blob.chain.is_empty());
        let mut forced = armed();
        forced.table.card_mut(DEAR).unwrap().power = Some(5);
        forced.resolve();
        forced.scripts = forced
            .scripts
            .clone()
            .with_script(CHEAP, &PUMP)
            .with_script(DEAR, &PUMP)
            .with_script(THEIRS, &PUMP);
        let mut ctx = forced.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            DEAR,
            Origin::Trash {
                leave: Leave::Recycle,
            },
            None,
        )
        .unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "an uncancellable play nobody can pay for asks nothing: {:?}",
            ctx.blob.why
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(DEAR).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} can't be played · its cost can't be paid".to_string()));
    }

    #[test]
    fn skipping_plays_nothing_and_a_countered_fizz_spell_is_recycled() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FIZZ).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.trash_of(0), [CHEAP, DEAR]);
        let mut again = armed();
        let mut ctx = again.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FIZZ).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(chain::counter(
            &mut ctx,
            3,
            crate::engine::ctx::CounterDest::Trash
        ));
        assert_eq!(ctx.card(CHEAP).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }

    #[test]
    fn an_empty_trash_asks_nothing() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.id != CHEAP && card.id != DEAR);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FIZZ).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
    }
}
