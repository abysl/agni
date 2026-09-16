use super::prelude::{a_friendly_unit, an_enemy_unit, card_target, deal, done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

fn might_as_damage(ctx: &Ctx, unit: u32) -> u8 {
    u8::try_from(ctx.current_might(unit).max(0)).unwrap_or(u8::MAX)
}

fn duel(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let mine = card_target(ctx, item, 0).filter(|unit| ctx.on_board(*unit));
    let theirs = card_target(ctx, item, 1).filter(|unit| ctx.on_board(*unit));
    let (Some(mine), Some(theirs)) = (mine, theirs) else {
        return done();
    };
    let to_theirs = might_as_damage(ctx, mine);
    let to_mine = might_as_damage(ctx, theirs);
    ctx.narrate(format!(
        "{{card {mine}}} and {{card {theirs}}} deal {to_theirs} and {to_mine} to each other"
    ));
    deal(ctx, item, theirs, to_theirs);
    deal(ctx, item, mine, to_mine);
    done()
}

pub static CARD: Card = spell(
    "Challenge",
    &[Keyword::Action],
    &[play(
        &[
            a_friendly_unit("a friendly unit"),
            an_enemy_unit("an enemy unit"),
        ],
        duel,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play, priority};
    use crate::state::{Expiry, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const CHALLENGE: u32 = 90;
    const BRUTE: u32 = 91;

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Body".into()],
            ..fixtures::spell(CHALLENGE, fixtures::HAND, 0, "Challenge", 2, 1)
        });
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Body", false));
        fixture.resolve();
        fixture
    }

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt {
                    card,
                    n,
                    source: Cause::Item(1),
                } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn challenge_is_an_action_choosing_a_friendly_then_an_enemy_unit() {
        assert!(std::ptr::eq(script_of("Challenge").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 2);
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHALLENGE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "the friendly unit first, anywhere"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 91}", "cancel"],
            "then an enemy unit, anywhere"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 1, &[fixtures::VI]),
            Err(Refusal::Illegal(
                crate::engine::legal::Reason::NotALegalTarget
            )),
            "a friendly unit is not an enemy"
        );
    }

    #[test]
    fn the_two_deal_their_current_might_to_each_other_the_buffed_one_winning() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        ctx.might(fixtures::VI, 2, Expiry::EndOfTurn(1), None, 7);
        fixtures::play_from_hand(&mut ctx, 0, CHALLENGE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            damage_events(&ctx),
            [(BRUTE, 5), (fixtures::VI, 4)],
            "each takes the other's current Might"
        );
        assert!(!ctx.on_board(BRUTE), "five damage kills the 4-Might Brute");
        assert!(
            ctx.on_board(fixtures::VI),
            "four does not kill a 5-Might Vi"
        );
        assert_eq!(ctx.card(CHALLENGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn when_either_side_has_left_the_board_nobody_is_dealt_anything() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHALLENGE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        ctx.bounce(fixtures::SPRITE);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            damage_events(&ctx).is_empty(),
            "359.3.e.7 · a duel with one side gone does not execute"
        );
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert_eq!(ctx.card(CHALLENGE).unwrap().zone, Some(fixtures::TRASH));
    }
}
