use super::prelude::{
    a_unit, asking, card_target, done, might_this_turn, play, spell, with_candidates,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const SHRINK: i16 = -4;
pub const QUESTION: &str = "the top card of your deck to recycle";
pub const STAGE_RECYCLE: u8 = 1;

pub fn predicted(ctx: &Ctx, seat: u8) -> Option<u32> {
    let deck = ctx.zones.main_deck?;
    ctx.table.held(deck, seat).last().map(|card| card.id)
}

fn top_of_deck(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != STAGE_RECYCLE {
        return Vec::new();
    }
    predicted(ctx, item.controller)
        .map(TargetRef::Card)
        .into_iter()
        .collect()
}

pub fn predict(ctx: &mut Ctx, item: &Item) -> Flow {
    if ctx.peek_top(item.controller).is_none() {
        ctx.narrate(format!(
            "{{seat {}}} has no card to predict",
            item.controller
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, STAGE_RECYCLE, 0, 1))
}

pub fn recycle_predicted(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = predicted(ctx, seat);
    match ctx.picks().first().copied() {
        Some(card) if top == Some(card) => {
            ctx.recycle_to_bottom(card);
            ctx.narrate(format!(
                "{{seat {seat}}} recycles the top card of their deck"
            ));
        }
        _ => ctx.narrate(format!("{{seat {seat}}} keeps the top card of their deck")),
    }
    done()
}

fn eclipse(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == STAGE_RECYCLE {
        return recycle_predicted(ctx, item);
    }
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, SHRINK, None);
        ctx.narrate(format!("{{card {unit}}} gets {SHRINK} might this turn"));
    }
    predict(ctx, item)
}

pub static CARD: Card = spell(
    "Eclipse",
    &[Keyword::Reaction],
    &[asking(
        with_candidates(play(&[a_unit("a unit")], eclipse), top_of_deck),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::{priority, prompts};
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const ECLIPSE: u32 = 90;
    const THEIR_ECLIPSE: u32 = 91;
    const MY_TOP: u32 = 23;
    const THEIR_TOP: u32 = 25;
    const MIND_RUNE: u32 = 46;

    fn eclipse_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Eclipse", 3, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(eclipse_card(ECLIPSE, 0));
        fixture.table.cards.push(eclipse_card(THEIR_ECLIPSE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 1, "Mind", false));
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn peeks(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Peek { card, seat } => Some((*card, *seat)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_reaction_over_one_unit_that_predicts_as_it_resolves() {
        assert!(std::ptr::eq(script_of("Eclipse").unwrap(), &CARD));
        assert_eq!(CARD.name, "Eclipse");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert_eq!(SHRINK, -4);
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(predicted(&ctx, 0), Some(MY_TOP));
        assert_eq!(predicted(&ctx, 1), Some(THEIR_TOP));
    }

    #[test]
    fn the_unit_loses_four_then_the_controller_looks_at_the_top_card_and_may_recycle_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ECLIPSE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 0, "3 - 4 reads 0");
        assert_eq!(
            ctx.state_of(fixtures::VI).unwrap().might[0],
            crate::state::MightMod {
                delta: -4,
                until: Expiry::EndOfTurn(1),
                src: 1
            }
        );
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_RECYCLE
            }),
            "436.1 · the Predict question opens once the Might lands"
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert!(!prompt.cancel);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {MY_TOP}}}"), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "{card 90}: choose the top card of your deck to recycle (0 of 1)"
        );
        assert_eq!(peeks(&ctx), [(MY_TOP, 0)], "127.4 · the look is private");
        assert_eq!(ctx.blob.chain.len(), 1, "Eclipse waits on the chain");
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, MY_TOP]);
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            deck_of(&ctx, 0),
            [MY_TOP, 20, 21, 22],
            "403.1.a · recycled to the bottom"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: MY_TOP,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} recycles the top card of their deck".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(ECLIPSE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            deck_of(&ctx, 1),
            [24, THEIR_TOP],
            "the other deck is untouched"
        );
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_keeps_the_top_card_where_it_is_and_the_might_stays_gone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ECLIPSE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 0);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, MY_TOP]);
        assert!(!ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Move { card, .. } if *card == MY_TOP
        )));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} keeps the top card of their deck".to_string()));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 0);
        assert_eq!(ctx.card(ECLIPSE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn the_other_seat_reacts_on_my_turn_and_predicts_its_own_deck() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_ECLIPSE).unwrap();
        fixtures::choose(&mut ctx, 1, "{card 50}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.current_might(fixtures::VI), 0);
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {THEIR_TOP}}}"), "skip".to_string()]
        );
        assert_eq!(peeks(&ctx), [(THEIR_TOP, 1)]);
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_TOP}}}")).unwrap();
        assert_eq!(deck_of(&ctx, 1), [THEIR_TOP, 24]);
        assert_eq!(ctx.blob.chain.len(), 1, "my spell still waits beneath");
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, MY_TOP]);
    }

    #[test]
    fn a_stale_target_is_skipped_but_the_predict_still_happens_and_an_empty_deck_asks_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ECLIPSE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::HAND, 1),
                1,
            )
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.state_of(fixtures::THEIR_UNIT).is_none());
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, .. })),
            "436 · the Predict is not a target and still happens"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut bare = armed();
        bare.table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.owner != 0);
        bare.resolve();
        let mut ctx = bare.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ECLIPSE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "436.4 · nothing to look at");
        assert!(ctx.blob.chain.is_empty());
        assert!(peeks(&ctx).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card to predict".to_string()));
        assert_eq!(ctx.current_might(fixtures::VI), 0);
    }

    #[test]
    fn non_units_are_refused_and_a_cancelled_eclipse_goes_home() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ECLIPSE).unwrap();
        for wrong in [fixtures::HAND_GEAR, fixtures::HAND_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(ECLIPSE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(peeks(&ctx).is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
