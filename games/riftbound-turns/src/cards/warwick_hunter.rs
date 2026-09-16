use super::prelude::{done, on_attack, unit, with_statics};
use super::{Card, Flow, Item, Stage, Static};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::kill;

pub fn enters_ready(_: &Ctx, _: u32) -> bool {
    true
}

pub fn damaged_enemies_here(ctx: &Ctx, me: u32, seat: u8) -> Vec<u32> {
    let Some(here) = ctx.location(me) else {
        return Vec::new();
    };
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat && ctx.damage_on(*unit) > 0)
        .collect()
}

fn infinite_duress(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    let prey = damaged_enemies_here(ctx, me, item.controller);
    if prey.is_empty() {
        ctx.narrate(format!("{{card {me}}}: no damaged enemy unit here"));
        return done();
    }
    let dead = kill::batch(ctx, &prey, Cause::Item(item.id));
    ctx.narrate(format!(
        "{{card {me}}} kills {} damaged enemy units here",
        dead.len()
    ));
    done()
}

pub static CARD: Card = with_statics(
    unit("Warwick - Hunter", &[], &[on_attack(&[], infinite_duress)]),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, triggers};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::{CounterInfo, Target};

    const WARWICK: u32 = 90;
    const WOUNDED: u32 = 91;
    const WHOLE: u32 = 92;
    const WOUNDED_AWAY: u32 = 93;
    const WOUNDED_FRIEND: u32 = 94;

    fn wound(fixture: &mut Fixture, card: u32, value: i32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(card),
            counter: COUNTER_DAMAGE,
            value,
        });
        fixture.table.counters.sort();
    }

    fn hunt() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut warwick = fixtures::unit(WARWICK, fixtures::BF1, 0, "Warwick - Hunter", 5);
        warwick.domain = vec!["Body".into()];
        warwick.energy = Some(6);
        warwick.power = Some(1);
        fixture.table.cards.push(warwick);
        fixture
            .table
            .cards
            .push(fixtures::unit(WOUNDED, fixtures::BF1, 1, "Wounded", 6));
        fixture
            .table
            .cards
            .push(fixtures::unit(WHOLE, fixtures::BF1, 1, "Whole", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(WOUNDED_AWAY, fixtures::BASE, 1, "Away", 6));
        fixture.table.cards.push(fixtures::unit(
            WOUNDED_FRIEND,
            fixtures::BF1,
            0,
            "Friend",
            6,
        ));
        wound(&mut fixture, WOUNDED, 1);
        wound(&mut fixture, WOUNDED_AWAY, 1);
        wound(&mut fixture, WOUNDED_FRIEND, 1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WARWICK).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: WARWICK });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "all damaged enemies: nothing to choose"
        );
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == WARWICK
        ));
    }

    #[test]
    fn the_script_is_a_unit_with_one_untargeted_attack_trigger_and_names_the_enters_ready_seam() {
        assert!(std::ptr::eq(script_of("Warwick - Hunter").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.targets.is_empty());
        let mut fixture = hunt();
        let ctx = fixture.ctx();
        assert!(enters_ready(&ctx, WARWICK));
    }

    #[test]
    fn attacking_kills_every_damaged_enemy_here_and_spares_the_whole_the_distant_and_the_friendly()
    {
        let mut fixture = hunt();
        let mut ctx = fixture.ctx();
        assert_eq!(damaged_enemies_here(&ctx, WARWICK, 0), [WOUNDED]);
        attacks(&mut ctx);
        assert!(ctx.on_board(WOUNDED), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.on_board(WOUNDED),
            "one damage on 6 Might is enough for the hunter"
        );
        assert!(ctx.on_board(WHOLE));
        assert!(ctx.on_board(WOUNDED_AWAY));
        assert!(ctx.on_board(WOUNDED_FRIEND));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == WOUNDED
        )));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} kills 1 damaged enemy units here".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn damage_dealt_before_the_trigger_resolves_counts_and_nothing_damaged_resolves_quietly() {
        let mut fixture = hunt();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        let strike = ctx.blob.chain[0].clone();
        ctx.damage(WHOLE, 1, Cause::Item(strike.id));
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(WOUNDED));
        assert!(
            !ctx.on_board(WHOLE),
            "the hunt reads the damage as it resolves"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} kills 2 damaged enemy units here".to_string()));
        drop(ctx);

        let mut whole = hunt();
        whole.table.counters.clear();
        whole.resolve();
        let mut ctx = whole.ctx();
        attacks(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(WOUNDED) && ctx.on_board(WHOLE));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90}: no damaged enemy unit here".to_string()));
        ctx.raise(Event::Defends { card: WARWICK });
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "the hunt is on the attack only"
        );
    }

    #[test]
    fn played_from_hand_warwick_enters_ready() {
        let mut fixture = Fixture::enforced();
        let mut warwick = fixtures::unit(WARWICK, fixtures::HAND, 0, "Warwick - Hunter", 5);
        warwick.domain = vec!["Fury".into()];
        warwick.energy = Some(1);
        warwick.power = Some(1);
        fixture.table.cards.push(warwick);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WARWICK).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(WARWICK));
        assert!(
            !ctx.card(WARWICK).unwrap().exhausted,
            "I enter ready · the replacement on the way a played unit lands"
        );
    }
}
