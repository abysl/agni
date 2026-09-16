use super::prelude::{a_card, a_friendly_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

const CHOSEN: usize = 0;
const OTHER: usize = 1;

pub const ANOTHER_FRIENDLY_UNIT: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::NotSame(CHOSEN as u8),
]);

pub const OTHER_TARGET: TargetSpec = a_card(ANOTHER_FRIENDLY_UNIT, "another friendly unit");

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let (Some(unit), Some(other)) = (
        card_target(ctx, item, CHOSEN),
        card_target(ctx, item, OTHER),
    ) else {
        return done();
    };
    let wanted = ctx.current_might(other);
    let held = ctx.current_might(unit);
    let Ok(delta) = i16::try_from((wanted - held).max(0)) else {
        return done();
    };
    if delta == 0 {
        ctx.narrate(format!(
            "{{card {unit}}} already has at least {{card {other}}}'s {wanted} might"
        ));
        return done();
    }
    might_this_turn(ctx, item, unit, delta, None);
    ctx.narrate(format!(
        "{{card {unit}}} gets +{delta} might this turn · up to {{card {other}}}'s {wanted}"
    ));
    done()
}

pub static CARD: Card = spell(
    "Convergent Mutation",
    &[Keyword::Reaction],
    &[play(
        &[a_friendly_unit("a friendly unit"), OTHER_TARGET],
        resolve,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority};
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const MUTATION: u32 = 90;
    const BRUTE: u32 = 91;
    const RUNT: u32 = 92;
    const MIND_RUNE: u32 = 46;

    fn mutation() -> CardInfo {
        let mut card = fixtures::spell(MUTATION, fixtures::HAND, 0, "Convergent Mutation", 2, 1);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mutation());
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 0, "Mega-Mech", 7));
        fixture.table.cards.push(fixtures::unit(
            RUNT,
            fixtures::BASE,
            0,
            "Legion Rearguard",
            1,
        ));
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
    fn the_script_is_a_reaction_choosing_a_friendly_unit_then_another() {
        assert!(std::ptr::eq(
            script_of("Convergent Mutation").unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let specs = CARD.abilities[0].targets;
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].filter, crate::cards::prelude::FRIENDLY_UNIT);
        assert_eq!(specs[1], OTHER_TARGET);
        assert_eq!((specs[1].min, specs[1].max), (1, 1));
    }

    #[test]
    fn the_chosen_unit_rises_to_the_other_units_might_for_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MUTATION).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 91}", "{card 92}", "cancel"],
            "friendly units only"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 91}", "cancel"],
            "another friendly unit than the first"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(RUNT), TargetRef::Card(BRUTE)]
        );
        assert_eq!(
            ctx.card(MIND_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Mind rune pays the power"
        );
        assert_eq!(might_counter(&ctx, RUNT), 0, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(RUNT), 7, "raised to the Mega-Mech's 7");
        assert_eq!(might_counter(&ctx, RUNT), 6);
        assert_eq!(ctx.current_might(BRUTE), 7, "the other unit is only read");
        assert_eq!(might_counter(&ctx, BRUTE), 0);
        let row = ctx.state_of(RUNT).unwrap();
        assert_eq!(row.might.len(), 1);
        assert_eq!(
            (row.might[0].delta, row.might[0].until),
            (6, Expiry::EndOfTurn(1))
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} gets +6 might this turn · up to {card 91}'s 7".to_string()));
        assert_eq!(ctx.card(MUTATION).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(RUNT), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_might_is_read_at_resolution_and_a_higher_unit_never_shrinks() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        fixtures::play_from_hand(&mut ctx, 0, MUTATION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        let other = crate::state::ChainItem::new(
            9,
            crate::state::ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            crate::state::Origin::Hand,
        );
        might_this_turn(&mut ctx, &other, BRUTE, -5, Some(1));
        assert_eq!(
            ctx.current_might(BRUTE),
            2,
            "a reaction shrank the Mega-Mech first"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "477.3.c · Vi already has 3, so the increase is 0"
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert!(ctx
            .state_of(fixtures::VI)
            .is_none_or(|row| row.might.is_empty()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} already has at least {card 91}'s 2 might".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn a_missing_other_unit_leaves_the_chosen_one_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MUTATION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BRUTE, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(RUNT), 1, "no Might to read, no increase");
        assert_eq!(might_counter(&ctx, RUNT), 0);
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn an_enemy_unit_the_same_unit_twice_and_a_lone_friendly_unit_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MUTATION).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is not friendly"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[RUNT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the other unit must be another unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is not another friendly unit"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(MUTATION).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(MIND_RUNE).unwrap().zone, Some(fixtures::RUNE_POOL));
        let mut lonely = armed();
        lonely
            .table
            .cards
            .retain(|card| card.id != BRUTE && card.id != RUNT);
        lonely.resolve();
        let mut ctx = lonely.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MUTATION).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "no other friendly unit to compare with"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(MUTATION).unwrap().zone, Some(fixtures::HAND));
    }
}
