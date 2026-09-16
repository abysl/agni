use super::prelude::{
    a_friendly_unit, buff, card_target, done, friendly_units, might_this_turn, play, spell,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const EXTRA_MIGHT: i16 = 1;

pub fn snapshot_of_the_turn_aura(ctx: &mut Ctx, item: &Item) -> Vec<u32> {
    let buffed: Vec<u32> = friendly_units(ctx, item.controller)
        .into_iter()
        .filter(|unit| ctx.is_buffed(*unit))
        .collect();
    for unit in &buffed {
        might_this_turn(ctx, item, *unit, EXTRA_MIGHT, None);
        ctx.narrate(format!(
            "{{card {unit}}}'s buff gives +{EXTRA_MIGHT} more might this turn"
        ));
    }
    buffed
}

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        }
    }
    snapshot_of_the_turn_aura(ctx, item);
    done()
}

pub static CARD: Card = spell(
    "Stand United",
    &[Keyword::Hidden, Keyword::Action],
    &[play(&[a_friendly_unit("a friendly unit to buff")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{COUNTER_BUFFED, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const STAND_UNITED: u32 = 90;
    const VETERAN: u32 = 91;
    const ROOKIE: u32 = 92;
    const THEIR_VETERAN: u32 = 93;

    fn stand_united() -> CardInfo {
        let mut card = fixtures::spell(STAND_UNITED, fixtures::HAND, 0, "Stand United", 3, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn buffed(fixture: &mut Fixture, unit: u32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(unit),
            counter: COUNTER_BUFFED,
            value: 1,
        });
        fixture.table.counters.sort();
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(stand_united());
        fixture.table.cards.push(fixtures::unit(
            VETERAN,
            fixtures::BF1,
            0,
            "Legion Rearguard",
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(ROOKIE, fixtures::BASE, 0, "Pit Rookie", 2));
        fixture.table.cards.push(fixtures::unit(
            THEIR_VETERAN,
            fixtures::BF2,
            1,
            "Vanguard Sergeant",
            2,
        ));
        buffed(&mut fixture, VETERAN);
        buffed(&mut fixture, THEIR_VETERAN);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_hidden_action_with_one_friendly_unit_target() {
        assert!(std::ptr::eq(script_of("Stand United").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let spec = CARD.abilities[0].targets[0];
        assert_eq!(spec.filter, crate::cards::prelude::FRIENDLY_UNIT);
        assert_eq!((spec.min, spec.max), (1, 1));
    }

    #[test]
    fn the_chosen_unit_is_buffed_and_every_buffed_friendly_unit_gets_one_more_might_for_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(VETERAN), 2, "1 + its buff");
        fixtures::play_from_hand(&mut ctx, 0, STAND_UNITED).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 91}", "{card 92}", "cancel"],
            "friendly units only, buffed or not"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(ROOKIE)]);
        assert!(!ctx.is_buffed(ROOKIE), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(ROOKIE));
        assert_eq!(ctx.current_might(ROOKIE), 4, "2 + the buff + the extra");
        assert_eq!(might_counter(&ctx, ROOKIE), 1);
        assert_eq!(
            ctx.current_might(VETERAN),
            3,
            "an already buffed friendly unit gets the extra too"
        );
        assert_eq!(might_counter(&ctx, VETERAN), 1);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "an unbuffed friendly unit gets nothing"
        );
        assert_eq!(
            ctx.current_might(THEIR_VETERAN),
            3,
            "a buffed enemy unit keeps only its buff"
        );
        assert_eq!(might_counter(&ctx, THEIR_VETERAN), 0);
        assert_eq!(
            ctx.state_of(ROOKIE).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx.blob.log.contains(&"{card 92} is buffed".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 91}'s buff gives +1 more might this turn".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92}'s buff gives +1 more might this turn".to_string()));
        assert_eq!(ctx.card(STAND_UNITED).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(ROOKIE), 3, "the buff outlives the turn");
        assert!(ctx.is_buffed(ROOKIE));
        assert_eq!(ctx.current_might(VETERAN), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_already_buffed_target_is_not_buffed_twice_but_still_counts_for_the_extra() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STAND_UNITED).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.table.counter(Target::Card(VETERAN), COUNTER_BUFFED),
            Some(1),
            "a buff is one per unit"
        );
        assert!(!ctx.blob.log.contains(&"{card 91} is buffed".to_string()));
        assert_eq!(ctx.current_might(VETERAN), 3, "1 + buff + extra");
        assert_eq!(ctx.current_might(ROOKIE), 2, "never buffed, nothing extra");
    }

    #[test]
    fn a_target_that_left_the_board_is_not_buffed_and_the_extra_still_lands_on_the_rest() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STAND_UNITED).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(ROOKIE, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.is_buffed(ROOKIE));
        assert_eq!(
            ctx.current_might(VETERAN),
            3,
            "356.3.e.5 · the second sentence is not about the target"
        );
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn an_enemy_unit_is_refused_and_an_empty_board_offers_only_cancel() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STAND_UNITED).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[THEIR_VETERAN]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is not friendly"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in hand is not on the board"
        );
        assert!(ctx.blob.prompt.is_some());
        assert!(ctx.effects.is_empty(), "nothing was paid");
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(STAND_UNITED).unwrap().zone, Some(fixtures::HAND));
        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| !card.is_kind("Unit") || card.owner == 1);
        empty.resolve();
        let mut ctx = empty.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STAND_UNITED).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["cancel"]);
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · floating turn effects: the second sentence is an aura for the turn, not a snapshot at resolution; a unit buffed later this turn must also get the extra Might until the turn ends"]
    fn a_unit_buffed_later_this_turn_also_gets_the_extra_might_while_the_aura_floats() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STAND_UNITED).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(buff(&mut ctx, fixtures::VI));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "3 + the new buff + the floating extra"
        );
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "only the buff outlives the turn"
        );
    }
}
