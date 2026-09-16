use super::prelude::{alone_there, done, gear, might_this_turn, trigger_subject, triggered, when};
use super::{Ability, Card, Flow, Item, Source, Stage, Trigger, Who};
use crate::engine::ctx::{Ctx, Event};

pub const BONUS: i16 = 1;

fn subject_alone(ctx: &Ctx, event: &Event, _: Source) -> bool {
    match event {
        Event::Attacks { card } | Event::Defends { card } => alone_there(ctx, *card),
        _ => false,
    }
}

fn foresee(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = trigger_subject(item) {
        if ctx.is_unit(unit) && ctx.on_board(unit) {
            might_this_turn(ctx, item, unit, BONUS, None);
        }
    }
    done()
}

const ON_ATTACK: Ability = when(
    triggered(Trigger::Attacks(Who::Friendly), &[], foresee),
    subject_alone,
);

const ON_DEFEND: Ability = when(
    triggered(Trigger::Defends(Who::Friendly), &[], foresee),
    subject_alone,
);

pub static CARD: Card = gear("Mask of Foresight", &[], &[ON_ATTACK, ON_DEFEND]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{
        act, cleanup, legal, priority, prompts, resume, settle, showdown, triggers,
    };
    use crate::state::{GameBlob, ItemKind, Mode, PromptWhy, TargetRef};
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::{CardInfo, Snapshot};

    const MASK: u32 = 90;
    const RAIDER: u32 = 91;
    const ALLY: u32 = 92;
    const SMOKE: u32 = 93;
    const SPRITE: u32 = 94;

    fn mask(seat: u8) -> CardInfo {
        let mut card = fixtures::gear(MASK, fixtures::BASE, seat, "Mask of Foresight", 2);
        card.domain = vec!["Calm".into()];
        card
    }

    fn masked() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.cards.push(mask(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(MASK).unwrap(), &CARD));
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

    fn resolve_chain_roundtrip(fixture: &mut Fixture) {
        for _ in 0..4 {
            let mut ctx = fixture.ctx();
            if ctx.blob.chain.is_empty() {
                return;
            }
            let Some(holder) = priority::holder(&ctx) else {
                return;
            };
            priority::pass(&mut ctx, holder).unwrap();
            let table = ctx.table.clone();
            let state = ctx.blob.encode();
            drop(ctx);
            reopen(fixture, table, state);
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

    fn smoke_entry(ctx: &Ctx) -> EntryMove {
        EntryMove {
            card: SMOKE,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn choose_target(ctx: &mut Ctx, card: u32) {
        let prompt = ctx.blob.prompt.as_ref().map(|prompt| prompt.id).unwrap();
        let option = prompts::offered(ctx)
            .iter()
            .position(|option| option.card == Some(card))
            .unwrap() as u16;
        if let Some(answered) = prompts::answer(ctx, 0, Pick { prompt, option }).unwrap() {
            resume(ctx, &answered).unwrap();
        }
        settle(ctx).unwrap();
    }

    fn choose_trigger(ctx: &mut Ctx, item: u16) {
        let prompt = ctx.blob.prompt.as_ref().map(|prompt| prompt.id).unwrap();
        let option = prompts::offered(ctx)
            .iter()
            .position(|option| option.answer == Answer::Item(item))
            .unwrap() as u16;
        if let Some(answered) = prompts::answer(ctx, 0, Pick { prompt, option }).unwrap() {
            resume(ctx, &answered).unwrap();
        }
        settle(ctx).unwrap();
    }

    fn reopen(fixture: &mut Fixture, table: Snapshot, state: Vec<u8>) {
        fixture.table = table;
        fixture.blob = GameBlob::decode(&state).unwrap();
        fixture.resolve();
    }

    #[test]
    fn the_script_is_a_gear_with_an_attack_and_a_defend_trigger_conditioned_on_alone() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Mask of Foresight").unwrap(),
            &CARD
        ));
        assert!(!CARD.is_equipment(), "it is worn by nobody");
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Attacks(Who::Friendly));
        assert_eq!(CARD.abilities[1].trigger, Trigger::Defends(Who::Friendly));
        for ability in CARD.abilities {
            assert!(ability.condition.is_some());
            assert!(ability.targets.is_empty());
            assert!(!ability.optional);
        }
    }

    #[test]
    fn a_friendly_unit_attacking_alone_gets_plus_one_this_turn_when_the_trigger_resolves() {
        let mut fixture = masked();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        contest(&mut fixture, 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        open_combat(&mut ctx);
        assert!(ctx.is_attacker(fixtures::VI));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "383.4.e · the trigger forms the initial chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MASK
        ));
        assert_eq!(
            ctx.blob.chain[0].subject,
            Some(TargetRef::Card(fixtures::VI))
        );
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "nothing until it resolves"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        finish_combat(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 4 might vs defenders 2 might"));
        assert!(!ctx.on_board(RAIDER));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "this turn, not this combat"
        );
        let turn = ctx.turn();
        ctx.expire(crate::state::Expiry::EndOfTurn(turn));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_friendly_unit_defending_alone_gets_it_too_and_only_once_per_designation() {
        let mut fixture = masked();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        contest(&mut fixture, 1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert!(ctx.is_defender(fixtures::VI));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == MASK
        ));
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(!ctx.mark_defender(fixtures::VI), "already designated");
        triggers::collect(&mut ctx);
        assert!(
            ctx.blob.queue.is_empty(),
            "383.4.f.2.b · evaluated once, when the unit first gains its designation"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_friendly_attackers_an_enemy_alone_and_the_other_seats_mask_fire_nothing() {
        let mut fixture = masked();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        contest(&mut fixture, 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert!(ctx.is_attacker(fixtures::VI) && ctx.is_attacker(ALLY));
        assert!(ctx.blob.chain.is_empty(), "neither attacks alone");
        assert!(
            ctx.is_defender(RAIDER),
            "the raider defends alone, but it is not friendly to the mask"
        );
        assert_eq!(ctx.current_might(RAIDER), 2);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        drop(ctx);

        let mut fixture = masked();
        fixture.table.card_mut(MASK).unwrap().owner = 1;
        fixture.table.card_mut(MASK).unwrap().seat = 1;
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        contest(&mut fixture, 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the other seat's mask sees its own lone defender"
        );
        assert_eq!(ctx.blob.chain[0].controller, 1);
        assert_eq!(ctx.blob.chain[0].subject, Some(TargetRef::Card(RAIDER)));
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(RAIDER), 3);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "Vi attacked alone against the wrong mask"
        );
        drop(ctx);

        let mut fixture = masked();
        fixture.blob.card_state_mut(MASK).attached_to = Some(fixtures::VI);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        contest(&mut fixture, 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "134.4 · an attached gear's rules text is inactive"
        );
        assert_eq!(
            ctx.location(MASK),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }

    #[test]
    fn smoke_swaps_a_lone_defender_before_lillias_sprite_and_masks_only_the_lone_sprite() {
        let mut fixture = masked();
        fixture
            .table
            .card_mut(fixtures::CHAMPION_CARD)
            .unwrap()
            .zone = Some(fixtures::BF1);
        fixture
            .table
            .card_mut(fixtures::CHAMPION_CARD)
            .unwrap()
            .exhausted = true;
        fixture.table.cards.push(fixtures::spell(
            SMOKE,
            fixtures::HAND,
            0,
            "Smoke and Mirrors",
            2,
            0,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(SPRITE, fixtures::BASE, 0, "Sprite", 3));
        fixture.table.tokens.push(SPRITE);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        contest(&mut fixture, 1);
        fixture.resolve();

        let mut ctx = fixture.ctx();
        open_combat(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        let mut ctx = fixture.ctx();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(fixtures::CHAMPION_CARD), 4);
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().focus(), 1);
        showdown::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().focus(), 0);
        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        let mut ctx = fixture.ctx();

        let intent = legal::classify(&ctx, 0, &smoke_entry(&ctx)).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        let mut ctx = fixture.ctx();
        choose_target(&mut ctx, fixtures::CHAMPION_CARD);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 1 }));
        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        let mut ctx = fixture.ctx();
        choose_target(&mut ctx, SPRITE);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].kind, ItemKind::Spell { card: SMOKE });

        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        let mut ctx = fixture.ctx();
        priority::pass(&mut ctx, 0).unwrap();
        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        let mut ctx = fixture.ctx();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.location(fixtures::CHAMPION_CARD),
            Some(Location::Base(0))
        );
        assert_eq!(
            ctx.location(SPRITE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.current_might(fixtures::CHAMPION_CARD), 4);
        assert_eq!(ctx.blob.chain.len(), 0);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "A3 compatibility path: this engine prompt is not legal freedom under 383.2.c"
        );
        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        let mut ctx = fixture.ctx();
        choose_trigger(&mut ctx, 3);
        assert_eq!(ctx.blob.chain.len(), 2);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == fixtures::CHAMPION_CARD
        ));
        assert!(matches!(
            ctx.blob.chain[1].kind,
            ItemKind::Trigger { source, index: 1 } if source == MASK
        ));
        assert_eq!(ctx.current_might(SPRITE), 3);

        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        let mut ctx = fixture.ctx();
        priority::pass(&mut ctx, 0).unwrap();
        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        let mut ctx = fixture.ctx();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(SPRITE), 4);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == fixtures::CHAMPION_CARD
        ));

        let table = ctx.table.clone();
        let state = ctx.blob.encode();
        drop(ctx);
        reopen(&mut fixture, table, state);
        resolve_chain_roundtrip(&mut fixture);
        let ctx = fixture.ctx();
        let sprites: Vec<u32> = ctx
            .table
            .cards
            .iter()
            .filter(|card| card.name == "Sprite" && card.owner == 0)
            .map(|card| card.id)
            .collect();
        assert_eq!(sprites.len(), 2);
        assert_eq!(ctx.current_might(SPRITE), 4);
        let second = sprites.into_iter().find(|card| *card != SPRITE).unwrap();
        assert_eq!(
            ctx.location(SPRITE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(second),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.current_might(second), 3);
        assert_eq!(
            ctx.current_might(SPRITE) + ctx.current_might(second),
            7,
            "323.2, 383.2, 383.4.f, 359.3.f.3"
        );
        assert_eq!(ctx.current_might(fixtures::CHAMPION_CARD), 4);
        assert_eq!(
            ctx.location(fixtures::CHAMPION_CARD),
            Some(Location::Base(0))
        );
        assert!(ctx.fault.is_none());
    }
}
