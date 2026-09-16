use super::faithful_manufactor::play_recruits;
use super::prelude::{
    activated, card_targets, done, gear, named, paying_with, play, target, Location,
};
use super::{Card, Cost, Filter, Flow, Item, SelfCost, Stage, TargetKind, Timing};
use crate::engine::ctx::Ctx;

pub const RECYCLES: u8 = 4;
pub const CARD_IN_A_TRASH: Filter = Filter::InTrash;
pub const CARDS_IN_TRASHES: crate::cards::TargetSpec = target(
    CARD_IN_A_TRASH,
    0,
    RECYCLES,
    TargetKind::Card,
    "up to four cards in trashes to recycle",
);

fn forge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_recruits(ctx, seat, Location::Base(seat), 1);
    done()
}

fn smelt(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let picked = card_targets(ctx, item);
    let recycled: Vec<u32> = picked
        .into_iter()
        .filter(|card| ctx.in_trash(*card))
        .take(usize::from(RECYCLES))
        .collect();
    for card in &recycled {
        ctx.recycle_to_bottom(*card);
    }
    let seat = item.controller;
    if recycled.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles nothing"));
    } else {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", recycled.len()));
    }
    done()
}

pub static CARD: Card = gear(
    "Forge of the Future",
    &[],
    &[
        play(&[], forge),
        named(
            paying_with(
                activated(Timing::Sorcery, Cost::FREE, &[CARDS_IN_TRASHES], smelt),
                SelfCost::KillSelf,
            ),
            "kill this: recycle up to 4 cards from trashes",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::recruits_of;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const FORGE: u32 = 90;
    const MINE: [u32; 3] = [100, 101, 102];
    const THEIRS: [u32; 2] = [110, 111];

    fn forge_card(zone: u16) -> CardInfo {
        let mut card = fixtures::gear(FORGE, zone, 0, "Forge of the Future", 2);
        card.domain = vec!["Order".into()];
        card
    }

    fn foundry(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(forge_card(zone));
        for id in MINE {
            fixture
                .table
                .cards
                .push(fixtures::spell(id, fixtures::TRASH, 0, "Spent", 1, 0));
        }
        for id in THEIRS {
            fixture
                .table
                .cards
                .push(fixtures::unit(id, fixtures::TRASH, 1, "Fallen", 2));
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FORGE).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn recycled(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card,
                    zone,
                    index: BOTTOM,
                    ..
                } if *zone == fixtures::MAIN_DECK => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_gear_with_a_play_trigger_and_a_kill_this_recycle_ability() {
        assert!(std::ptr::eq(
            script_of("Forge of the Future").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let entry = &CARD.abilities[0];
        assert_eq!(entry.trigger, Trigger::Play);
        assert!(entry.targets.is_empty() && !entry.optional);
        let smelt = &CARD.abilities[1];
        assert_eq!(smelt.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(smelt.cost, Some(Cost::FREE));
        assert_eq!(smelt.self_cost, SelfCost::KillSelf);
        assert_eq!(smelt.targets.len(), 1);
        assert_eq!(smelt.targets[0].filter, CARD_IN_A_TRASH);
        assert_eq!((smelt.targets[0].min, smelt.targets[0].max), (0, RECYCLES));
        assert_eq!(smelt.targets[0].kind, TargetKind::Card);
        assert_eq!(RECYCLES, 4);
    }

    #[test]
    fn playing_the_forge_plays_an_exhausted_recruit_at_your_base() {
        let mut fixture = foundry(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FORGE).unwrap();
        assert!(ctx.on_board(FORGE));
        assert_eq!(ctx.location(FORGE), Some(Location::Base(0)));
        assert!(
            !ctx.card(FORGE).unwrap().exhausted,
            "149.1 · gear enters ready"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == FORGE
        ));
        assert!(
            recruits_of(&ctx, 0).is_empty(),
            "the Recruit waits for the trigger"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(recruits_of(&ctx, 0), [next]);
        assert_eq!(ctx.location(next), Some(Location::Base(0)));
        assert!(ctx.is_token(next));
        assert!(ctx.card(next).unwrap().exhausted);
        assert_eq!(ctx.current_might(next), 1);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == next
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn killing_the_forge_recycles_up_to_four_chosen_cards_from_any_trash_to_the_bottom() {
        let mut fixture = foundry(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let offers = activate::offers(&ctx, 0);
        assert!(offers.iter().any(|offer| offer.enabled
            && offer.label
                == format!("{{card {FORGE}}}: kill this: recycle up to 4 cards from trashes")));
        activate::activate(&mut ctx, 0, FORGE, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, RECYCLES));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 100}",
                "{card 101}",
                "{card 102}",
                "{card 110}",
                "{card 111}",
                "done",
                "skip",
                "cancel"
            ],
            "every trash is offered, the Forge on the board is not"
        );
        assert!(ctx.on_board(FORGE), "358.5 · the kill waits for the plan");
        fixtures::choose(&mut ctx, 0, "{card 100}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 101}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 110}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 111}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["done", "cancel"],
            "four is the cap"
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.on_board(FORGE), "the Forge is killed as the cost");
        assert_eq!(ctx.card(FORGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: false, .. } if *card == FORGE
        )));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(100),
                TargetRef::Card(101),
                TargetRef::Card(110),
                TargetRef::Card(111)
            ]
        );
        assert!(recycled(&ctx).is_empty(), "nothing moves before resolution");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(recycled(&ctx), [100, 101, 110, 111]);
        for card in [100, 101] {
            assert_eq!(ctx.card(card).unwrap().zone, Some(fixtures::MAIN_DECK));
            assert_eq!(ctx.card(card).unwrap().owner, 0);
        }
        for card in [110, 111] {
            assert_eq!(ctx.card(card).unwrap().zone, Some(fixtures::MAIN_DECK));
            assert_eq!(ctx.card(card).unwrap().owner, 1, "each to its owner's deck");
        }
        assert_eq!(ctx.card(102).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(FORGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 4".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_every_pick_still_kills_the_forge_and_a_card_gone_from_the_trash_is_not_recycled() {
        let mut fixture = foundry(fixtures::BASE);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, FORGE, 1).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(!ctx.on_board(FORGE));
        assert!(ctx.blob.chain[0].targets.is_empty());
        resolve_chain(&mut ctx);
        assert!(recycled(&ctx).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} recycles nothing".to_string()));
        drop(ctx);

        let mut fixture = foundry(fixtures::BASE);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, FORGE, 1).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 100}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 101}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.banish(100), "a response banishes one of them");
        resolve_chain(&mut ctx);
        assert_eq!(recycled(&ctx), [101]);
        assert_eq!(ctx.card(100).unwrap().zone, Some(fixtures::BANISHMENT));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_ability_is_refused_off_turn_and_to_the_other_seat() {
        let mut fixture = foundry(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, FORGE, 1),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.on_board(FORGE));
        assert!(ctx.effects.is_empty());
        drop(ctx);

        let mut fixture = foundry(fixtures::BASE);
        fixture.blob.core_mut().unwrap().advance();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        assert_eq!(
            activate::activate(&mut ctx, 0, FORGE, 1),
            Err(Refusal::NotYourTurn),
            "381 · only on the controller's turn"
        );
        assert!(ctx.on_board(FORGE));
        assert!(ctx.blob.queue.is_empty() && ctx.blob.chain.is_empty());
    }
}
