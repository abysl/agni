use super::prelude::{
    asking, deal, done, draw, enemy_units, on_attack, stun, unit, with_candidates,
};
use super::{Card, Domain, Flow, Item, Stage};
use crate::engine::ctx::{is_rune_face, Ctx};
use crate::state::{CardState, TargetRef, FLAG_REVEALING};
use agni_plugin_sdk::decide::{Effect, TOP};

pub const QUESTION: &str = "an enemy unit for the revealed rune";
pub const RED_CARD: u8 = 2;
pub const RED_CARD_SPLASH: u8 = 1;
pub const CARDS: usize = 1;
const REVEALED: u8 = 1;
const FURY_PICK: u8 = 2;
const ORDER_PICK: u8 = 3;

pub fn reveal_top_rune(ctx: &mut Ctx, seat: u8) -> Option<u32> {
    let (Some(deck), Some(chain)) = (ctx.zones.rune_deck, ctx.zones.chain) else {
        return None;
    };
    let top = ctx.top_of(deck, seat, 1).first().copied()?;
    ctx.emit(Effect::Move {
        card: top,
        zone: chain,
        seat: 0,
        index: TOP,
    });
    ctx.set_flag(top, FLAG_REVEALING, true);
    ctx.reveal(top);
    ctx.narrate(format!(
        "{{seat {seat}}} reveals the top rune of their rune deck"
    ));
    Some(top)
}

fn revealing(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .find(|card| {
            ctx.owner(*card) == seat
                && ctx
                    .card(*card)
                    .is_some_and(|held| held.zone == ctx.zones.chain && is_rune_face(held))
        })
}

fn recycle_revealed(ctx: &mut Ctx, seat: u8) -> Option<Domain> {
    let rune = revealing(ctx, seat)?;
    let domain = ctx.domains_of(rune).first().copied();
    ctx.set_flag(rune, FLAG_REVEALING, false);
    if ctx.state_of(rune).is_some_and(CardState::is_default) {
        ctx.blob.drop_card_state(rune);
    }
    ctx.recycle_to_bottom(rune);
    ctx.narrate(format!("{{seat {seat}}} recycles {{card {rune}}}"));
    domain
}

pub fn enemies_here(ctx: &Ctx, me: u32, seat: u8) -> Vec<u32> {
    let Some(here) = ctx.location(me) else {
        return Vec::new();
    };
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat)
        .collect()
}

fn marked(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    let seat = item.controller;
    let units = match stage.0 {
        FURY_PICK => enemies_here(ctx, item.kind.source(), seat),
        ORDER_PICK => enemy_units(ctx, seat),
        _ => Vec::new(),
    };
    units.into_iter().map(TargetRef::Card).collect()
}

fn picked(ctx: &Ctx, item: &Item, stage: Stage) -> Option<u32> {
    let offered = marked(ctx, item, stage);
    ctx.picks()
        .first()
        .copied()
        .filter(|unit| offered.contains(&TargetRef::Card(*unit)))
}

fn pick_a_card(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    match stage.0 {
        REVEALED => match recycle_revealed(ctx, seat) {
            Some(Domain::Fury) => {
                if enemies_here(ctx, me, seat).is_empty() {
                    ctx.narrate(format!("{{card {me}}}: Fury, but no enemy unit here"));
                    return done();
                }
                Flow::Ask(ctx.ask_resume(item, FURY_PICK, 1, 1))
            }
            Some(Domain::Mind) => {
                ctx.narrate(format!("{{card {me}}}: Mind · draw {CARDS}"));
                draw(ctx, seat, CARDS);
                done()
            }
            Some(Domain::Order) => {
                if enemy_units(ctx, seat).is_empty() {
                    ctx.narrate(format!("{{card {me}}}: Order, but no enemy unit"));
                    return done();
                }
                Flow::Ask(ctx.ask_resume(item, ORDER_PICK, 1, 1))
            }
            Some(other) => {
                ctx.narrate(format!(
                    "{{card {me}}}: {} · nothing happens",
                    other.label()
                ));
                done()
            }
            None => done(),
        },
        FURY_PICK => {
            let Some(unit) = picked(ctx, item, stage) else {
                return done();
            };
            deal(ctx, item, unit, RED_CARD);
            let others: Vec<u32> = enemies_here(ctx, me, seat)
                .into_iter()
                .filter(|other| *other != unit)
                .collect();
            for other in &others {
                deal(ctx, item, *other, RED_CARD_SPLASH);
            }
            ctx.narrate(format!(
                "{{card {me}}}: Fury · {RED_CARD} to {{card {unit}}} and {RED_CARD_SPLASH} to {} other enemy units here",
                others.len()
            ));
            done()
        }
        ORDER_PICK => {
            if let Some(unit) = picked(ctx, item, stage) {
                ctx.narrate(format!("{{card {me}}}: Order"));
                stun(ctx, unit);
            }
            done()
        }
        _ => {
            let Some(rune) = reveal_top_rune(ctx, seat) else {
                ctx.narrate(format!("{{seat {seat}}} has no rune to reveal"));
                return done();
            };
            Flow::Ask(ctx.await_faces(item, &[rune], REVEALED))
        }
    }
}

