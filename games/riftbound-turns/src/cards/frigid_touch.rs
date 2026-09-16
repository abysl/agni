use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const CHILL: i16 = -2;
pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};

fn chill(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, CHILL, None);
        ctx.narrate(format!("{{card {unit}}} gets {CHILL} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Frigid Touch",
    &[Keyword::Reaction, Keyword::Repeat(REPEAT)],
    &[play(&[a_unit("a unit")], chill)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::engine::priority;
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const FRIGID: u32 = 90;
    const THEIR_FRIGID: u32 = 91;
    const BRUTE: u32 = 92;
    const MY_EXTRA: [u32; 2] = [46, 47];
    const THEIR_EXTRA: [u32; 2] = [48, 49];

    fn frigid(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Frigid Touch", 2, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(frigid(FRIGID, 0));
        fixture.table.cards.push(frigid(THEIR_FRIGID, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF2, 1, "Brute", 5));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        for rune in THEIR_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Mind", false));
        }
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_repeatable_reaction_over_one_unit() {
        assert!(std::ptr::eq(script_of("Frigid Touch").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
    }

    #[test]
    fn the_chosen_unit_loses_two_might_until_the_turn_ends_with_no_floor() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FRIGID).unwrap();
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
            ["{card 50}", "{card 60}", "{card 81}", "{card 92}", "cancel"],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        assert_eq!(might_counter(&ctx, BRUTE), 0);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(BRUTE), 3, "5 - 2");
        assert_eq!(might_counter(&ctx, BRUTE), -2);
        assert_eq!(
            ctx.state_of(BRUTE).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} gets -2 might this turn".to_string()));
        assert_eq!(ctx.card(FRIGID).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(BRUTE), 5);
        let mut low = armed();
        let mut ctx = low.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FRIGID).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            might_counter(&ctx, fixtures::THEIR_UNIT),
            -2,
            "no minimum is printed: a 2 Might unit takes the whole -2"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_it_chills_twice_and_the_other_seat_reacts_on_my_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FRIGID).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy twice");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(BRUTE), 3);
        assert_eq!(ctx.current_might(fixtures::SPRITE), 1);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        let mut theirs = armed();
        let mut ctx = theirs.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_FRIGID).unwrap();
        fixtures::choose(&mut ctx, 1, "no").unwrap();
        fixtures::choose(&mut ctx, 1, "{card 50}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.current_might(fixtures::VI), 1, "3 - 2");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_battlefield_is_refused_and_a_unit_that_left_is_skipped() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FRIGID).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::ROCKFALL]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BRUTE, fixtures::TRASH, 1), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, BRUTE), 0);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("might this turn")));
        assert_eq!(ctx.card(FRIGID).unwrap().zone, Some(fixtures::TRASH));
    }
}
