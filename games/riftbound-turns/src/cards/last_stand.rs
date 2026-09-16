use super::prelude::{a_friendly_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

fn give_temporary(ctx: &mut Ctx, unit: u32) -> bool {
    ctx.mark_temporary(unit)
}

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let current = ctx.current_might(unit);
    let delta = i16::try_from(current.max(0)).unwrap_or(i16::MAX);
    might_this_turn(ctx, item, unit, delta, None);
    ctx.narrate(format!(
        "{{card {unit}}} doubles to {} might this turn",
        current + i32::from(delta)
    ));
    if give_temporary(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is Temporary"));
    }
    done()
}

pub static CARD: Card = spell(
    "Last Stand",
    &[Keyword::Action],
    &[play(&[a_friendly_unit("a friendly unit")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{chain, play as play_engine, triggers};
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const LAST_STAND: u32 = 90;
    const CALM_RUNE: u32 = 46;

    fn last_stand() -> CardInfo {
        let mut card = fixtures::spell(LAST_STAND, fixtures::HAND, 0, "Last Stand", 3, 1);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(last_stand());
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_an_action_with_one_friendly_unit_target() {
        assert!(std::ptr::eq(script_of("Last Stand").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let spec = CARD.abilities[0].targets[0];
        assert_eq!(spec.filter, crate::cards::prelude::FRIENDLY_UNIT);
        assert_eq!((spec.min, spec.max), (1, 1));
    }

    #[test]
    fn the_unit_doubles_its_current_might_for_the_turn_and_stays_temporary_after() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.grant(fixtures::VI, Keyword::Shield(2), Expiry::Permanent);
        ctx.mark_defender(fixtures::VI);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "3 + Shield 2 while defending"
        );
        fixtures::play_from_hand(&mut ctx, 0, LAST_STAND).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "friendly units only"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(
            [42, CALM_RUNE]
                .iter()
                .filter(|rune| ctx.card(**rune).unwrap().zone == Some(fixtures::RUNE_DECK))
                .count(),
            1,
            "one Calm rune pays the power"
        );
        assert!(!ctx.is_temporary(fixtures::VI), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.current_might(fixtures::VI),
            10,
            "432.1.a · the current 5 is added once"
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 5);
        assert!(ctx.is_temporary(fixtures::VI));
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(fixtures::VI),
            counter: crate::rules::COUNTER_TEMPORARY,
            delta: 1
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} doubles to 10 might this turn".to_string()));
        assert!(ctx.blob.log.contains(&"{card 50} is Temporary".to_string()));
        assert_eq!(ctx.card(LAST_STAND).unwrap().zone, Some(fixtures::TRASH));
        ctx.clear_designation(fixtures::VI);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            8,
            "432.1.a · after combat the Shield falls away but the +5 stays"
        );
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the doubling ends with the turn"
        );
        assert!(ctx.is_temporary(fixtures::VI), "Temporary does not");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_temporary_unit_dies_at_its_controllers_next_beginning_phase() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LAST_STAND).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_temporary(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        ctx.raise(Event::BeginningPhase { seat: 0 });
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "the implicit Temporary trigger reads the mark"
        );
        chain::proceed(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            !ctx.on_board(fixtures::VI),
            "the Temporary unit is killed at the start of its controller's Beginning Phase"
        );
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { card, .. } if *card == fixtures::VI)));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 50} is Temporary and dies"));
    }

    #[test]
    fn a_unit_shrunk_to_nothing_doubles_by_zero_and_one_that_left_is_untouched() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LAST_STAND).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let eclipse = crate::state::ChainItem::new(
            9,
            crate::state::ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            crate::state::Origin::Hand,
        );
        might_this_turn(&mut ctx, &eclipse, fixtures::VI, -5, None);
        assert_eq!(ctx.current_might(fixtures::VI), 0);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            0,
            "477.3.c · a negative current Might is increased by 0"
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), -5);
        assert!(ctx.is_temporary(fixtures::VI), "Temporary is still given");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} doubles to 0 might this turn".to_string()));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LAST_STAND).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_temporary(fixtures::VI));
        assert!(!ctx.blob.log.iter().any(|line| line.contains("doubles")));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn an_enemy_unit_is_refused_and_a_board_without_friendly_units_offers_only_cancel() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LAST_STAND).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is not friendly"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::CHAMPION_CARD]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a champion off the board is not a unit on it"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(LAST_STAND).unwrap().zone, Some(fixtures::HAND));
        let mut empty = armed();
        empty.table.cards.retain(|card| card.id != fixtures::VI);
        empty.resolve();
        let mut ctx = empty.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LAST_STAND).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["cancel"]);
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
    }
}
