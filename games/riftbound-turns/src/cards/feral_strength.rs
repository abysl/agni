use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 2;
pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};

fn strengthen(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Feral Strength",
    &[Keyword::Reaction, Keyword::Repeat(REPEAT)],
    &[play(&[a_unit("a unit")], strengthen)],
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

    const FERAL: u32 = 90;
    const THEIR_FERAL: u32 = 91;
    const MY_EXTRA: [u32; 2] = [46, 47];
    const THEIR_EXTRA: [u32; 2] = [48, 49];

    fn feral(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Feral Strength", 2, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(feral(FERAL, 0));
        fixture.table.cards.push(feral(THEIR_FERAL, 1));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        for rune in THEIR_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Calm", false));
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
        assert!(std::ptr::eq(script_of("Feral Strength").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
    }

    #[test]
    fn the_chosen_unit_gets_two_might_until_the_turn_ends() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FERAL).unwrap();
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
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 5, "3 + 2");
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        assert_eq!(
            ctx.state_of(fixtures::VI).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +2 might this turn".to_string()));
        assert_eq!(ctx.card(FERAL).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_it_reacts_to_a_spell_with_two_picks_and_the_same_unit_twice_gets_four() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        fixtures::play_from_hand(&mut ctx, 1, THEIR_FERAL).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_REPEAT as u8
            }),
            "the other seat reacts on my turn"
        );
        fixtures::choose(&mut ctx, 1, "no").unwrap();
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4, "2 + 2");
        let mut twice = armed();
        let mut ctx = twice.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FERAL).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy twice");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::SPRITE), 7, "3 + 2 + 2");
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gear_is_refused_and_an_unaffordable_repeat_is_not_offered() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FERAL).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_GEAR]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(FERAL).unwrap().zone, Some(fixtures::HAND));
        let mut poor = armed();
        poor.table.cards.retain(|card| !MY_EXTRA.contains(&card.id));
        poor.resolve();
        let mut ctx = poor.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FERAL).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 1, spec: 0 }),
            "three ready runes cannot pay four: the repeat is skipped"
        );
    }
}
