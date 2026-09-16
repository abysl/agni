use super::prelude::{a_unit_at_a_battlefield, card_target, deal, done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;
pub const DAMAGE_TO_AN_ATTACKER: u8 = 4;

pub fn damage_for(ctx: &Ctx, unit: u32) -> u8 {
    if ctx.is_attacker(unit) {
        DAMAGE_TO_AN_ATTACKER
    } else {
        DAMAGE
    }
}

fn storm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let amount = damage_for(ctx, unit);
    if deal(ctx, item, unit, amount) {
        ctx.narrate(format!("{{card {unit}}} takes {amount}"));
    }
    done()
}

pub static CARD: Card = spell(
    "Sudden Storm",
    &[Keyword::Hidden, Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        storm,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, Showdown, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const STORM: u32 = 90;

    fn brewing() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Fury".into()],
            ..fixtures::spell(STORM, fixtures::HAND, 0, "Sudden Storm", 3, 0)
        });
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(5);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(STORM).unwrap(), &CARD));
        fixture
    }

    fn damage(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_hidden_action_over_a_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Sudden Storm").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(ability.targets[0].kind, TargetKind::Card);
        assert_eq!((DAMAGE, DAMAGE_TO_AN_ATTACKER), (2, 4));
    }

    #[test]
    fn a_unit_at_a_battlefield_outside_combat_takes_two() {
        let mut fixture = brewing();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STORM).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "units at battlefields, friendly or not"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            damage(&ctx, fixtures::THEIR_UNIT),
            0,
            "nothing before it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(damage(&ctx, fixtures::THEIR_UNIT), 2);
        assert!(ctx.on_board(fixtures::THEIR_UNIT), "5 Might survives 2");
        assert!(ctx.blob.log.contains(&"{card 81} takes 2".to_string()));
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::THEIR_UNIT,
            n: 2,
            source: crate::engine::ctx::Cause::Item(1)
        }));
        assert_eq!(ctx.card(STORM).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_attacking_unit_takes_four_instead_read_as_the_spell_resolves() {
        let mut fixture = brewing();
        fixture.blob.showdown = Some(Showdown {
            combat: true,
            ..Showdown::open(fixtures::BF1, 0, 1)
        });
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(fixtures::VI));
        assert!(ctx.mark_defender(fixtures::THEIR_UNIT));
        assert_eq!(damage_for(&ctx, fixtures::VI), 4);
        assert_eq!(
            damage_for(&ctx, fixtures::THEIR_UNIT),
            2,
            "a defender takes the base amount"
        );
        fixtures::play_from_hand(&mut ctx, 0, STORM).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(damage(&ctx, fixtures::VI), 0);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.log.contains(&"{card 50} takes 4".to_string()));
        assert!(
            !ctx.on_board(fixtures::VI),
            "four is lethal for Vi's three: {:?}",
            ctx.blob.log
        );
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = brewing();
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(fixtures::VI));
        fixtures::play_from_hand(&mut ctx, 0, STORM).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.clear_designation(fixtures::VI);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            damage(&ctx, fixtures::VI),
            2,
            "no longer attacking when the storm lands"
        );
        assert!(ctx.on_board(fixtures::VI));
    }

    #[test]
    fn a_unit_in_a_base_is_refused_and_a_target_that_left_takes_nothing() {
        let mut fixture = brewing();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STORM).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.bounce(fixtures::THEIR_UNIT);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert_eq!(ctx.card(STORM).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }
}
