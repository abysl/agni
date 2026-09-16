use super::prelude::{a_unit, card_target, done, grant_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 2;

fn breach(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        let assault = grant_this_turn(ctx, unit, Keyword::Assault(ASSAULT));
        let ganking = grant_this_turn(ctx, unit, Keyword::Ganking);
        if assault && ganking {
            ctx.narrate(format!(
                "{{card {unit}}} gets [Assault {ASSAULT}] and [Ganking] this turn"
            ));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Vault Breaker",
    &[Keyword::Action],
    &[play(&[a_unit("a unit")], breach)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const BREAKER: u32 = 90;
    const THEIR_BREAKER: u32 = 91;

    fn breaker(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Vault Breaker", 1, 1);
        card.domain = vec!["Fury".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(breaker(BREAKER, 0));
        fixture.table.cards.push(breaker(THEIR_BREAKER, 1));
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

    #[test]
    fn the_script_is_an_action_over_one_unit_that_prints_neither_keyword_it_grants() {
        assert!(std::ptr::eq(script_of("Vault Breaker").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert!(!CARD.has_keyword(Keyword::Assault(ASSAULT)));
        assert!(!CARD.has_keyword(Keyword::Ganking));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ASSAULT, 2);
    }

    #[test]
    fn any_unit_gets_assault_two_and_ganking_until_the_end_of_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREAKER).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Ganking));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let turn = ctx.turn();
        assert_eq!(
            ctx.state_of(fixtures::THEIR_UNIT).unwrap().granted,
            [
                (Keyword::Assault(ASSAULT), Expiry::EndOfTurn(turn)),
                (Keyword::Ganking, Expiry::EndOfTurn(turn))
            ]
        );
        assert!(ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Ganking));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        ctx.mark_attacker(fixtures::THEIR_UNIT);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4, "2 + Assault 2");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} gets [Assault 2] and [Ganking] this turn".to_string()));
        ctx.expire(Expiry::EndOfTurn(turn));
        assert!(
            !ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)),
            "the grant ends with the turn"
        );
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Ganking));
        assert_eq!(ctx.card(BREAKER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_target_that_left_the_board_gets_nothing_and_the_spell_is_still_spent() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREAKER).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let hand = ctx.zones.hand.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::THEIR_UNIT, hand, 1), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .state_of(fixtures::THEIR_UNIT)
            .is_none_or(|state| state.granted.is_empty()));
        assert!(!ctx.blob.log.iter().any(|line| line.ends_with("this turn")));
        assert_eq!(ctx.card(BREAKER).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn an_action_has_no_window_off_turn_and_a_gear_or_a_battlefield_is_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_BREAKER)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, BREAKER).unwrap();
        for wrong in [
            fixtures::GROUNDS,
            fixtures::HAND_UNIT,
            fixtures::LEGEND_CARD,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(BREAKER).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
