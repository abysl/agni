use super::prelude::{
    a_card, card_target, deal, done, on_defend, on_play_from_facedown, unit, ENEMY_UNIT_HERE,
};
use super::{Ability, Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::{CardState, FLAG_REVEALING};
use agni_plugin_sdk::decide::{Effect, TOP};

pub const LOOK: usize = 5;
pub const TARGET: TargetSpec = a_card(
    ENEMY_UNIT_HERE,
    "an enemy unit here to deal 1 to per Hidden card revealed",
);
const REVEALED: u8 = 1;

pub fn reveal_top_cards(ctx: &mut Ctx, seat: u8, count: usize) -> Vec<u32> {
    let (Some(deck), Some(chain)) = (ctx.zones.main_deck, ctx.zones.chain) else {
        return Vec::new();
    };
    let top = ctx.top_of(deck, seat, count);
    for card in &top {
        ctx.emit(Effect::Move {
            card: *card,
            zone: chain,
            seat: 0,
            index: TOP,
        });
        ctx.set_flag(*card, FLAG_REVEALING, true);
        ctx.reveal(*card);
    }
    if !top.is_empty() {
        ctx.narrate(format!(
            "{{seat {seat}}} reveals the top {} cards of their deck",
            top.len()
        ));
    }
    top
}

fn revealing(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let Some(chain) = ctx.zones.chain else {
        return Vec::new();
    };
    ctx.table
        .held(chain, 0)
        .map(|card| card.id)
        .filter(|card| ctx.has_flag(*card, FLAG_REVEALING) && ctx.owner(*card) == seat)
        .collect()
}

fn recycle_revealed(ctx: &mut Ctx, seat: u8, cards: &[u32]) {
    for card in cards {
        ctx.set_flag(*card, FLAG_REVEALING, false);
        if ctx.state_of(*card).is_some_and(CardState::is_default) {
            ctx.blob.drop_card_state(*card);
        }
        ctx.recycle_to_bottom(*card);
    }
    ctx.narrate(format!("{{seat {seat}}} recycles {}", cards.len()));
}

fn scout(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    if stage.0 == REVEALED {
        let revealed = revealing(ctx, seat);
        let hidden = revealed
            .iter()
            .filter(|card| ctx.has_keyword(**card, Keyword::Hidden))
            .count();
        let amount = u8::try_from(hidden).unwrap_or(u8::MAX);
        match card_target(ctx, item, 0) {
            Some(unit) if amount > 0 => {
                deal(ctx, item, unit, amount);
                ctx.narrate(format!(
                    "{{card {me}}} deals {amount} to {{card {unit}}} · {hidden} of {} revealed cards have Hidden",
                    revealed.len()
                ));
            }
            Some(_) => ctx.narrate(format!("{{card {me}}}: no revealed card has Hidden")),
            None => ctx.narrate(format!("{{card {me}}}: the target is gone")),
        }
        recycle_revealed(ctx, seat, &revealed);
        return done();
    }
    let top = reveal_top_cards(ctx, seat, LOOK);
    if top.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no cards left to reveal"));
        return done();
    }
    Flow::Ask(ctx.await_faces(item, &top, REVEALED))
}

const ON_DEFEND: Ability = on_defend(&[TARGET], scout);
const ON_PLAY_FROM_HIDDEN: Ability = on_play_from_facedown(&[TARGET], scout);

