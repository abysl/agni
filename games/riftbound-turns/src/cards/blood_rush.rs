use super::prelude::{a_unit, card_target, done, play, spell};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::Expiry;

pub const ASSAULT: u8 = 2;
pub const REPEAT: Cost = Cost {
    energy: 1,
    power: &[],
};

fn rush(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if ctx.grant(unit, Keyword::Assault(ASSAULT), Expiry::Permanent) {
            ctx.narrate(format!("{{card {unit}}} gets [Assault {ASSAULT}]"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Blood Rush",
    &[Keyword::Action, Keyword::Repeat(REPEAT)],
    &[play(&[a_unit("a unit")], rush)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const RUSH: u32 = 90;
    const THEIR_RUSH: u32 = 91;

    fn rush_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Blood Rush", 1, 0);
        card.domain = vec!["Fury".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rush_card(RUSH, 0));
        fixture.table.cards.push(rush_card(THEIR_RUSH, 1));
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
    fn the_script_is_a_repeatable_action_over_one_unit_and_prints_no_assault_of_its_own() {
        assert!(std::ptr::eq(script_of("Blood Rush").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert!(!CARD.has_keyword(Keyword::Assault(ASSAULT)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn the_grant_is_permanent_and_counts_only_while_the_unit_attacks() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
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
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)));
        assert_eq!(
            ctx.state_of(fixtures::THEIR_UNIT).unwrap().granted,
            [(Keyword::Assault(ASSAULT), Expiry::Permanent)]
        );
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        ctx.mark_attacker(fixtures::THEIR_UNIT);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4, "2 + Assault 2");
        ctx.expire(Expiry::EndOfTurn(1));
        assert!(
            ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)),
            "the grant outlives the turn"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} gets [Assault 2]".to_string()));
        assert_eq!(ctx.card(RUSH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_it_asks_two_units_and_the_same_unit_twice_stacks_two_assaults() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(item.spec_counts, [1, 1]);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "one energy for the spell and one for the repeat"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.state_of(fixtures::VI).unwrap().granted,
            [
                (Keyword::Assault(ASSAULT), Expiry::Permanent),
                (Keyword::Assault(ASSAULT), Expiry::Permanent)
            ]
        );
        ctx.mark_attacker(fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 7, "3 + 2 + 2");
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| line == &"{card 50} gets [Assault 2]")
                .count(),
            2
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_action_has_no_window_off_turn_and_a_gear_or_a_unit_in_hand_is_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_RUSH)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(RUSH).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
