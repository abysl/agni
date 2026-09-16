use super::prelude::{
    a_card, a_friendly_unit, card_target, done, draw, kill, might_this_turn, play, spell,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::{Ctx, Killed};

pub const DRAWS: usize = 1;

pub const ANOTHER_FRIENDLY_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::NotSame(0)]);

pub fn grip(ctx: &mut Ctx, item: &Item, victim: u32, heir: u32) -> bool {
    let might = ctx.current_might(victim);
    if kill(ctx, item, victim) != Killed::Yes {
        ctx.narrate(format!(
            "{{card {victim}}} is not killed · nothing is given"
        ));
        return false;
    }
    let Ok(delta) = i16::try_from(might) else {
        return false;
    };
    if !ctx.on_board(heir) {
        return false;
    }
    might_this_turn(ctx, item, heir, delta, None);
    ctx.narrate(format!(
        "{{card {heir}}} gets {delta:+} Might this turn · {{card {victim}}}'s Might"
    ));
    true
}

fn deathgrip(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if let (Some(victim), Some(heir)) = (card_target(ctx, item, 0), card_target(ctx, item, 1)) {
        grip(ctx, item, victim, heir);
    }
    draw(ctx, seat, DRAWS);
    done()
}

pub static CARD: Card = spell(
    "Deathgrip",
    &[Keyword::Reaction],
    &[play(
        &[
            a_friendly_unit("a friendly unit to kill"),
            a_card(ANOTHER_FRIENDLY_UNIT, "another friendly unit to grow"),
        ],
        deathgrip,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, deathknell, unit as unit_card};
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const DEATHGRIP: u32 = 90;
    const MARTYR: u32 = 91;

    static MARTYR_CARD: Card = unit_card(
        "Martyr",
        &[],
        &[deathknell(&[], |ctx, item, _| {
            prelude::draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    fn wake() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Order".into()],
            ..fixtures::spell(DEATHGRIP, fixtures::HAND, 0, "Deathgrip", 2, 0)
        });
        fixture
            .table
            .cards
            .push(fixtures::unit(MARTYR, fixtures::BF1, 0, "Martyr", 4));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(MARTYR, &MARTYR_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DEATHGRIP).unwrap(),
            &CARD
        ));
        fixture
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_reaction_over_two_different_friendly_units() {
        assert!(std::ptr::eq(script_of("Deathgrip").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, prelude::FRIENDLY_UNIT);
        assert_eq!(ability.targets[1].filter, ANOTHER_FRIENDLY_UNIT);
        assert!(ability
            .targets
            .iter()
            .all(|spec| (spec.min, spec.max, spec.kind) == (1, 1, TargetKind::Card)));
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn the_martyr_dies_and_vi_gets_its_might_this_turn_then_one_card_is_drawn() {
        let mut fixture = wake();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DEATHGRIP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 91}", "cancel"],
            "friendly units only"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "another friendly unit than the one to kill"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(MARTYR), TargetRef::Card(fixtures::VI)]
        );
        assert!(ctx.on_board(MARTYR), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(MARTYR));
        assert_eq!(ctx.card(MARTYR).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.current_might(fixtures::VI), 7, "3 + the Martyr's 4");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +4 Might this turn · {card 91}'s Might".to_string()));
        assert!(
            ctx.events.iter().any(|event| matches!(
                event,
                Event::Died { card, .. } if *card == MARTYR
            )),
            "a kill, not a bounce or banish: {:?}",
            ctx.events
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert_eq!(
            drew(&ctx, 0),
            DRAWS + 1,
            "Deathgrip's draw plus the Martyr's Deathknell"
        );
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.card(DEATHGRIP).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_this_turn_bonus_on_the_victim_is_passed_on_and_the_might_is_read_at_resolution() {
        let mut fixture = wake();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DEATHGRIP).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.might(MARTYR, 2, crate::state::Expiry::EndOfTurn(1), None, 0);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 9, "3 + 6");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_victim_that_left_the_board_gives_nothing_but_the_draw_still_happens() {
        let mut fixture = wake();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DEATHGRIP).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.bounce(MARTYR);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(drew(&ctx, 0), DRAWS, "356.3.e.5 · the draw is not a target");
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "the Martyr came back too");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_same_unit_twice_and_an_enemy_unit_are_refused() {
        let mut fixture = wake();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DEATHGRIP).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[MARTYR]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the victim cannot inherit its own Might"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(DEATHGRIP).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.on_board(MARTYR));
    }
}
