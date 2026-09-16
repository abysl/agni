use super::prelude::{card_targets, done, might_this_turn, play, spell, target, FRIENDLY_UNIT};
use super::{Card, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 2;
pub const UNITS: u8 = 2;

pub const TWO_FRIENDLY_UNITS: TargetSpec = target(
    FRIENDLY_UNIT,
    UNITS,
    UNITS,
    TargetKind::Card,
    "two friendly units",
);

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for unit in card_targets(ctx, item) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Back to Back",
    &[Keyword::Reaction],
    &[play(&[TWO_FRIENDLY_UNITS], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts};
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const BACK_TO_BACK: u32 = 90;
    const THEIR_BACK_TO_BACK: u32 = 91;
    const ALLY: u32 = 92;
    const THEIR_ALLY: u32 = 93;
    const THEIR_RUNE: u32 = 46;

    fn back_to_back(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Back to Back", 3, 0);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(back_to_back(BACK_TO_BACK, 0));
        fixture
            .table
            .cards
            .push(back_to_back(THEIR_BACK_TO_BACK, 1));
        fixture.table.cards.push(fixtures::unit(
            ALLY,
            fixtures::BF1,
            0,
            "Legion Rearguard",
            1,
        ));
        fixture.table.cards.push(fixtures::unit(
            THEIR_ALLY,
            fixtures::BF2,
            1,
            "Vanguard Sergeant",
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::rune(THEIR_RUNE, 1, "Mind", false));
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
    fn the_script_is_a_reaction_that_chooses_exactly_two_friendly_units() {
        assert!(std::ptr::eq(script_of("Back to Back").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets, [TWO_FRIENDLY_UNITS]);
        assert_eq!((TWO_FRIENDLY_UNITS.min, TWO_FRIENDLY_UNITS.max), (2, 2));
        assert_eq!(TWO_FRIENDLY_UNITS.filter, FRIENDLY_UNIT);
    }

    #[test]
    fn two_friendly_units_are_picked_one_at_a_time_and_each_gets_two_might_for_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BACK_TO_BACK).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 2, 2));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 92}", "cancel"],
            "only friendly units, in the base or at a battlefield"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose two friendly units (0 of 2)"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "cancel"],
            "a unit is chosen once"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose two friendly units (1 of 2)"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["done", "cancel"],
            "the pair is full and waits for the confirm"
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(ALLY)]
        );
        assert_eq!(ctx.blob.chain[0].spec_counts, [2]);
        assert_eq!(
            might_counter(&ctx, fixtures::VI),
            0,
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 5, "3 + 2");
        assert_eq!(ctx.current_might(ALLY), 3, "1 + 2");
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        assert_eq!(might_counter(&ctx, ALLY), 2);
        assert_eq!(
            ctx.state_of(ALLY).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +2 might this turn".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} gets +2 might this turn".to_string()));
        assert_eq!(ctx.card(BACK_TO_BACK).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(ALLY), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_reacts_with_its_own_pair_and_a_unit_that_left_is_skipped() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        fixtures::play_from_hand(&mut ctx, 1, THEIR_BACK_TO_BACK).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 93}", "cancel"],
            "seat 1's units only"
        );
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 1, "{card 93}").unwrap();
        fixtures::choose(&mut ctx, 1, "done").unwrap();
        assert_eq!(
            ctx.blob.chain[1].targets,
            [
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Card(THEIR_ALLY)
            ]
        );
        ctx.table
            .apply_entry(&fixtures::move_action(THEIR_ALLY, fixtures::TRASH, 1), 1)
            .unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4, "2 + 2");
        assert_eq!(
            might_counter(&ctx, THEIR_ALLY),
            0,
            "356.3.e · the other target is affected on its own"
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert_eq!(
            ctx.card(THEIR_BACK_TO_BACK).unwrap().zone,
            Some(fixtures::TRASH)
        );
    }

    #[test]
    fn one_friendly_unit_cannot_fill_the_pair_and_an_enemy_unit_is_never_a_target() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BACK_TO_BACK).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI, fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is not friendly"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "355.8 · both targets are required"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI, fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the same unit twice is one unit"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(BACK_TO_BACK).unwrap().zone, Some(fixtures::HAND));
        let mut lonely = armed();
        lonely.table.cards.retain(|card| card.id != ALLY);
        lonely.resolve();
        let mut ctx = lonely.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BACK_TO_BACK).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "one friendly unit is offered but the pair can never be filled"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "nothing is left to pick, only the way out"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(BACK_TO_BACK).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
