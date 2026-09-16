use super::ember_monk::spell_played_from_hidden;
use super::prelude::{an_enemy_unit, card_target, deal, done, on_you_play_card, ready, unit, when};
use super::{Card, Flow, Item, Source, Stage, TargetSpec};
use crate::engine::ctx::{Ctx, Event};
use crate::state::Origin;

pub const DAMAGE: u8 = 2;

pub const MARK: TargetSpec = an_enemy_unit("an enemy unit to deal 2 to");

fn from_hidden(ctx: &Ctx, event: &Event, _: Source) -> bool {
    match event {
        Event::Played {
            origin: Origin::Facedown { .. },
            ..
        } => true,
        Event::PlayedSpell { .. } => spell_played_from_hidden(ctx, event),
        _ => false,
    }
}

fn shunpo(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    match card_target(ctx, item, 0) {
        Some(unit) => {
            deal(ctx, item, unit, DAMAGE);
            ctx.narrate(format!("{{card {me}}} deals {DAMAGE} to {{card {unit}}}"));
        }
        None => ctx.narrate(format!("{{card {me}}}: the target is gone")),
    }
    done()
}

pub fn when_you_hide_a_card_until_ctx_hide_raises_a_hidden_event(
    ctx: &mut Ctx,
    item: &Item,
    _: Stage,
) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    } else {
        ctx.narrate(format!("{{card {me}}} is already ready"));
    }
    done()
}

