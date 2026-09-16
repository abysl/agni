use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::vi_hotheaded::doubling_of;
use super::{Card, Cost, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;

pub const READY_ME: Cost = Cost {
    energy: 0,
    power: &[Power::Rainbow, Power::Rainbow],
};
pub const READY_ME_LABEL: &str = "ready me";

pub fn double_might_this_turn(ctx: &mut Ctx, item: &Item, unit: u32) -> i16 {
    let current = ctx.current_might(unit);
    let delta = doubling_of(ctx, unit);
    might_this_turn(ctx, item, unit, delta, None);
    ctx.narrate(format!(
        "{{card {unit}}} doubles to {} might this turn",
        current + i32::from(delta)
    ));
    delta
}

pub fn readies_for_two_rainbow_this_turn(ctx: &mut Ctx, _: &Item, unit: u32) {
    let turn = ctx.turn();
    ctx.narrate(format!(
        "{{card {unit}}} has \"2 any power: Ready me.\" this turn (turn {turn})"
    ));
}

fn dominate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    double_might_this_turn(ctx, item, unit);
    readies_for_two_rainbow_this_turn(ctx, item, unit);
    done()
}

pub static CARD: Card = spell(
    "Dominus",
    &[Keyword::Action],
    &[play(&[a_unit("a unit")], dominate)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{this_turn, UNIT};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, play as play_engine};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const DOMINUS: u32 = 90;
    const MY_RUNES: [u32; 3] = [46, 47, 48];

    fn dominus() -> CardInfo {
        CardInfo {
            domain: vec!["Fury".into(), "Body".into()],
            ..fixtures::spell(DOMINUS, fixtures::HAND, 0, "Dominus", 4, 0)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dominus());
        for rune in MY_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DOMINUS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn cast_on(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, DOMINUS).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
    }

    #[test]
    fn the_script_is_an_action_over_any_unit_with_a_two_rainbow_ready_grant() {
        assert!(std::ptr::eq(script_of("Dominus").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!(READY_ME.energy, 0);
        assert_eq!(READY_ME.power, [Power::Rainbow, Power::Rainbow]);
    }

    #[test]
    fn it_adds_the_units_current_might_once_for_the_turn_and_announces_the_ready_grant() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DOMINUS).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "nothing until it resolves"
        );
        let spell = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &spell, fixtures::VI, 1, None);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.current_might(fixtures::VI),
            8,
            "432.1.a · the current 4 as it resolves is added once, the response's +1 included"
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 5);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} doubles to 8 might this turn".to_string()));
        assert!(ctx.blob.log.contains(&format!(
            "{{card 50}} has \"2 any power: Ready me.\" this turn (turn {})",
            ctx.turn()
        )));
        assert_eq!(ctx.card(DOMINUS).unwrap().zone, Some(fixtures::TRASH));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the doubling ends with the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_shrunk_to_nothing_doubles_by_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_on(&mut ctx, fixtures::THEIR_UNIT);
        let spell = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &spell, fixtures::THEIR_UNIT, -4, None);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 0);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            0,
            "477.3.c · increased by 0 instead of a negative amount"
        );
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), -4);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} doubles to 0 might this turn".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gear_a_legend_and_an_empty_pick_are_refused_and_a_gone_target_is_left_alone() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DOMINUS).unwrap();
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
            .any(|line| line.contains("doubles to") || line.contains("Ready me")));
        assert_eq!(ctx.card(DOMINUS).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · floating turn effects, the Relentless Pursuit row: a granted activated ability for the turn (\"2 any power: Ready me.\") has no home, activate::offers reads the source's own script only and Grant carries no Ability; readies_for_two_rainbow_this_turn only narrates until a turn-scoped granted ability lands"]
    fn the_dominated_unit_offers_a_two_rainbow_ready_this_turn_and_not_the_next() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        cast_on(&mut ctx, fixtures::VI);
        fixtures::pass_until_open(&mut ctx);
        let offers: Vec<(String, bool)> = activate::offers(&ctx, 0)
            .iter()
            .filter(|offer| offer.source == fixtures::VI)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect();
        assert_eq!(
            offers,
            [(
                format!("{{card {}}}: {READY_ME_LABEL} (2 any power)", fixtures::VI),
                true
            )]
        );
        let index = activate::offers(&ctx, 0)
            .iter()
            .find(|offer| offer.source == fixtures::VI)
            .map(|offer| offer.index)
            .unwrap();
        activate::activate(&mut ctx, 0, fixtures::VI, index).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert!(activate::offers(&ctx, 0)
            .iter()
            .all(|offer| offer.source != fixtures::VI));
    }
}
