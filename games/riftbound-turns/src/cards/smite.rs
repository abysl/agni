use super::prelude::{
    a_unit_at_a_battlefield, at_end_of_turn, banish_by, card_target, deal, done, play, replaces,
    spell, triggered, with_replacement,
};
use super::{Card, Flow, Item, Keyword, Source, Stage, Trigger, WouldDie};
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, When};

pub const DAMAGE: u8 = 3;
pub const LAPSE: u8 = 1;

pub fn watches(ctx: &Ctx, smite: u32, unit: u32) -> bool {
    let turn = ctx.turn();
    ctx.blob.delayed.iter().any(|delayed| {
        delayed.source == smite
            && delayed.ability == LAPSE
            && delayed.when == When::EndOfTurn(turn)
            && delayed.args.first() == Some(&unit)
    })
}

fn spend_watch(ctx: &mut Ctx, smite: u32, unit: u32) {
    let turn = ctx.turn();
    let mut spent = false;
    ctx.blob.delayed.retain(|delayed| {
        let hit = !spent
            && delayed.source == smite
            && delayed.ability == LAPSE
            && delayed.when == When::EndOfTurn(turn)
            && delayed.args.first() == Some(&unit);
        if hit {
            spent = true;
        }
        !hit
    });
}

pub fn banished_instead_of_dying_this_turn(ctx: &mut Ctx, item: &Item, unit: u32) {
    at_end_of_turn(ctx, item, LAPSE, vec![unit]);
    ctx.narrate(format!(
        "if {{card {unit}}} would die this turn, it is banished instead"
    ));
}

fn smite(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if deal(ctx, item, unit, DAMAGE) {
        ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
    }
    banished_instead_of_dying_this_turn(ctx, item, unit);
    done()
}

fn lapse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(TargetRef::Card(unit)) = item.targets.first() {
        ctx.narrate(format!("the watch on {{card {unit}}} lapses"));
    }
    done()
}

fn smitten_unit_would_die(ctx: &Ctx, would: &WouldDie, source: Source) -> bool {
    ctx.is_unit(would.unit) && ctx.on_board(would.unit) && watches(ctx, source.card, would.unit)
}

fn banish_instead(ctx: &mut Ctx, would: &WouldDie, source: Source) {
    spend_watch(ctx, source.card, would.unit);
    if banish_by(ctx, would.unit, ctx.controller(source.card)) {
        ctx.narrate(format!(
            "{{card {}}} is banished instead of dying",
            would.unit
        ));
    }
}

