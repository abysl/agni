use super::prelude::{a_unit, card_target, done, draw, play, spell, this_turn};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::{Amount, DamageSource};

pub const DRAWS: usize = 1;

pub fn prevent_the_next_damage_to(ctx: &mut Ctx, unit: u32) {
    let until = this_turn(ctx);
    ctx.prevent_on(unit, DamageSource::Any, Amount::Next, until);
    ctx.narrate(format!(
        "the next time {{card {unit}}} would be dealt damage this turn, it is prevented"
    ));
}

fn parry(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if let Some(unit) = card_target(ctx, item, 0) {
        prevent_the_next_damage_to(ctx, unit);
    }
    draw(ctx, seat, DRAWS);
    done()
}

pub static CARD: Card = spell(
    "Counter Strike",
    &[Keyword::Reaction],
    &[play(&[a_unit("a unit")], parry)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const COUNTER_STRIKE: u32 = 90;

    fn guarded() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Calm".into()],
            ..fixtures::spell(COUNTER_STRIKE, fixtures::HAND, 0, "Counter Strike", 2, 1)
        });
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(COUNTER_STRIKE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn cast_on(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, COUNTER_STRIKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_script_is_a_reaction_over_any_unit() {
        assert!(std::ptr::eq(script_of("Counter Strike").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!(
            (
                ability.targets[0].min,
                ability.targets[0].max,
                ability.targets[0].kind
            ),
            (1, 1, TargetKind::Card)
        );
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn any_unit_is_offered_the_promise_is_announced_and_one_card_is_drawn() {
        let mut fixture = guarded();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, COUNTER_STRIKE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "friendly and enemy units alike"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(drew(&ctx, 0), 0, "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.log.contains(
            &"the next time {card 81} would be dealt damage this turn, it is prevented".to_string()
        ));
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert_eq!(
            ctx.card(COUNTER_STRIKE).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_target_that_left_the_board_makes_no_promise_but_the_draw_still_happens() {
        let mut fixture = guarded();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, COUNTER_STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.bounce(fixtures::THEIR_UNIT);
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("it is prevented")));
        assert_eq!(drew(&ctx, 0), DRAWS, "356.3.e.5 · the draw is not a target");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gear_or_a_legend_is_refused_as_the_target() {
        let mut fixture = guarded();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, COUNTER_STRIKE).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_GEAR]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::LEGEND_CARD]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(COUNTER_STRIKE).unwrap().zone, Some(fixtures::HAND));
    }

    #[test]
    fn the_next_damage_to_the_unit_this_turn_is_prevented_once_and_other_units_still_take_theirs() {
        let mut fixture = guarded();
        let mut ctx = fixture.ctx();
        cast_on(&mut ctx, fixtures::VI);
        assert!(
            !ctx.damage(fixtures::VI, 2, Cause::Item(9)),
            "the first instance is prevented in full"
        );
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.damage(fixtures::THEIR_UNIT, 1, Cause::Item(9)));
        assert_eq!(
            ctx.damage_on(fixtures::THEIR_UNIT),
            1,
            "other units are unshielded"
        );
        assert!(ctx.damage(fixtures::VI, 1, Cause::Combat));
        assert_eq!(ctx.damage_on(fixtures::VI), 1, "the second instance lands");
    }
}
