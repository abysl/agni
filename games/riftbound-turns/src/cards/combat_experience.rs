use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell, xp_of};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
pub const LEVEL: u8 = 6;
pub const LEVELED_MIGHT: i16 = 3;

pub fn leveled(ctx: &Ctx, seat: u8) -> bool {
    xp_of(ctx, seat) >= i32::from(LEVEL)
}

pub fn might_for(ctx: &Ctx, seat: u8) -> i16 {
    if leveled(ctx, seat) {
        LEVELED_MIGHT
    } else {
        MIGHT
    }
}

fn experience(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        let delta = might_for(ctx, item.controller);
        might_this_turn(ctx, item, unit, delta, None);
        let level = if leveled(ctx, item.controller) {
            format!(" · Level {LEVEL}")
        } else {
            String::new()
        };
        ctx.narrate(format!(
            "{{card {unit}}} gets +{delta} might this turn{level}"
        ));
    }
    done()
}

pub static CARD: Card = spell(
    "Combat Experience",
    &[Keyword::Reaction],
    &[play(&[a_unit("a unit")], experience)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::priority;
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const EXPERIENCE: u32 = 90;
    const THEIR_EXPERIENCE: u32 = 91;

    fn experience_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Combat Experience", 1, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(experience_card(EXPERIENCE, 0));
        fixture
            .table
            .cards
            .push(experience_card(THEIR_EXPERIENCE, 1));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_reaction_over_one_unit_whose_level_reads_the_controllers_xp() {
        assert!(std::ptr::eq(script_of("Combat Experience").unwrap(), &CARD));
        assert_eq!(CARD.name, "Combat Experience");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].kind, TargetKind::Card);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!((MIGHT, LEVEL, LEVELED_MIGHT), (1, 6, 3));
        let mut fixture = armed(5);
        let ctx = fixture.ctx();
        assert!(!leveled(&ctx, 0), "824.1.c · five XP is short of Level 6");
        assert_eq!(might_for(&ctx, 0), MIGHT);
        assert!(!leveled(&ctx, 1));
        drop(ctx);
        let mut fixture = armed(6);
        let ctx = fixture.ctx();
        assert!(leveled(&ctx, 0));
        assert_eq!(might_for(&ctx, 0), LEVELED_MIGHT);
        assert!(
            !leveled(&ctx, 1),
            "824.1.c.1 · the other seat reads its own XP"
        );
    }

    #[test]
    fn below_level_six_the_chosen_unit_gets_one_might_until_the_turn_ends() {
        let mut fixture = armed(0);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EXPERIENCE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit on the board"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 3, "2 + 1");
        assert_eq!(
            ctx.state_of(fixtures::THEIR_UNIT).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} gets +1 might this turn".to_string()));
        assert_eq!(ctx.card(EXPERIENCE).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn at_level_six_the_unit_gets_three_instead_and_the_level_is_read_as_it_resolves() {
        let mut fixture = armed(6);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EXPERIENCE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 6, "3 + 3, not 3 + 1 + 3");
        assert_eq!(
            ctx.state_of(fixtures::VI)
                .unwrap()
                .might
                .iter()
                .map(|held| held.delta)
                .collect::<Vec<i16>>(),
            [3],
            "824.1.b · the Level text replaces the base effect"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +3 might this turn · Level 6".to_string()));
        drop(ctx);

        let mut fixture = armed(5);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EXPERIENCE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.score_xp(0, 1);
        assert!(
            leveled(&ctx, 0),
            "the sixth XP arrives while the spell waits"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            6,
            "824.1.c · the Level is judged as the spell resolves"
        );
    }

    #[test]
    fn the_other_seat_reacts_on_my_turn_with_its_own_xp() {
        let mut fixture = armed(6);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        fixtures::play_from_hand(&mut ctx, 1, THEIR_EXPERIENCE).unwrap();
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            3,
            "seat 1 has no XP: +1, whatever seat 0 has"
        );
    }

    #[test]
    fn a_gear_or_a_hand_card_is_refused_and_a_target_that_left_is_left_alone() {
        let mut fixture = armed(6);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EXPERIENCE).unwrap();
        for wrong in [fixtures::HAND_GEAR, fixtures::HAND_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(EXPERIENCE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        fixtures::play_from_hand(&mut ctx, 0, EXPERIENCE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::HAND, 1),
                1,
            )
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.state_of(fixtures::THEIR_UNIT).is_none(),
            "356.3.e.5 · nothing lands on a unit that left the board"
        );
        assert_eq!(ctx.card(EXPERIENCE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }
}
