use super::prelude::{done, might_this_turn, on_attack, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 2;

pub fn ready_enemy_here(ctx: &Ctx, me: u32) -> bool {
    let Some(here) = ctx.location(me) else {
        return false;
    };
    let seat = ctx.controller(me);
    ctx.units_at(here).into_iter().any(|unit| {
        ctx.controller(unit) != seat && ctx.card(unit).is_some_and(|held| !held.exhausted)
    })
}

fn sandstorm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    if ready_enemy_here(ctx, me) {
        might_this_turn(ctx, item, me, MIGHT, None);
        ctx.narrate(format!(
            "{{card {me}}} gets +{MIGHT} might this turn · a ready enemy stands here"
        ));
    } else {
        ctx.narrate(format!("{{card {me}}}: no ready enemy unit here"));
    }
    done()
}

pub static CARD: Card = unit("Dune Drake", &[], &[on_attack(&[], sandstorm)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, phases, triggers};
    use crate::state::ItemKind;

    const DRAKE: u32 = 90;
    const SECOND: u32 = 91;

    fn desert() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut drake = fixtures::unit(DRAKE, fixtures::BF1, 0, "Dune Drake", 5);
        drake.domain = vec!["Body".into()];
        drake.energy = Some(5);
        drake.exhausted = true;
        fixture.table.cards.push(drake);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(DRAKE).unwrap(), &CARD));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: DRAKE });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == DRAKE
        ));
        assert!(ctx.blob.prompt.is_none(), "the drake chooses nothing");
    }

    #[test]
    fn the_script_is_a_unit_with_one_untargeted_attack_trigger() {
        assert!(std::ptr::eq(script_of("Dune Drake").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_none(), "the if is read at resolution");
    }

    #[test]
    fn a_ready_defender_here_gives_the_drake_two_might_until_the_turn_ends() {
        let mut fixture = desert();
        let mut ctx = fixture.ctx();
        assert!(ready_enemy_here(&ctx, DRAKE));
        attacks(&mut ctx);
        assert_eq!(ctx.current_might(DRAKE), 5, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(DRAKE), 5 + i32::from(MIGHT));
        assert!(ctx.blob.log.contains(
            &"{card 90} gets +2 might this turn · a ready enemy stands here".to_string()
        ));
        let table = ctx.table.clone();
        let mut blob = ctx.blob.clone();
        drop(ctx);
        blob.prompt = None;
        blob.why = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(DRAKE), 5, "the bonus is for the turn");
    }

    #[test]
    fn an_exhausted_enemy_or_an_enemy_elsewhere_gives_nothing_and_the_check_is_at_resolution() {
        let mut fixture = desert();
        fixture
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!ready_enemy_here(&ctx, DRAKE));
        attacks(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(DRAKE), 5);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90}: no ready enemy unit here".to_string()));
        drop(ctx);

        let mut away = desert();
        away.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BASE);
        away.table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 0, "Friend", 1));
        away.resolve();
        let mut ctx = away.ctx();
        assert!(
            !ready_enemy_here(&ctx, DRAKE),
            "a ready friend is not a ready enemy"
        );
        attacks(&mut ctx);
        ctx.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(DRAKE),
            7,
            "an enemy that arrives before the trigger resolves counts"
        );
    }
}
