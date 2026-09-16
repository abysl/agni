use super::prelude::{a_unit, card_target, done, grant_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const SHIELD: u8 = 3;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        let shielded = grant_this_turn(ctx, unit, Keyword::Shield(SHIELD));
        let tanking = grant_this_turn(ctx, unit, Keyword::Tank);
        if shielded && tanking {
            ctx.narrate(format!(
                "{{card {unit}}} gets [Shield {SHIELD}] and [Tank] this turn"
            ));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Block",
    &[Keyword::Hidden, Keyword::Action],
    &[play(&[a_unit("a unit")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, settle};
    use crate::state::{Expiry, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const BLOCK: u32 = 90;

    fn block(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Block", 2, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(block(BLOCK, 0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_hidden_action_with_one_unit_target() {
        assert!(std::ptr::eq(script_of("Block").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn block_gives_shield_three_and_tank_for_the_turn_and_shield_counts_only_while_defending() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLOCK).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"]
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(
            ctx.effects,
            [Effect::exhaust(41), Effect::exhaust(42)],
            "two energy, no power"
        );
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Tank));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Tank));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Shield(SHIELD)));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "Shield is nothing outside a defence"
        );
        ctx.mark_attacker(fixtures::VI);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "an attacker gets nothing from Shield"
        );
        ctx.clear_designation(fixtures::VI);
        ctx.mark_defender(fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 6, "3 + Shield 3");
        let row = ctx.state_of(fixtures::VI).unwrap();
        assert_eq!(
            row.granted,
            [
                (Keyword::Shield(SHIELD), Expiry::EndOfTurn(1)),
                (Keyword::Tank, Expiry::EndOfTurn(1))
            ]
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets [Shield 3] and [Tank] this turn".to_string()));
        assert_eq!(ctx.card(BLOCK).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Tank));
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Shield(SHIELD)));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_from_facedown_it_is_free_and_may_only_choose_a_unit_at_the_hiding_battlefield() {
        let mut fixture = armed();
        fixture.table.card_mut(BLOCK).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(BLOCK).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BLOCK, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            BLOCK,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "cancel"],
            "811.1.d.2 · only a unit at the hiding battlefield"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in the base is out of reach from facedown"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(
            !ctx.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Annotate { key, .. } if key == "exhausted")),
            "a hidden card reacts for free"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Tank));
        ctx.mark_defender(fixtures::THEIR_UNIT);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 5);
        assert_eq!(ctx.card(BLOCK).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_target_that_left_the_board_gets_nothing_and_a_battlefield_is_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLOCK).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::TRASH, 1),
                1,
            )
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Tank));
        assert!(!ctx.blob.log.iter().any(|line| line.contains("[Tank]")));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }
}