pub static CARD: Card = unit(
    "Twisted Fate - Gambler",
    &[],
    &[asking(
        with_candidates(on_attack(&[], pick_a_card), marked),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, Who, KIND_RUNE};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, prompts, settle, triggers};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::Face;

    const FATE: u32 = 90;
    const SECOND: u32 = 91;
    const AWAY: u32 = 92;
    const TOP_RUNE: u32 = 32;

    fn table() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut fate = fixtures::unit(FATE, fixtures::BF1, 0, "Twisted Fate - Gambler", 4);
        fate.domain = vec!["Chaos".into()];
        fate.energy = Some(4);
        fixture.table.cards.push(fate);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(5);
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 1, "Second", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(AWAY, fixtures::BASE, 1, "Away", 3));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FATE).unwrap(), &CARD));
        fixture
    }

    fn attack(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: FATE });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "the trigger chooses nothing up front"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == FATE
        ));
        pass_until_parked(&mut ctx);
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [TOP_RUNE]);
        assert_eq!(ctx.card(TOP_RUNE).unwrap().zone, Some(fixtures::CHAIN));
        assert!(ctx.has_flag(TOP_RUNE, FLAG_REVEALING));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals the top rune of their rune deck".to_string()));
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, domain: &str) {
        let action = Action::Reveal {
            card: TOP_RUNE,
            face: Face::named(format!("{domain} Rune"))
                .with_kind(KIND_RUNE)
                .with_domain(vec![domain.into()]),
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, TOP_RUNE).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn rune_deck(ctx: &Ctx) -> Vec<u32> {
        ctx.table
            .held(fixtures::RUNE_DECK, 0)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_script_is_a_unit_with_one_attack_trigger_that_asks_per_domain() {
        assert!(std::ptr::eq(
            script_of("Twisted Fate - Gambler").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn a_fury_rune_is_recycled_then_two_land_on_the_pick_and_one_on_every_other_enemy_here() {
        let mut fixture = table();
        attack(&mut fixture);
        arrives(&mut fixture, "Fury");
        let mut ctx = fixture.ctx();
        assert_eq!(
            rune_deck(&ctx),
            [TOP_RUNE, 30, 31],
            "recycled under the rune deck before the mode"
        );
        assert!(!ctx.has_flag(TOP_RUNE, FLAG_REVEALING));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: FURY_PICK
            })
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {SECOND}}}")
            ],
            "Fury reaches only the enemies here"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(SECOND), 2);
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 1);
        assert_eq!(ctx.damage_on(AWAY), 0);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {FATE}}}: Fury · 2 to {{card {SECOND}}} and 1 to 1 other enemy units here"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_mind_rune_draws_one_and_an_order_rune_stuns_an_enemy_anywhere() {
        let mut fixture = table();
        let hand = fixture.ctx().hand_of(0).len();
        attack(&mut fixture);
        arrives(&mut fixture, "Mind");
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(rune_deck(&ctx), [TOP_RUNE, 30, 31]);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FATE}}}: Mind · draw 1")));
        drop(ctx);

        let mut fixture = table();
        attack(&mut fixture);
        arrives(&mut fixture, "Order");
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: ORDER_PICK
            })
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {SECOND}}}"),
                format!("{{card {AWAY}}}")
            ],
            "Order reaches any enemy unit"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {AWAY}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(AWAY));
        assert!(!ctx.is_stunned(SECOND));
        assert_eq!(ctx.damage_on(AWAY), 0);
    }

    #[test]
    fn a_calm_rune_does_nothing_but_recycle_and_an_empty_rune_deck_reveals_nothing() {
        let mut fixture = table();
        attack(&mut fixture);
        arrives(&mut fixture, "Calm");
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(rune_deck(&ctx), [TOP_RUNE, 30, 31]);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FATE}}}: Calm · nothing happens")));
        assert_eq!(ctx.damage_on(SECOND), 0);
        drop(ctx);

        let mut empty = table();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::RUNE_DECK) || card.seat != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        ctx.raise(Event::Attacks { card: FATE });
        triggers::collect(&mut ctx);
        chain::proceed(&mut ctx);
        pass_until_parked(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no rune to reveal".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_fury_rune_with_no_enemy_here_and_a_pick_that_fled_both_end_quietly() {
        let mut fixture = table();
        for unit in [fixtures::THEIR_UNIT, SECOND] {
            fixture.table.card_mut(unit).unwrap().zone = Some(fixtures::BASE);
        }
        fixture.resolve();
        attack(&mut fixture);
        arrives(&mut fixture, "Fury");
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FATE}}}: Fury, but no enemy unit here")));
        drop(ctx);

        let mut fixture = table();
        attack(&mut fixture);
        arrives(&mut fixture, "Fury");
        let mut ctx = fixture.ctx();
        ctx.table.card_mut(SECOND).unwrap().zone = Some(fixtures::BASE);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::THEIR_UNIT)],
            "a unit that left is no longer offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 2);
        assert_eq!(ctx.damage_on(SECOND), 0);
    }
}
