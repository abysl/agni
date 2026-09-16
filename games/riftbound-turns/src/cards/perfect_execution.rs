use super::prelude::{a_unit, card_target, done, grant_this_turn, play, ready, spell};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 3;
pub const FLOW: Cost = Cost {
    energy: 3,
    power: &[Power::Domain(Domain::Fury)],
};

fn execute(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if ready(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is readied"));
    }
    if grant_this_turn(ctx, unit, Keyword::Assault(ASSAULT)) {
        ctx.narrate(format!(
            "{{card {unit}}} gets [Assault {ASSAULT}] this turn"
        ));
    }
    done()
}

pub static CARD: Card = spell(
    "Perfect Execution",
    &[Keyword::Flow(FLOW)],
    &[play(
        &[a_unit("a unit to ready and give Assault 3")],
        execute,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{Expiry, Leave, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const EXECUTION: u32 = 90;
    const THEIR_EXECUTION: u32 = 91;
    const SPARE_RUNE: u32 = 100;

    fn execution(id: u32, zone: u16, seat: u8) -> CardInfo {
        fixtures::spell(id, zone, seat, "Perfect Execution", 3, 1)
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(execution(EXECUTION, zone, 0));
        fixture
            .table
            .cards
            .push(execution(THEIR_EXECUTION, fixtures::HAND, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(SPARE_RUNE, 0, "Fury", false));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
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
    fn the_script_is_a_flow_sorcery_over_any_unit() {
        assert!(std::ptr::eq(script_of("Perfect Execution").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!(ASSAULT, 3);
    }

    #[test]
    fn the_exhausted_unit_is_readied_and_swings_with_three_more_until_the_turn_ends() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EXECUTION).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "nothing before it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.events.contains(&Event::Readied {
            card: fixtures::VI,
            by: 0
        }));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Assault(ASSAULT)));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "Assault waits for an attack"
        );
        ctx.mark_attacker(fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 6, "3 + Assault 3");
        assert_eq!(
            ctx.state_of(fixtures::VI).unwrap().granted,
            [(Keyword::Assault(ASSAULT), Expiry::EndOfTurn(1))]
        );
        assert!(ctx.blob.log.contains(&"{card 50} is readied".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets [Assault 3] this turn".to_string()));
        assert_eq!(ctx.card(EXECUTION).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Assault(ASSAULT)));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_ready_unit_only_gains_the_assault_and_from_the_trash_the_spell_is_banished() {
        let mut fixture = armed(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            EXECUTION,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Readied { .. })),
            "a ready unit is not readied again"
        );
        assert!(ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)));
        assert!(!ctx.blob.log.contains(&"{card 81} is readied".to_string()));
        assert_eq!(ctx.banished_of(0), [EXECUTION]);
        assert!(ctx.trash_of(0).is_empty());
    }

    #[test]
    fn a_gear_is_refused_a_unit_gone_before_resolution_gets_nothing_and_the_other_seat_waits() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_EXECUTION)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, EXECUTION).unwrap();
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
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::HAND, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Assault(ASSAULT)));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { .. })));
        assert_eq!(ctx.card(EXECUTION).unwrap().zone, Some(fixtures::TRASH));
    }
}