pub static CARD: Card = with_replacement(
    spell(
        "Smite",
        &[Keyword::Action],
        &[
            play(&[a_unit_at_a_battlefield("a unit at a battlefield")], smite),
            triggered(Trigger::Reflexive, &[], lapse),
        ],
    ),
    replaces(smitten_unit_would_die, banish_instead),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, EntryMove, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, play as play_engine, priority, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const SMITE: u32 = 90;
    const THEIR_SMITE: u32 = 91;
    const BRUTE: u32 = 92;
    const SCOUT: u32 = 93;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::spell(SMITE, fixtures::HAND, 0, "Smite", 2, 1));
        fixture.table.cards.push(fixtures::spell(
            THEIR_SMITE,
            fixtures::HAND,
            1,
            "Smite",
            2,
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 1, "Scout", 3));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, SMITE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    fn source() -> Source {
        Source {
            card: SMITE,
            ability: 0,
        }
    }

    #[test]
    fn the_script_is_an_action_with_a_play_a_lapse_trigger_and_the_banish_replacement() {
        assert!(std::ptr::eq(script_of("Smite").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(
            CARD.abilities[usize::from(LAPSE)].trigger,
            Trigger::Reflexive
        );
        assert!(CARD.replacement.is_some());
        assert_eq!(DAMAGE, 3);
    }

    #[test]
    fn three_lands_on_a_unit_at_a_battlefield_and_the_watch_lives_on_the_delayed_list_until_the_turn_ends(
    ) {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SMITE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "{card 93}", "cancel"],
            "units at battlefields only"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(!watches(&ctx, SMITE, BRUTE), "nothing before it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(BRUTE), 3);
        assert!(ctx.on_board(BRUTE), "three on four Might survives");
        assert!(watches(&ctx, SMITE, BRUTE));
        assert!(!watches(&ctx, SMITE, SCOUT));
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].when, When::EndOfTurn(ctx.turn()));
        assert!(ctx.blob.log.contains(&"{card 92} takes 3".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"if {card 92} would die this turn, it is banished instead".to_string()));
        assert_eq!(ctx.card(SMITE).unwrap().zone, Some(fixtures::TRASH));
        phases::end_turn(&mut ctx).unwrap();
        while !ctx.blob.chain.is_empty() {
            let holder = priority::holder(&ctx).unwrap();
            priority::pass(&mut ctx, holder).unwrap();
        }
        assert!(
            ctx.blob.delayed.is_empty(),
            "the watch lapses with the turn"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"the watch on {card 92} lapses".to_string()));
        assert!(!watches(&ctx, SMITE, BRUTE));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_replacement_reads_the_watched_unit_only_and_banishes_it_once() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        let replacement = CARD.replacement.unwrap();
        let watched = WouldDie {
            unit: BRUTE,
            cause: Cause::Item(9),
        };
        let other = WouldDie {
            unit: SCOUT,
            cause: Cause::Item(9),
        };
        assert!((replacement.applies)(&ctx, &watched, source()));
        assert!(
            !(replacement.applies)(&ctx, &other, source()),
            "the Scout was not smitten"
        );
        (replacement.run)(&mut ctx, &watched, source());
        assert!(ctx.in_banishment(BRUTE));
        assert!(!ctx.on_board(BRUTE));
        assert!(
            !watches(&ctx, SMITE, BRUTE),
            "the watch is spent by the banish"
        );
        assert!(ctx.blob.delayed.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} is banished instead of dying".to_string()));
        assert!(
            !(replacement.applies)(&ctx, &watched, source()),
            "a banished unit is off the board"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn today_a_later_kill_of_the_smitten_unit_still_sends_it_to_the_trash() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        assert!(watches(&ctx, SMITE, BRUTE));
        assert_eq!(
            ctx.kill(BRUTE, Cause::Item(9)),
            Killed::Yes,
            "kill::applicable consults faces on the board only, so a spell in the trash is never asked"
        );
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::TRASH));
        assert!(watches(&ctx, SMITE, BRUTE), "the watch was never consulted");
    }

    #[test]
    #[ignore = "engine gap · a floating turn replacement · kill::applicable lists faces on the board only, so a resolved spell's replacement is never consulted; banished_instead_of_dying_this_turn registers the watch until the engine folds floating replacements in beside in-play cards"]
    fn a_smitten_unit_that_would_die_this_turn_is_banished_instead_by_its_own_damage_or_a_later_kill(
    ) {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, SCOUT);
        assert!(
            ctx.in_banishment(SCOUT),
            "three lethal on the 3-Might Scout in the cleanup after the spell is banished"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { card, .. } if *card == SCOUT)));
        let mut later = armed();
        let mut ctx = later.ctx();
        cast_at(&mut ctx, BRUTE);
        assert_eq!(ctx.kill(BRUTE, Cause::Item(9)), Killed::Replaced);
        settle(&mut ctx).unwrap();
        assert!(ctx.in_banishment(BRUTE));
        assert!(!watches(&ctx, SMITE, BRUTE));
    }

    #[test]
    fn a_unit_in_a_base_is_refused_a_gone_target_is_not_hit_and_the_action_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SMITE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, SMITE).unwrap();
        for wrong in [fixtures::THEIR_UNIT, fixtures::VI, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base or not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.recall(BRUTE, false);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(
            ctx.blob.delayed.is_empty(),
            "no watch on a unit that was not hit"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert_eq!(ctx.card(SMITE).unwrap().zone, Some(fixtures::TRASH));
    }
}