pub static CARD: Card = unit(
    "Katarina - Reckless",
    &[],
    &[when(on_you_play_card(&[MARK], shunpo), from_hidden)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::ENEMY_UNIT;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{hide, play as play_engine, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const KATARINA: u32 = 90;
    const HIDDEN_UNIT: u32 = 91;
    const THEIR_HIDDEN_UNIT: u32 = 92;
    const THEIR_BRUTE: u32 = 93;

    fn katarina(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            exhausted,
            domain: vec!["Fury".into()],
            ..fixtures::unit(KATARINA, zone, seat, "Katarina - Reckless", 5)
        }
    }

    fn noxus() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(katarina(fixtures::BASE, 0, true));
        fixture
            .table
            .cards
            .push(fixtures::unit(HIDDEN_UNIT, fixtures::BF1, 0, "Ally", 2));
        fixture.table.cards.push(fixtures::unit(
            THEIR_HIDDEN_UNIT,
            fixtures::BF2,
            1,
            "Foe",
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.card_state_mut(HIDDEN_UNIT).hidden_at = Some(fixtures::BF1);
        fixture.blob.card_state_mut(THEIR_HIDDEN_UNIT).hidden_at = Some(fixtures::BF2);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(KATARINA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn play_facedown(ctx: &mut Ctx, seat: u8, card: u32, zone: u16) {
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play_engine::begin(
            ctx,
            seat,
            card,
            Origin::Facedown { zone },
            Some(Location::Battlefield(zone)),
        )
        .unwrap();
        settle(ctx).unwrap();
    }

    fn trigger_item(id: u16) -> Item {
        Item::new(
            id,
            ItemKind::Trigger {
                source: KATARINA,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_script_watches_your_plays_from_face_down_and_names_the_hide_watcher_as_a_seam() {
        assert!(std::ptr::eq(
            script_of("Katarina - Reckless").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Katarina - Reckless");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let strike = &CARD.abilities[0];
        assert_eq!(strike.trigger, Trigger::YouPlayCard);
        assert!(strike.condition.is_some(), "from face down");
        assert!(!strike.optional);
        assert_eq!(strike.targets, [MARK]);
        assert_eq!((MARK.min, MARK.max), (1, 1));
        assert_eq!(MARK.kind, TargetKind::Card);
        assert_eq!(MARK.filter, ENEMY_UNIT);
        assert_eq!(DAMAGE, 2);
    }

    #[test]
    fn a_unit_you_play_from_face_down_asks_for_an_enemy_unit_and_the_pick_takes_two() {
        let mut fixture = noxus();
        let mut ctx = fixture.ctx();
        play_facedown(&mut ctx, 0, HIDDEN_UNIT, fixtures::BF1);
        assert_eq!(
            ctx.location(HIDDEN_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let item = ctx
            .blob
            .queue
            .iter()
            .find(|pending| {
                matches!(pending.item.kind, ItemKind::Trigger { source, index: 0 } if source == KATARINA)
            })
            .map(|pending| pending.item.id)
            .expect("her trigger is pending");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {THEIR_BRUTE}}}"),
            ],
            "every enemy unit on the board · the opponent's facedown card is not a unit yet"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[HIDDEN_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the unit just played is yours"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == KATARINA
        ));
        assert_eq!(
            ctx.blob.chain.last().unwrap().targets,
            [TargetRef::Card(THEIR_BRUTE)]
        );
        assert_eq!(
            ctx.damage_on(THEIR_BRUTE),
            0,
            "the damage waits for the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(THEIR_BRUTE), i32::from(DAMAGE));
        assert!(ctx.on_board(THEIR_BRUTE));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {KATARINA}}} deals 2 to {{card {THEIR_BRUTE}}}"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_card_played_from_hand_and_an_opponents_hidden_play_fire_nothing() {
        let mut fixture = noxus();
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::HAND_UNIT, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            fixtures::HAND_UNIT,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(
            ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty(),
            "a play from hand is not from face down"
        );
        assert!(ctx.blob.prompt.is_none());
        play_facedown(&mut ctx, 1, THEIR_HIDDEN_UNIT, fixtures::BF2);
        assert_eq!(
            ctx.location(THEIR_HIDDEN_UNIT),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(
            ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty(),
            "the opponent's plays are not yours"
        );
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_hide_watcher_readies_her_and_says_so_when_she_was_already_ready() {
        let mut fixture = noxus();
        let mut ctx = fixture.ctx();
        assert!(ctx.card(KATARINA).unwrap().exhausted);
        assert_eq!(
            when_you_hide_a_card_until_ctx_hide_raises_a_hidden_event(
                &mut ctx,
                &trigger_item(7),
                Stage(0)
            ),
            Flow::Done
        );
        assert!(!ctx.card(KATARINA).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == KATARINA
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {KATARINA}}} readies")));
        assert_eq!(
            when_you_hide_a_card_until_ctx_hide_raises_a_hidden_event(
                &mut ctx,
                &trigger_item(8),
                Stage(0)
            ),
            Flow::Done
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {KATARINA}}} is already ready")));
        assert!(ctx.bounce(KATARINA));
        let readied = ctx.events.len();
        when_you_hide_a_card_until_ctx_hide_raises_a_hidden_event(
            &mut ctx,
            &trigger_item(9),
            Stage(0),
        );
        assert_eq!(
            ctx.events.len(),
            readied,
            "off the board she readies nothing"
        );
        assert!(ctx.fault.is_none());
    }

    fn bare_noxus() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(katarina(fixtures::BASE, 0, true));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn today_hiding_a_card_readies_nothing() {
        let mut fixture = bare_noxus();
        let mut ctx = fixture.ctx();
        assert!(hide::legal(&ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1).is_ok());
        hide::hide(&mut ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.is_facedown(fixtures::HAND_HIDDEN));
        assert!(
            ctx.blob.chain.is_empty(),
            "no trigger reaches the chain yet"
        );
        assert!(ctx.card(KATARINA).unwrap().exhausted);
    }

    #[test]
    #[ignore = "engine gap · missing trigger subject: hide::hide raises no Hidden event and Trigger has no you-hide-a-card variant; when_you_hide_a_card_until_ctx_hide_raises_a_hidden_event is the body the owed ability runs, and with it hiding a card readies her"]
    fn hiding_a_card_readies_her_once_the_trigger_resolves() {
        let mut fixture = bare_noxus();
        let mut ctx = fixture.ctx();
        hide::hide(&mut ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.card(KATARINA).unwrap().exhausted,
            "the ready waits for the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(KATARINA).unwrap().exhausted);
    }
}
