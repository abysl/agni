use super::prelude::{
    at_battlefield, done, on_opponent_plays_unit, stun_and_lock, trigger_subject, unit, when,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::FLAG_NO_MOVE_BY_OWNER;

const DEFLECT: u8 = 1;

fn subject(ctx: &Ctx, item: &Item) -> Option<u32> {
    let played = trigger_subject(item)?;
    let enemy = ctx.is_unit(played)
        && ctx.on_board(played)
        && ctx.controller(played) != item.controller
        && !ctx.is_pending_play(played);
    enemy.then_some(played)
}

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(played) = subject(ctx, item) {
        let locked = ctx.has_flag(played, FLAG_NO_MOVE_BY_OWNER);
        stun_and_lock(ctx, played);
        if !locked {
            ctx.narrate(format!("{{card {played}}} can't move this turn"));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Vex - Apathetic",
    &[Keyword::Deflect(DEFLECT)],
    &[when(
        on_opponent_plays_unit(&[], resolve),
        |ctx, _, source| at_battlefield(ctx, source.card),
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_card, play, spell, MOVABLE_UNIT};
    use crate::cards::{Trigger, KIND_GEAR, KIND_UNIT};
    use crate::engine::ctx::{EntryMove, Event, Location, ANNOTATION_STUNNED};
    use crate::engine::ctx::{MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{Intent, Reason};
    use crate::engine::{legal, play as play_engine, priority, prompts, settle, targets, triggers};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, FLAG_STUNNED};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const VEX: u32 = 90;
    const PLAYED: u32 = fixtures::HAND_UNIT;
    const ZAP: u32 = 92;

    static SPARK: Card = spell(
        "Spark",
        &[],
        &[play(&[a_card(MOVABLE_UNIT, "a unit")], |_, _, _| {
            Flow::Done
        })],
    );

    fn vex(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            might: Some(4),
            domain: vec!["Chaos".into()],
            ..fixtures::card(VEX, zone, seat, "Vex - Apathetic", "Unit")
        }
    }

    fn watching_from(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(vex(zone, 1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(VEX).unwrap(), &CARD));
        fixture
    }

    fn watching() -> Fixture {
        watching_from(fixtures::BF2)
    }

    fn zapping(runes: &[u32]) -> Fixture {
        let mut fixture = watching();
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1 || runes.contains(&card.id));
        fixture
            .table
            .cards
            .push(fixtures::spell(ZAP, fixtures::HAND, 0, "Spark", 1, 1));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ZAP, &SPARK);
        fixture
    }

    fn play_unit(ctx: &mut Ctx, seat: u8, card: u32, to: u16) -> Result<(), Refusal> {
        let entry = EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: Some(to),
            to_seat: seat,
            index: TOP,
            hidden: false,
        };
        let intent = legal::classify(ctx, seat, &entry)?;
        let Intent::Play {
            origin, location, ..
        } = intent
        else {
            panic!("a unit from hand is a play: {intent:?}");
        };
        ctx.table
            .apply_entry(&fixtures::move_action(card, to, seat), seat)
            .unwrap();
        play_engine::begin(ctx, seat, card, origin, location)?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn play_spell(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        let entry = EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        legal::classify(ctx, seat, &entry)?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play_engine::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn both_pass(ctx: &mut Ctx) {
        let first = priority::holder(ctx).unwrap_or(0);
        priority::pass(ctx, first).unwrap();
        if let Some(next) = priority::holder(ctx) {
            priority::pass(ctx, next).unwrap();
        }
    }

    fn played_by(seat: u8, kind: &str) -> Event {
        Event::Played {
            card: PLAYED,
            controller: seat,
            kind: kind.into(),
            origin: Origin::Hand,
            paid_additional: false,
        }
    }

    fn watchers(ctx: &Ctx, event: Event) -> Vec<(u8, u32, u8)> {
        triggers::find(ctx, &event)
            .into_iter()
            .map(|held| (held.controller, held.source, held.index))
            .collect()
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn item_of(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, .. }) => item,
            why => panic!("a target prompt was expected, got {why:?}"),
        }
    }

    #[test]
    fn the_script_is_a_deflecting_unit_with_one_conditional_trigger() {
        assert_eq!(CARD.name, "Vex - Apathetic");
        assert!(CARD.has_keyword(Keyword::Deflect(DEFLECT)));
        assert_eq!(CARD.keywords, &[Keyword::Deflect(1)]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::OpponentPlaysUnit);
        assert!(ability.targets.is_empty(), "the played unit is not chosen");
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(
            ability.condition.is_some(),
            "only while Vex is at a battlefield"
        );
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(super::super::script_of("Vex - Apathetic").is_some());
    }

    #[test]
    fn an_opponents_unit_is_stunned_and_locked_when_the_trigger_resolves() {
        let mut fixture = watching();
        let mut ctx = fixture.ctx();
        play_unit(&mut ctx, 0, PLAYED, fixtures::BASE).unwrap();
        assert_eq!(ctx.location(PLAYED), Some(Location::Base(0)));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger is on the chain");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == VEX
        ));
        assert_eq!(ctx.blob.chain[0].controller, 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert!(ctx.blob.prompt.is_none(), "Vex asks nothing");
        assert_eq!(priority::holder(&ctx), Some(1));
        assert!(!ctx.is_stunned(PLAYED), "nothing until it resolves");
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.is_stunned(PLAYED));
        assert!(ctx.has_flag(PLAYED, FLAG_NO_MOVE_BY_OWNER));
        assert_eq!(ctx.current_might(PLAYED), 2, "a stun is not a debuff");
        assert_eq!(ctx.combat_might(PLAYED), 0, "410.1.b");
        assert!(ctx.effects.contains(&Effect::Annotate {
            card: PLAYED,
            key: ANNOTATION_STUNNED.into(),
            value: Some(vec![1])
        }));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PLAYED}}} is stunned")));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PLAYED}}} can't move this turn")));
        assert!(!ctx.is_stunned(fixtures::VI), "only the played unit");
        assert!(!ctx.has_flag(fixtures::VI, FLAG_NO_MOVE_BY_OWNER));
        assert!(!ctx.is_stunned(VEX));
    }

    #[test]
    fn the_locked_unit_refuses_its_owners_march_and_leaves_their_move_effects() {
        let mut fixture = watching();
        let mut ctx = fixture.ctx();
        play_unit(&mut ctx, 0, PLAYED, fixtures::BASE).unwrap();
        both_pass(&mut ctx);
        assert!(ctx.ready(PLAYED), "the lock, not the exhaustion, is tested");
        let march = EntryMove {
            card: PLAYED,
            from: Some(fixtures::BASE),
            from_seat: 0,
            to: Some(fixtures::BF1),
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &march),
            Err(Refusal::Illegal(Reason::Locked)),
            "they can't move it this turn"
        );
        let spec = a_card(MOVABLE_UNIT, "a unit");
        let mine = ChainItem::new(
            60,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        let theirs = ChainItem::new(
            61,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            Origin::Hand,
        );
        let owners = targets::candidates(&ctx, &mine, &spec);
        let charmers = targets::candidates(&ctx, &theirs, &spec);
        assert!(
            !owners.contains(&crate::state::TargetRef::Card(PLAYED)),
            "no candidate of its owner's move effects: {owners:?}"
        );
        assert!(
            owners.contains(&crate::state::TargetRef::Card(fixtures::VI)),
            "the rest of the board is untouched: {owners:?}"
        );
        assert!(
            charmers.contains(&crate::state::TargetRef::Card(PLAYED)),
            "Charm by the opponent still moves it: {charmers:?}"
        );
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        crate::engine::expiry::at_expiration(&mut ctx);
        assert!(
            !ctx.has_flag(PLAYED, FLAG_NO_MOVE_BY_OWNER),
            "this turn only"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &march).map(|intent| matches!(
                intent,
                Intent::StandardMove { unit, .. } if unit == PLAYED
            )),
            Ok(true)
        );
    }

    #[test]
    fn a_second_unit_played_in_the_same_turn_is_the_one_the_second_trigger_stuns() {
        let mut fixture = watching();
        let second = 91;
        fixture
            .table
            .cards
            .push(fixtures::unit(second, fixtures::HAND, 0, "Ekko", 2));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_unit(&mut ctx, 0, PLAYED, fixtures::BASE).unwrap();
        both_pass(&mut ctx);
        assert!(ctx.is_stunned(PLAYED));
        play_unit(&mut ctx, 0, second, fixtures::BASE).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        both_pass(&mut ctx);
        assert!(ctx.is_stunned(second));
        assert!(ctx.has_flag(second, FLAG_NO_MOVE_BY_OWNER));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {second}}} is stunned")));
        assert!(ctx.is_stunned(PLAYED), "the first one stays stunned");
        assert!(ctx.has_flag(PLAYED, FLAG_NO_MOVE_BY_OWNER));
    }

    #[test]
    fn an_opponents_spawned_token_is_the_subject_the_trigger_stuns() {
        let mut fixture = watching();
        let mut ctx = fixture.ctx();
        let sprite = ctx
            .spawn(
                0,
                crate::cards::prelude::Token::Sprite,
                Location::Base(0),
                true,
            )
            .expect("seat 0 mints a Sprite");
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob
                .chain
                .iter()
                .map(|held| held.kind.source())
                .collect::<Vec<_>>(),
            [VEX]
        );
        both_pass(&mut ctx);
        assert!(ctx.is_stunned(sprite));
        assert!(ctx.has_flag(sprite, FLAG_NO_MOVE_BY_OWNER));
    }

    #[test]
    fn a_bystander_that_entered_earlier_is_not_the_subject_of_a_later_trigger() {
        let mut fixture = watching_from(fixtures::BASE);
        let second = 91;
        fixture
            .table
            .cards
            .push(fixtures::unit(second, fixtures::HAND, 0, "Ekko", 2));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Fury", false));
        fixture.resolve();
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        let mut ctx = fixture.ctx();
        play_unit(&mut ctx, 0, PLAYED, fixtures::BASE).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vex is still in its base");
        assert_eq!(
            ctx.move_unit(VEX, Location::Battlefield(fixtures::BF2), MoveCause::Effect),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        play_unit(&mut ctx, 0, second, fixtures::BASE).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "now Vex is watching");
        both_pass(&mut ctx);
        assert!(ctx.is_stunned(second));
        assert!(ctx.has_flag(second, FLAG_NO_MOVE_BY_OWNER));
        assert!(
            !ctx.is_stunned(PLAYED),
            "the unit played before Vex arrived is a bystander"
        );
        assert!(!ctx.has_flag(PLAYED, FLAG_NO_MOVE_BY_OWNER));
    }

    #[test]
    fn vex_watches_only_opponents_units_and_only_from_a_battlefield() {
        let mut fixture = watching();
        let ctx = fixture.ctx();
        assert_eq!(
            watchers(&ctx, played_by(0, KIND_UNIT)),
            [(1, VEX, 0)],
            "an opponent's unit while Vex is at a battlefield"
        );
        assert!(
            watchers(&ctx, played_by(1, KIND_UNIT)).is_empty(),
            "its own controller's unit never triggers it"
        );
        assert!(
            watchers(&ctx, played_by(0, KIND_GEAR)).is_empty(),
            "gear is not a unit"
        );
        drop(ctx);
        let mut homebound = watching_from(fixtures::BASE);
        let mut ctx = homebound.ctx();
        assert!(
            watchers(&ctx, played_by(0, KIND_UNIT)).is_empty(),
            "Vex sits in its base"
        );
        play_unit(&mut ctx, 0, PLAYED, fixtures::BASE).unwrap();
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_stunned(PLAYED));
        assert!(!ctx.has_flag(PLAYED, FLAG_NO_MOVE_BY_OWNER));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn an_already_stunned_and_locked_unit_leaves_the_trigger_with_nothing_to_do() {
        let mut fixture = watching();
        let mut ctx = fixture.ctx();
        play_unit(&mut ctx, 0, PLAYED, fixtures::BASE).unwrap();
        ctx.blob.card_state_mut(PLAYED).set(FLAG_STUNNED, true);
        ctx.blob
            .card_state_mut(PLAYED)
            .set(FLAG_NO_MOVE_BY_OWNER, true);
        let before = ctx.blob.log.len();
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(PLAYED), "410.1.a.1: it stays stunned");
        assert!(!ctx.blob.log[before..]
            .iter()
            .any(|line| line.ends_with("is stunned")));
        assert!(!ctx.blob.log[before..]
            .iter()
            .any(|line| line.ends_with("can't move this turn")));
    }

    #[test]
    fn deflect_keeps_vex_out_of_a_spell_whose_rune_pool_cannot_pay_the_rainbow() {
        let mut fixture = zapping(&[41]);
        let mut ctx = fixture.ctx();
        play_spell(&mut ctx, 0, ZAP).unwrap();
        let item = item_of(&ctx);
        let offered = labels(&ctx);
        assert!(
            !offered.contains(&format!("{{card {VEX}}}")),
            "one rune pays the spell but not the deflect: {offered:?}"
        );
        assert!(
            offered.contains(&format!("{{card {}}}", fixtures::VI)),
            "their own units cost nothing extra: {offered:?}"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[VEX]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "735.1.c: the rainbow is a mandatory additional cost"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        drop(ctx);
        let mut richer = zapping(&[41, 42]);
        let mut ctx = richer.ctx();
        play_spell(&mut ctx, 0, ZAP).unwrap();
        let offered = labels(&ctx);
        assert!(
            offered.contains(&format!("{{card {VEX}}}")),
            "a second rune buys the deflect: {offered:?}"
        );
    }
}
