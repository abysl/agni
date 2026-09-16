use super::prelude::{card_targets, done, might_this_turn, play, spell, target, FRIENDLY_UNIT};
use super::{Card, Cost, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
pub const UNITS: u8 = 2;
pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};

pub const TWO_FRIENDLY_UNITS: TargetSpec = target(
    FRIENDLY_UNIT,
    UNITS,
    UNITS,
    TargetKind::Card,
    "two friendly units",
);

fn bond(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for unit in card_targets(ctx, item) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Bonds of Strength",
    &[Keyword::Reaction, Keyword::Repeat(REPEAT)],
    &[play(&[TWO_FRIENDLY_UNITS], bond)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const BONDS: u32 = 90;
    const ALLY: u32 = 92;
    const SECOND_ALLY: u32 = 93;
    const MY_EXTRA: [u32; 2] = [46, 47];

    fn bonds(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Bonds of Strength", 2, 0);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bonds(BONDS, 0));
        fixture.table.cards.push(fixtures::unit(
            ALLY,
            fixtures::BF1,
            0,
            "Legion Rearguard",
            1,
        ));
        fixture.table.cards.push(fixtures::unit(
            SECOND_ALLY,
            fixtures::BASE,
            0,
            "Vanguard Sergeant",
            2,
        ));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
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
    fn the_script_is_a_repeatable_reaction_choosing_exactly_two_friendly_units() {
        assert!(std::ptr::eq(script_of("Bonds of Strength").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets, [TWO_FRIENDLY_UNITS]);
        assert_eq!((TWO_FRIENDLY_UNITS.min, TWO_FRIENDLY_UNITS.max), (2, 2));
    }

    #[test]
    fn two_friendly_units_each_get_one_might_for_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BONDS).unwrap();
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
            ["{card 50}", "{card 92}", "{card 93}", "cancel"],
            "only friendly units"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(fixtures::labels(&ctx), ["done", "cancel"]);
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(ALLY)]
        );
        assert_eq!(ctx.blob.chain[0].spec_counts, [2]);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 4, "3 + 1");
        assert_eq!(ctx.current_might(ALLY), 2, "1 + 1");
        assert_eq!(might_counter(&ctx, SECOND_ALLY), 0);
        assert_eq!(
            ctx.state_of(ALLY).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} gets +1 might this turn".to_string()));
        assert_eq!(ctx.card(BONDS).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_it_asks_two_pairs_and_a_unit_in_both_pairs_gets_two() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BONDS).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 92}", "{card 93}", "cancel"],
            "the second pair starts afresh"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(item.spec_counts, [2, 2]);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy twice");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 5, "3 + 1 + 1");
        assert_eq!(ctx.current_might(ALLY), 2);
        assert_eq!(ctx.current_might(SECOND_ALLY), 3);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_unit_or_a_lone_pick_is_refused_and_a_unit_that_left_is_skipped() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BONDS).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI, fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI, fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(ALLY, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(
            might_counter(&ctx, ALLY),
            0,
            "356.3.e · the other target is affected on its own"
        );
        assert!(ctx.fault.is_none());
    }
}
