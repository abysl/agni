use super::prelude::{
    a_card, a_unit, at_end_of_turn, card_target, chosen_mode, disempower, done, is_empowered,
    modal, mode, spell, triggered, UNIT,
};
use super::{Card, Filter, Flow, Item, Keyword, ModeSpec, Stage, Trigger};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const DISEMPOWER_AT_END_OF_TURN: u8 = 1;
pub const EMPOWER_AT_END_OF_TURN: u8 = 2;
pub const EMPOWERED_UNIT: Filter = Filter::And(&[UNIT, Filter::Empowered]);
pub const MODES: &[ModeSpec] = &[
    mode("Empower a unit", &[a_unit("a unit to Empower")], empower_it),
    mode(
        "Disempower a unit that's Empowered",
        &[a_card(EMPOWERED_UNIT, "an Empowered unit to Disempower")],
        disempower_it,
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Empower,
    Disempower,
}

pub fn mode_of(item: &Item) -> Option<Mode> {
    match chosen_mode(item)? {
        0 => Some(Mode::Empower),
        1 => Some(Mode::Disempower),
        _ => None,
    }
}

fn empower_it(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if is_empowered(ctx, unit) {
        ctx.narrate(format!(
            "{{card {unit}}} is already Empowered · disempowered at end of turn"
        ));
    } else {
        ctx.empower_by(unit, item.controller);
        ctx.narrate(format!("{{card {unit}}} is empowered until end of turn"));
    }
    at_end_of_turn(ctx, item, DISEMPOWER_AT_END_OF_TURN, vec![unit]);
    done()
}

fn disempower_it(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    disempower(ctx, unit);
    ctx.narrate(format!("{{card {unit}}} is disempowered until end of turn"));
    at_end_of_turn(ctx, item, EMPOWER_AT_END_OF_TURN, vec![unit]);
    done()
}

fn delayed_unit(item: &Item) -> Option<u32> {
    match item.targets.first() {
        Some(TargetRef::Card(unit)) => Some(*unit),
        _ => None,
    }
}

fn disempower_again(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = delayed_unit(item) {
        if disempower(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is disempowered"));
        }
    }
    done()
}

fn empower_again(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = delayed_unit(item) {
        if ctx.empower_by(unit, item.controller) {
            ctx.narrate(format!("{{card {unit}}} is empowered"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Sanction",
    &[Keyword::Reaction],
    &[
        modal(MODES),
        triggered(Trigger::Reflexive, &[], disempower_again),
        triggered(Trigger::Reflexive, &[], empower_again),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{phases, play as play_engine, priority};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef, When};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const SANCTION: u32 = 90;
    const CALM_RUNES: [u32; 2] = [46, 47];

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Calm".into()],
            ..fixtures::spell(SANCTION, fixtures::HAND, 0, "Sanction", 3, 1)
        });
        for rune in CALM_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SANCTION).unwrap(),
            &CARD
        ));
        fixture
    }

    fn empowered(mut fixture: Fixture, unit: u32) -> Fixture {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(unit),
            counter: COUNTER_EMPOWERED,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.resolve();
        fixture
    }

    fn cast_on(ctx: &mut Ctx, mode: &str, unit: u32) {
        fixtures::play_from_hand(ctx, 0, SANCTION).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Mode {
                item: 1,
                execution: 0
            })
        );
        fixtures::choose(ctx, 0, mode).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    fn end_the_turn(ctx: &mut Ctx) {
        phases::end_turn(ctx).unwrap();
        while !ctx.blob.chain.is_empty() {
            let holder = priority::holder(ctx).unwrap();
            priority::pass(ctx, holder).unwrap();
        }
    }

    #[test]
    fn the_script_is_a_reaction_over_two_named_modes_with_two_end_of_turn_reverts() {
        assert!(std::ptr::eq(script_of("Sanction").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 3);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        let modes = CARD.abilities[0].modes;
        assert_eq!(modes.len(), 2);
        assert_eq!(modes[0].label, "Empower a unit");
        assert_eq!(modes[0].targets.len(), 1);
        assert_eq!(modes[0].targets[0].filter, UNIT);
        assert_eq!(modes[1].label, "Disempower a unit that's Empowered");
        assert_eq!(modes[1].targets.len(), 1);
        assert_eq!(modes[1].targets[0].filter, EMPOWERED_UNIT);
        assert_eq!(
            CARD.abilities[usize::from(DISEMPOWER_AT_END_OF_TURN)].trigger,
            Trigger::Reflexive
        );
        assert_eq!(
            CARD.abilities[usize::from(EMPOWER_AT_END_OF_TURN)].trigger,
            Trigger::Reflexive
        );
        let mut item = ChainItem::new(1, ItemKind::Spell { card: SANCTION }, 0, Origin::Hand);
        assert_eq!(mode_of(&item), None);
        item.set_mode(0, 0);
        assert_eq!(mode_of(&item), Some(Mode::Empower));
        item.set_mode(0, 1);
        assert_eq!(mode_of(&item), Some(Mode::Disempower));
        item.set_mode(0, 2);
        assert_eq!(mode_of(&item), None);
    }

    #[test]
    fn a_plain_unit_is_empowered_and_disempowered_again_at_end_of_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SANCTION).unwrap();
        fixtures::choose(&mut ctx, 0, "Empower a unit").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(
            !ctx.is_empowered(fixtures::THEIR_UNIT),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_empowered(fixtures::THEIR_UNIT));
        assert!(ctx.events.contains(&Event::Empowered {
            card: fixtures::THEIR_UNIT,
            by: 0
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} is empowered until end of turn".to_string()));
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].when, When::EndOfTurn(ctx.turn()));
        assert_eq!(ctx.blob.delayed[0].source, SANCTION);
        assert_eq!(ctx.blob.delayed[0].ability, DISEMPOWER_AT_END_OF_TURN);
        assert_eq!(ctx.blob.delayed[0].args, [fixtures::THEIR_UNIT]);
        assert_eq!(ctx.card(SANCTION).unwrap().zone, Some(fixtures::TRASH));
        end_the_turn(&mut ctx);
        assert!(!ctx.is_empowered(fixtures::THEIR_UNIT));
        assert!(ctx.events.contains(&Event::Disempowered {
            card: fixtures::THEIR_UNIT
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} is disempowered".to_string()));
        assert!(ctx.blob.delayed.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_empowered_unit_is_disempowered_and_empowered_again_at_end_of_turn() {
        let mut fixture = empowered(armed(), fixtures::VI);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SANCTION).unwrap();
        fixtures::choose(&mut ctx, 0, "Disempower a unit that's Empowered").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "only the Empowered unit"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_empowered(fixtures::VI));
        assert!(ctx
            .events
            .contains(&Event::Disempowered { card: fixtures::VI }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} is disempowered until end of turn".to_string()));
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].ability, EMPOWER_AT_END_OF_TURN);
        end_the_turn(&mut ctx);
        assert!(ctx.is_empowered(fixtures::VI));
        assert!(ctx.blob.log.contains(&"{card 50} is empowered".to_string()));
        assert!(ctx.blob.delayed.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_mode_is_fixed_as_it_is_played_and_a_unit_gone_by_end_of_turn_is_left_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_on(&mut ctx, "Empower a unit", fixtures::VI);
        drop(ctx);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SANCTION).unwrap();
        fixtures::choose(&mut ctx, 0, "Empower a unit").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(ctx.empower(fixtures::VI), "Empowered in response");
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.is_empowered(fixtures::VI),
            "355.3 · the mode was chosen as it was played: Empower again does nothing"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} is already Empowered · disempowered at end of turn".to_string()));
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].ability, DISEMPOWER_AT_END_OF_TURN);
        ctx.bounce(fixtures::VI);
        end_the_turn(&mut ctx);
        assert!(ctx.blob.delayed.is_empty());
        assert!(!ctx.is_empowered(fixtures::VI));
        assert!(
            !ctx.blob
                .log
                .contains(&"{card 50} is disempowered".to_string()),
            "a unit in hand is not disempowered"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gear_a_legend_and_an_empty_pick_are_refused_and_a_gone_target_is_left_alone() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SANCTION).unwrap();
        fixtures::choose(&mut ctx, 0, "Empower a unit").unwrap();
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
        assert!(ctx.blob.delayed.is_empty(), "no revert is scheduled");
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Empowered { .. })));
        assert_eq!(ctx.card(SANCTION).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_mode_is_asked_by_name_before_its_target() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SANCTION).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "Empower a unit",
                "Disempower a unit that's Empowered",
                "cancel"
            ]
        );
    }
}
