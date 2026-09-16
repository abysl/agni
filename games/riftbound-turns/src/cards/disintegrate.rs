use super::prelude::{
    a_unit_at_a_battlefield, card_target, deal, done, draw, play, spell, triggered,
};
use super::{Card, Flow, Item, Keyword, Stage, Trigger};
use crate::engine::cleanup;
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, When};

pub const DAMAGE: u8 = 3;
const DRAWS: usize = 1;
const DRAW_AFTER_KILL: u8 = 1;

fn disintegrate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if deal(ctx, item, unit, DAMAGE) {
        ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        ctx.delay(
            When::AfterKillsBy(item.id),
            item.kind.source(),
            item.controller,
            DRAW_AFTER_KILL,
            vec![unit],
        );
    }
    done()
}

fn watched_unit(item: &Item) -> Option<u32> {
    match item.targets.first()? {
        TargetRef::Card(unit) => Some(*unit),
        _ => None,
    }
}

fn draw_if_it_died(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = watched_unit(item) else {
        return done();
    };
    if cleanup::kills_of(item) == 0 || ctx.on_board(unit) {
        return done();
    }
    let seat = item.controller;
    ctx.narrate(format!("{{card {unit}}} died · {{seat {seat}}} draws 1"));
    draw(ctx, seat, DRAWS);
    done()
}

pub static CARD: Card = spell(
    "Disintegrate",
    &[Keyword::Action],
    &[
        play(
            &[a_unit_at_a_battlefield("a unit at a battlefield")],
            disintegrate,
        ),
        triggered(Trigger::Reflexive, &[], draw_if_it_died),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::{Amount, DamageSource, Expiry, ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const DISINTEGRATE: u32 = 90;
    const BRUTE: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            ..fixtures::spell(DISINTEGRATE, fixtures::HAND, 0, "Disintegrate", 4, 0)
        });
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, DISINTEGRATE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_an_action_with_a_play_and_its_after_kills_watcher() {
        assert!(std::ptr::eq(script_of("Disintegrate").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(
            CARD.abilities[usize::from(DRAW_AFTER_KILL)].trigger,
            Trigger::Reflexive
        );
    }

    #[test]
    fn three_damage_kills_the_sprite_and_the_kill_draws_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, fixtures::SPRITE);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::SPRITE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(
            ctx.blob.delayed.is_empty(),
            "the watcher was consumed by the spell's own cleanup"
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the draw trigger waits on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == DISINTEGRATE && index == DRAW_AFTER_KILL
        ));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1,
            "the draw waits for the trigger"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the spell left, one card came in"
        );
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} died · {seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn three_damage_on_a_four_might_brute_draws_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, BRUTE);
        assert_eq!(ctx.damage_on(BRUTE), 3);
        assert!(ctx.on_board(BRUTE));
        assert!(
            ctx.blob.delayed.is_empty(),
            "nothing died in the attributed cleanup, so the watcher was dropped"
        );
        assert!(ctx.blob.chain.is_empty(), "no trigger for no kill");
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { .. })));
    }

    #[test]
    fn prevented_damage_registers_no_watcher_and_a_gone_target_is_not_hit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::All,
            Expiry::EndOfTurn(ctx.turn()),
        );
        cast_at(&mut ctx, fixtures::SPRITE);
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(ctx.on_board(fixtures::SPRITE));
        assert!(ctx.blob.delayed.is_empty());
        assert!(ctx.blob.chain.is_empty());
        let mut gone = armed();
        let mut ctx = gone.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DISINTEGRATE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        ctx.recall(fixtures::SPRITE, false);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(ctx.blob.delayed.is_empty());
        assert!(ctx.blob.chain.is_empty());
    }
}
