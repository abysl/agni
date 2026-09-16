use super::prelude::{asking, play, spell, with_candidates};
use super::sabotage::{reveal_and_pick, revealed_matching, AN_OPPONENT};
use super::{Card, Domain, Filter, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const QUESTION: &str = "a Mind card from their hand to recycle";
pub const MIND_IN_HAND: Filter =
    Filter::And(&[Filter::InHand, Filter::Enemy, Filter::Domain(Domain::Mind)]);

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    revealed_matching(ctx, item, MIND_IN_HAND)
}

fn run(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    reveal_and_pick(ctx, item, stage, MIND_IN_HAND)
}

pub static CARD: Card = spell(
    "Decree of Strength",
    &[],
    &[asking(
        with_candidates(play(&[AN_OPPONENT], run), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::{
        cast, face_of, hand_of_three, reveal, spell_card, THEIR_GEAR_CARD, THEIR_MIND_GEAR,
        THEIR_SPELL_CARD, THEIR_UNIT_CARD,
    };
    use crate::cards::sabotage::PICKED;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::prompts;
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, Effect, BOTTOM};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const DECREE: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = hand_of_three();
        fixture
            .table
            .cards
            .push(spell_card(DECREE, 0, "Decree of Strength", "Body", 0));
        fixture
            .table
            .cards
            .push(fixtures::hidden(THEIR_MIND_GEAR, fixtures::HAND, 1));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_reads_like_sabotage_with_a_mind_filter() {
        assert_eq!(CARD.name, "Decree of Strength");
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_pick_is_the_mind_card_and_lands_at_the_bottom_of_the_opponents_deck() {
        let mut fixture = armed();
        let item = cast(&mut fixture, DECREE);
        {
            let ctx = fixture.ctx();
            assert_eq!(ctx.blob.chain[0].stage, crate::cards::sabotage::AWAITED);
            assert_eq!(ctx.blob.chain[0].awaiting.len(), 4);
        }
        assert!(reveal(&mut fixture, THEIR_SPELL_CARD));
        assert!(reveal(&mut fixture, THEIR_GEAR_CARD));
        assert!(reveal(&mut fixture, THEIR_UNIT_CARD));
        assert!(reveal(&mut fixture, THEIR_MIND_GEAR));
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: PICKED
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_SPELL_CARD}}}"),
                format!("{{card {THEIR_MIND_GEAR}}}")
            ],
            "only the Mind cards are offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {DECREE}}}: choose {QUESTION} (0 of 1)")
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SPELL_CARD}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_SPELL_CARD,
            zone: fixtures::MAIN_DECK,
            seat: 1,
            index: BOTTOM
        }));
        assert_eq!(
            ctx.table
                .held(fixtures::MAIN_DECK, 1)
                .map(|card| card.id)
                .collect::<Vec<_>>(),
            [THEIR_SPELL_CARD, 24, 25]
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lone_mind_card_is_picked_without_a_click() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.id != THEIR_MIND_GEAR);
        fixture.resolve();
        cast(&mut fixture, DECREE);
        assert!(reveal(&mut fixture, THEIR_SPELL_CARD));
        assert!(reveal(&mut fixture, THEIR_GEAR_CARD));
        assert!(reveal(&mut fixture, THEIR_UNIT_CARD));
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none(), "one candidate answers itself");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(THEIR_SPELL_CARD).unwrap().zone,
            Some(fixtures::MAIN_DECK)
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{seat 1}} recycles {{card {THEIR_SPELL_CARD}}}")));
    }

    #[test]
    fn a_hand_without_a_mind_card_ends_the_spell_after_the_reveal() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![THEIR_SPELL_CARD, THEIR_MIND_GEAR].contains(&card.id));
        fixture.resolve();
        cast(&mut fixture, DECREE);
        assert!(reveal(&mut fixture, THEIR_GEAR_CARD));
        {
            let ctx = fixture.ctx();
            assert_eq!(ctx.blob.chain[0].awaiting, [THEIR_UNIT_CARD]);
        }
        assert!(reveal(&mut fixture, THEIR_UNIT_CARD));
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        let stale = Action::Reveal {
            card: THEIR_UNIT_CARD,
            face: face_of(THEIR_UNIT_CARD),
        };
        drop(ctx);
        let mut ctx = fixture.ctx_for(1, &stale);
        assert!(!crate::engine::chain::face_arrived(&mut ctx, THEIR_UNIT_CARD).unwrap());
    }
}
