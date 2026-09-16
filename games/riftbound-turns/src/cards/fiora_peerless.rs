use super::prelude::{alone_there, done, enemy_alone_at, on_attack, on_defend, unit, when};
use super::{Ability, Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::state::Expiry;

fn one_on_one(ctx: &Ctx, _: &Event, source: Source) -> bool {
    alone_there(ctx, source.card) && enemy_alone_at(ctx, source.card)
}

fn riposte(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.is_unit(me) || !ctx.on_board(me) {
        return done();
    }
    let current = ctx.current_might(me);
    let Ok(delta) = i16::try_from(current) else {
        return done();
    };
    if delta > 0 {
        ctx.might(me, delta, Expiry::CombatEnd, None, item.id);
        ctx.narrate(format!("{{card {me}}} doubles her Might this combat"));
    }
    done()
}

const ON_ATTACK: Ability = when(on_attack(&[], riposte), one_on_one);
const ON_DEFEND: Ability = when(on_defend(&[], riposte), one_on_one);

pub static CARD: Card = unit("Fiora - Peerless", &[], &[ON_ATTACK, ON_DEFEND]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, showdown};
    use crate::state::{GameBlob, ItemKind, Mode};
    use agni_plugin_sdk::table::CardInfo;

    const FIORA: u32 = 90;
    const RAIDER: u32 = 91;
    const SECOND: u32 = 92;

    fn fiora(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            might: Some(3),
            domain: vec!["Body".into()],
            ..fixtures::card(FIORA, zone, seat, "Fiora - Peerless", "Unit")
        }
    }

    fn duel() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::VI, fixtures::SPRITE].contains(&card.id));
        fixture.table.cards.push(fiora(fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 4));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FIORA).unwrap(), &CARD));
        fixture
    }

    fn contest(fixture: &mut Fixture, attacker: u8) {
        let holder = 1 - attacker;
        fixture.blob.set_holder(fixtures::BF1, Some(holder));
        fixture.blob.set_contested(fixtures::BF1, Some(attacker));
    }

    fn open_combat(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        crate::engine::settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        for _ in 0..4 {
            if ctx.blob.chain.is_empty() {
                return;
            }
            let Some(holder) = priority::holder(ctx) else {
                return;
            };
            priority::pass(ctx, holder).unwrap();
        }
    }

    fn finish_combat(ctx: &mut Ctx) {
        for _ in 0..4 {
            let Some(held) = ctx.blob.showdown.clone() else {
                break;
            };
            showdown::pass(ctx, held.focus()).unwrap();
        }
        assert!(ctx.blob.showdown.is_none());
    }

    #[test]
    fn the_script_is_a_unit_with_an_attack_and_a_defend_trigger_conditioned_on_one_on_one() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Fiora - Peerless").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Attacks(Who::Me));
        assert_eq!(CARD.abilities[1].trigger, Trigger::Defends(Who::Me));
        for ability in CARD.abilities {
            assert!(ability.condition.is_some());
            assert!(ability.targets.is_empty());
        }
    }

    #[test]
    fn attacking_one_on_one_doubles_her_might_until_the_combat_ends() {
        let mut fixture = duel();
        contest(&mut fixture, 0);
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert!(ctx.is_attacker(FIORA));
        assert_eq!(ctx.blob.chain.len(), 1, "740.2.b · one on one");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == FIORA
        ));
        assert_eq!(ctx.current_might(FIORA), 3, "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(FIORA), 6, "432 · doubling is additive");
        assert_eq!(
            ctx.state_of(FIORA).unwrap().might[0].until,
            Expiry::CombatEnd
        );
        finish_combat(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 6 might vs defenders 4 might"));
        assert!(!ctx.on_board(RAIDER), "six damage kills the 4-Might raider");
        assert!(ctx.on_board(FIORA), "four damage does not kill her at six");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(
            ctx.current_might(FIORA),
            3,
            "466.7 · the doubling ends with the combat"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn defending_one_on_one_doubles_her_might_too() {
        let mut fixture = duel();
        contest(&mut fixture, 1);
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert!(ctx.is_defender(FIORA));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == FIORA
        ));
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(FIORA), 6);
        finish_combat(&mut ctx);
        assert!(!ctx.on_board(RAIDER));
        assert!(ctx.on_board(FIORA));
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "she keeps the battlefield"
        );
        assert_eq!(ctx.current_might(FIORA), 3);
    }

    #[test]
    fn against_two_enemies_or_beside_a_friend_she_stays_at_three() {
        let mut fixture = duel();
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 1, "Second", 1));
        contest(&mut fixture, 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert!(ctx.is_attacker(FIORA));
        assert!(ctx.blob.chain.is_empty(), "two enemies is not one on one");
        assert_eq!(ctx.current_might(FIORA), 3);
        drop(ctx);

        let mut fixture = duel();
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 0, "Friend", 1));
        contest(&mut fixture, 1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert!(ctx.is_defender(FIORA) && ctx.is_defender(SECOND));
        assert!(ctx.blob.chain.is_empty(), "740.2.a · she is not alone");
        assert_eq!(ctx.current_might(FIORA), 3);
        drop(ctx);

        let mut fixture = duel();
        fixture.table.card_mut(RAIDER).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(RAIDER).unwrap().seat = 1;
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(
            ctx.blob.showdown.as_ref().is_some_and(|held| !held.combat),
            "nobody to fight is a showdown, not a combat"
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert_eq!(ctx.current_might(FIORA), 3);
    }
}