pub static CARD: Card = unit(
    "Teemo - Strategist",
    &[Keyword::Hidden],
    &[ON_DEFEND, ON_PLAY_FROM_HIDDEN],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, Who, KIND_SPELL, KIND_UNIT};
    use crate::engine::ctx::{Cause, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, play as play_engine, settle, triggers};
    use crate::state::{ItemKind, ItemStatus, Origin, PromptWhy, TargetRef};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::Face;

    const TEEMO: u32 = 90;
    const SECOND: u32 = 91;
    const DECK: [u32; 4] = [23, 22, 21, 20];

    fn face_of(card: u32) -> Face {
        match card {
            23 | 21 => Face::named("Back Off")
                .with_kind(KIND_SPELL)
                .with_domain(vec!["Calm".into()]),
            22 => Face::named("Jinx").with_kind(KIND_UNIT),
            _ => Face::named("Spark").with_kind(KIND_SPELL),
        }
    }

    fn scouting(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut teemo = fixtures::unit(TEEMO, zone, 0, "Teemo - Strategist", 2);
        teemo.domain = vec!["Mind".into()];
        teemo.energy = Some(2);
        teemo.power = Some(1);
        fixture.table.cards.push(teemo);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(5);
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 1, "Second", 5));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(TEEMO).unwrap(), &CARD));
        fixture
    }

    fn park(ctx: &mut Ctx) {
        pass_until_parked(ctx);
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, DECK);
        for card in DECK {
            assert_eq!(ctx.card(card).unwrap().zone, Some(fixtures::CHAIN));
            assert!(ctx.has_flag(card, FLAG_REVEALING));
        }
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals the top 4 cards of their deck".to_string()));
        assert!(ctx.fault.is_none());
    }

    fn defends(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Defends { card: TEEMO });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == TEEMO
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(SECOND)]);
        park(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, card: u32) -> bool {
        let action = Action::Reveal {
            card,
            face: face_of(card),
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none());
        let finished = ctx.blob.chain.is_empty();
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        finished
    }

    fn deck_of(ctx: &Ctx) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, 0)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_script_prints_hidden_with_a_defend_and_a_played_from_hidden_trigger_at_an_enemy_here() {
        assert!(std::ptr::eq(
            script_of("Teemo - Strategist").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, &[Keyword::Hidden]);
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Defends(Who::Me));
        assert_eq!(CARD.abilities[1].trigger, Trigger::PlayFromFacedown);
        for ability in CARD.abilities {
            assert_eq!(ability.targets, &[TARGET]);
            assert!(!ability.optional);
        }
    }

    #[test]
    fn defending_reveals_the_top_of_the_deck_and_each_hidden_face_is_one_damage_to_the_pick() {
        let mut fixture = scouting(fixtures::BF1);
        defends(&mut fixture);
        assert!(!arrives(&mut fixture, 23));
        assert!(!arrives(&mut fixture, 22));
        assert!(!arrives(&mut fixture, 21));
        {
            let ctx = fixture.ctx();
            assert_eq!(
                ctx.blob.chain[0].awaiting,
                [20],
                "three faces in, one to go"
            );
            assert_eq!(ctx.damage_on(SECOND), 0);
        }
        assert!(arrives(&mut fixture, 20));
        let ctx = fixture.ctx();
        assert_eq!(ctx.damage_on(SECOND), 2, "two Back Offs among the four");
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert_eq!(
            deck_of(&ctx),
            [20, 21, 22, 23],
            "recycled under the deck as a block that keeps its order"
        );
        for card in DECK {
            assert!(!ctx.has_flag(card, FLAG_REVEALING));
            assert!(ctx.state_of(card).is_none(), "no state row lingers");
        }
        assert!(ctx.blob.log.contains(&format!(
            "{{card {TEEMO}}} deals 2 to {{card {SECOND}}} · 2 of 4 revealed cards have Hidden"
        )));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 4".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_from_hidden_the_second_ability_fires_at_the_hiding_battlefield() {
        let mut fixture = scouting(fixtures::BF1);
        fixture.blob.card_state_mut(TEEMO).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            TEEMO,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            Some(Location::Battlefield(fixtures::BF1)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, origin: Origin::Facedown { .. }, .. } if *card == TEEMO
        )));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {SECOND}}}")
            ],
            "the enemies at the hiding battlefield"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 1 }) if source == TEEMO
        ));
        park(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        for card in [23, 22, 21] {
            assert!(!arrives(&mut fixture, card));
        }
        assert!(arrives(&mut fixture, 20));
        let ctx = fixture.ctx();
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 2);
        assert_eq!(ctx.damage_on(SECOND), 0);
        assert_eq!(deck_of(&ctx), [20, 21, 22, 23]);
    }

    #[test]
    fn attacking_never_fires_it_an_empty_deck_reveals_nothing_and_a_target_that_fled_takes_nothing()
    {
        let mut fixture = scouting(fixtures::BF1);
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: TEEMO });
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "the scout reads only defends"
        );
        drop(ctx);

        let mut empty = scouting(fixtures::BF1);
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        ctx.raise(Event::Defends { card: TEEMO });
        triggers::collect(&mut ctx);
        chain::proceed(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        pass_until_parked(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to reveal".to_string()));
        assert_eq!(ctx.damage_on(SECOND), 0);
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fled = scouting(fixtures::BF1);
        defends(&mut fled);
        fled.table.card_mut(SECOND).unwrap().zone = Some(fixtures::BASE);
        fled.resolve();
        for card in [23, 22, 21] {
            assert!(!arrives(&mut fled, card));
        }
        assert!(arrives(&mut fled, 20));
        let ctx = fled.ctx();
        assert_eq!(ctx.damage_on(SECOND), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TEEMO}}}: the target is gone")));
        assert_eq!(
            deck_of(&ctx),
            [20, 21, 22, 23],
            "the reveal is recycled whether or not the shot lands"
        );
    }

    #[test]
    fn a_reveal_with_no_hidden_face_deals_nothing_and_the_damage_is_attributed_to_the_trigger() {
        let mut fixture = scouting(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| ![23, 21].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Defends { card: TEEMO });
        triggers::collect(&mut ctx);
        chain::proceed(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        pass_until_parked(&mut ctx);
        assert_eq!(ctx.blob.chain[0].awaiting, [22, 20]);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        assert!(!arrives(&mut fixture, 22));
        assert!(arrives(&mut fixture, 20));
        let ctx = fixture.ctx();
        assert_eq!(ctx.damage_on(SECOND), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TEEMO}}}: no revealed card has Hidden")));
        assert_eq!(deck_of(&ctx), [20, 22]);
        drop(ctx);

        let mut fixture = scouting(fixtures::BF1);
        defends(&mut fixture);
        for card in [23, 22, 21] {
            assert!(!arrives(&mut fixture, card));
        }
        let action = Action::Reveal {
            card: 20,
            face: face_of(20),
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, 20).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: SECOND,
            n: 2,
            source: Cause::Item(1)
        }));
    }
}
