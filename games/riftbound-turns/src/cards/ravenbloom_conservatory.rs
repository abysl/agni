use super::prelude::{battlefield, done, triggered, when};
use super::{Card, Flow, Item, Source, Stage, Trigger, Who, KIND_SPELL};
use crate::engine::ctx::{Ctx, Event, Location};
use crate::state::{CardState, FLAG_DEFENDER, FLAG_REVEALING};
use agni_plugin_sdk::decide::{Effect, TOP};

const REVEALED: u8 = 1;

fn first_defender_here(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Defends { card } = event else {
        return false;
    };
    let here = ctx
        .card(source.card)
        .and_then(|held| held.zone)
        .map(Location::Battlefield);
    here.is_some()
        && ctx.location(*card) == here
        && ctx
            .designated(FLAG_DEFENDER)
            .into_iter()
            .find(|unit| ctx.location(*unit) == here)
            == Some(*card)
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == REVEALED {
        return sort(ctx, item);
    }
    let seat = item.controller;
    let Some(top) = ctx.reveal_top(seat) else {
        ctx.narrate(format!("{{seat {seat}}} has no card to reveal"));
        return done();
    };
    Flow::Ask(ctx.await_faces(item, &[top], REVEALED))
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
                    .is_some_and(|held| held.zone == ctx.zones.chain)
        })
}

fn sort(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let Some(card) = revealing(ctx, seat) else {
        return done();
    };
    ctx.set_flag(card, FLAG_REVEALING, false);
    if ctx.state_of(card).is_some_and(CardState::is_default) {
        ctx.blob.drop_card_state(card);
    }
    if ctx.kind_of(card) == Some(KIND_SPELL) {
        if let Some(hand) = ctx.zones.hand {
            ctx.emit(Effect::Move {
                card,
                zone: hand,
                seat,
                index: TOP,
            });
        }
        ctx.narrate(format!(
            "{{seat {seat}}} puts {{card {card}}} into their hand"
        ));
    } else {
        ctx.recycle_to_bottom(card);
        ctx.narrate(format!("{{seat {seat}}} recycles {{card {card}}}"));
    }
    done()
}

pub static CARD: Card = battlefield(
    "Ravenbloom Conservatory",
    &[],
    &[when(
        triggered(Trigger::Defends(Who::You), &[], run),
        first_defender_here,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::KIND_UNIT;
    use crate::engine::ctx::MoveCause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, cleanup, settle};
    use crate::state::{ItemKind, ItemStatus};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::Face;

    const CONSERVATORY: u32 = 51;
    const THEIR_SECOND: u32 = 82;
    const THEIR_TOP: u32 = 25;

    fn held_by_them() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::GROUNDS);
        fixture.table.cards.push(fixtures::card(
            CONSERVATORY,
            fixtures::BF1,
            1,
            "Ravenbloom Conservatory",
            "Battlefield",
        ));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn attack(fixture: &mut Fixture) -> u16 {
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(ctx
            .blob
            .showdown
            .as_ref()
            .is_some_and(|showdown| showdown.combat));
        assert!(ctx.is_defender(fixtures::THEIR_UNIT));
        let item = ctx
            .blob
            .chain
            .iter()
            .find(|held| {
                matches!(held.kind, ItemKind::Trigger { source, .. } if source == CONSERVATORY)
            })
            .map(|held| held.id)
            .expect("the defend trigger is on the chain");
        pass_until_parked(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        item
    }

    fn arrives(fixture: &mut Fixture, face: Face) {
        let action = Action::Reveal {
            card: THEIR_TOP,
            face,
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, THEIR_TOP).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    #[test]
    fn the_script_is_a_battlefield_with_a_conditional_defend_trigger() {
        assert_eq!(CARD.name, "Ravenbloom Conservatory");
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Defends(Who::You));
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn defending_here_reveals_the_top_card_and_a_spell_goes_to_hand_after_the_hosts_reveal() {
        let mut fixture = held_by_them();
        let item = attack(&mut fixture);
        {
            let ctx = fixture.ctx();
            let parked = &ctx.blob.chain[0];
            assert_eq!(parked.id, item);
            assert_eq!(parked.controller, 1, "the holder defends");
            assert_eq!(parked.status, ItemStatus::Resolving);
            assert_eq!(parked.stage, REVEALED);
            assert_eq!(parked.awaiting, [THEIR_TOP]);
            assert_eq!(
                ctx.card(THEIR_TOP).unwrap().zone,
                Some(fixtures::CHAIN),
                "the revealed card sits in the public zone for one entry"
            );
            assert!(ctx.has_flag(THEIR_TOP, FLAG_REVEALING));
            assert!(ctx.blob.prompt.is_none());
            assert!(ctx
                .blob
                .log
                .iter()
                .any(|line| line == "{seat 1} reveals the top card of their deck"));
        }
        let hand = fixture.ctx().hand_of(1).len();
        arrives(&mut fixture, Face::named("Spark").with_kind(KIND_SPELL));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty(), "the trigger finished");
        assert_eq!(ctx.card(THEIR_TOP).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(THEIR_TOP).unwrap().seat, 1);
        assert_eq!(ctx.hand_of(1).len(), hand + 1);
        assert!(!ctx.has_flag(THEIR_TOP, FLAG_REVEALING));
        assert!(ctx.state_of(THEIR_TOP).is_none(), "no state row lingers");
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{seat 1}} puts {{card {THEIR_TOP}}} into their hand")));
        assert!(
            ctx.blob.showdown.is_some(),
            "the combat is still open around the reveal"
        );
    }

    #[test]
    fn a_unit_on_top_is_recycled_and_a_second_defender_fires_the_trigger_once() {
        let mut fixture = held_by_them();
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SECOND, fixtures::BF1, 1, "Nobody", 1));
        fixture.resolve();
        attack(&mut fixture);
        {
            let ctx = fixture.ctx();
            assert_eq!(
                ctx.blob
                    .chain
                    .iter()
                    .filter(|held| matches!(
                        held.kind,
                        ItemKind::Trigger { source, .. } if source == CONSERVATORY
                    ))
                    .count(),
                1,
                "two defenders, one defend"
            );
        }
        arrives(&mut fixture, Face::named("Jinx").with_kind(KIND_UNIT));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(THEIR_TOP).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert_eq!(
            ctx.table
                .held(fixtures::MAIN_DECK, 1)
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            [THEIR_TOP, 24],
            "recycled under the deck"
        );
    }

    #[test]
    fn an_empty_deck_reveals_nothing_and_the_attacker_never_triggers_the_defence() {
        let mut fixture = held_by_them();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        pass_until_parked(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 1} has no card to reveal"));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 0} reveals the top card of their deck"));
        assert!(ctx.fault.is_none());
    }
}
