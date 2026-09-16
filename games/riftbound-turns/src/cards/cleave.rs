use super::prelude::{a_unit, card_target, done, grant_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 3;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if grant_this_turn(ctx, unit, Keyword::Assault(ASSAULT)) {
            ctx.narrate(format!(
                "{{card {unit}}} gets [Assault {ASSAULT}] this turn"
            ));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Cleave",
    &[Keyword::Action],
    &[play(&[a_unit("a unit")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const CLEAVE: u32 = 90;
    const THEIR_CLEAVE: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::spell(CLEAVE, fixtures::HAND, 0, "Cleave", 1, 0));
        fixture.table.cards.push(fixtures::spell(
            THEIR_CLEAVE,
            fixtures::HAND,
            1,
            "Cleave",
            1,
            0,
        ));
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
    fn the_script_is_an_action_with_one_unit_target() {
        assert!(std::ptr::eq(script_of("Cleave").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Hidden));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].kind, TargetKind::Card);
    }

    #[test]
    fn cleave_gives_the_chosen_unit_assault_three_that_counts_only_while_it_attacks_and_ends_with_the_turn(
    ) {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CLEAVE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(
            !ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)));
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "Assault is nothing outside an attack"
        );
        ctx.mark_attacker(fixtures::THEIR_UNIT);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 5, "2 + Assault 3");
        ctx.clear_designation(fixtures::THEIR_UNIT);
        ctx.mark_defender(fixtures::THEIR_UNIT);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "a defender gets nothing from Assault"
        );
        let row = ctx.state_of(fixtures::THEIR_UNIT).unwrap();
        assert_eq!(
            row.granted,
            [(Keyword::Assault(ASSAULT), Expiry::EndOfTurn(1))]
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} gets [Assault 3] this turn".to_string()));
        assert_eq!(ctx.card(CLEAVE).unwrap().zone, Some(fixtures::TRASH));
        ctx.clear_designation(fixtures::THEIR_UNIT);
        ctx.mark_attacker(fixtures::THEIR_UNIT);
        ctx.expire(Expiry::EndOfTurn(1));
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_left_the_board_gets_nothing_and_a_grant_stacks_with_a_printed_assault() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CLEAVE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::TRASH, 1),
                1,
            )
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("gets [Assault")));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.grant(fixtures::VI, Keyword::Assault(1), Expiry::Permanent);
        fixtures::play_from_hand(&mut ctx, 0, CLEAVE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        ctx.mark_attacker(fixtures::VI);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            7,
            "3 + Assault 1 + Assault 3"
        );
    }

    #[test]
    fn cleave_is_refused_off_turn_and_needs_a_unit_on_the_board() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CLEAVE)),
            Err(Refusal::NotYourTurn),
            "an Action has no window in the other seat's neutral open state"
        );
        fixtures::play_from_hand(&mut ctx, 0, CLEAVE).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a battlefield is not a unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in hand is not on the board"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "one unit is required"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CLEAVE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
