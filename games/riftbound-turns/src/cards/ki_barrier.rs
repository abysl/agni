use super::prelude::{a_unit, card_target, done, play, spell, this_turn};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::{Amount, DamageSource};

pub const PREVENT: u8 = 7;

pub fn prevent_the_next_damage_to_this_turn(ctx: &mut Ctx, unit: u32, amount: u8) {
    let until = this_turn(ctx);
    ctx.prevent_on(unit, DamageSource::Any, Amount::N(amount), until);
    ctx.narrate(format!(
        "the next {amount} damage that would be dealt to {{card {unit}}} this turn is prevented"
    ));
}

fn barrier(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        prevent_the_next_damage_to_this_turn(ctx, unit, PREVENT);
    }
    done()
}

pub static CARD: Card = spell(
    "Ki Barrier",
    &[Keyword::Reaction],
    &[play(&[a_unit("a unit")], barrier)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BARRIER: u32 = 90;
    const ORDER_RUNE: u32 = 46;

    fn guarded() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Order".into()],
            ..fixtures::spell(BARRIER, fixtures::HAND, 0, "Ki Barrier", 2, 1)
        });
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BARRIER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn cast_on(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, BARRIER).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_script_is_a_reaction_over_any_unit_promising_seven() {
        assert!(std::ptr::eq(script_of("Ki Barrier").unwrap(), &CARD));
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
        assert_eq!(PREVENT, 7);
    }

    #[test]
    fn any_unit_is_offered_and_the_promise_is_announced_for_the_pick() {
        let mut fixture = guarded();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BARRIER).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "friendly and enemy units alike"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(
            !ctx.blob
                .log
                .iter()
                .any(|line| line.ends_with("is prevented")),
            "nothing before it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.log.contains(
            &"the next 7 damage that would be dealt to {card 81} this turn is prevented"
                .to_string()
        ));
        assert_eq!(ctx.card(BARRIER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gear_a_legend_and_an_empty_pick_are_refused_and_a_gone_target_gets_no_promise() {
        let mut fixture = guarded();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BARRIER).unwrap();
        for wrong in [
            fixtures::HAND_GEAR,
            fixtures::LEGEND_CARD,
            fixtures::GROUNDS,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.bounce(fixtures::THEIR_UNIT);
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("is prevented")));
        assert_eq!(ctx.card(BARRIER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_next_seven_damage_to_the_unit_this_turn_is_prevented_across_instances_and_others_take_theirs(
    ) {
        let mut fixture = guarded();
        let mut ctx = fixture.ctx();
        cast_on(&mut ctx, fixtures::VI);
        assert!(
            !ctx.damage(fixtures::VI, 4, Cause::Item(9)),
            "the first four are prevented in full"
        );
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.damage(fixtures::THEIR_UNIT, 1, Cause::Item(9)));
        assert_eq!(
            ctx.damage_on(fixtures::THEIR_UNIT),
            1,
            "other units are unshielded"
        );
        assert!(
            ctx.damage(fixtures::VI, 5, Cause::Combat),
            "three of the next five are prevented and two land"
        );
        assert_eq!(ctx.damage_on(fixtures::VI), 2);
        assert!(ctx.damage(fixtures::VI, 1, Cause::Combat));
        assert_eq!(ctx.damage_on(fixtures::VI), 3, "the shield is spent");
    }
}
