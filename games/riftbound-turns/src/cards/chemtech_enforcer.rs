use super::prelude::{ask_discard, done, play, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

const STAGE_DISCARDED: u8 = 1;

fn enforce(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == STAGE_DISCARDED {
        return done();
    }
    match ask_discard(ctx, item, STAGE_DISCARDED) {
        Some(ask) => Flow::Ask(ask),
        None => done(),
    }
}

pub static CARD: Card = unit(
    "Chemtech Enforcer",
    &[Keyword::Assault(2)],
    &[play(&[], enforce)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, prompts};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const ENFORCER: u32 = 90;

    fn precinct() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut enforcer = fixtures::unit(ENFORCER, fixtures::HAND, 0, "Chemtech Enforcer", 2);
        enforcer.domain = vec!["Fury".into()];
        enforcer.energy = Some(2);
        fixture.table.cards.push(enforcer);
        fixture.resolve();
        fixture
    }

    fn card_label(card: u32) -> String {
        format!("{{card {card}}}")
    }

    fn trigger_id(ctx: &Ctx) -> u16 {
        let top = ctx
            .blob
            .chain
            .last()
            .expect("the play trigger is on the chain");
        assert!(matches!(
            top.kind,
            ItemKind::Trigger { source, index: 0 } if source == ENFORCER
        ));
        top.id
    }

    #[test]
    fn the_script_is_an_assault_two_unit_whose_play_trigger_asks_for_one_discard() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Chemtech Enforcer").unwrap(),
            &CARD
        ));
        let fixture = precinct();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ENFORCER).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Assault(2)]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
    }

    #[test]
    fn playing_the_enforcer_asks_its_controller_for_a_discard_when_the_trigger_resolves() {
        let mut fixture = precinct();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ENFORCER).unwrap();
        let hand = ctx.hand_of(0).len();
        let item = trigger_id(&ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item,
                stage: STAGE_DISCARDED
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|p| p.seat), Some(0));
        assert_eq!(
            ctx.blob.chain.last().map(|top| top.status),
            Some(ItemStatus::Resolving)
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
        let offered = fixtures::labels(&ctx);
        assert_eq!(offered.len(), hand, "every card in hand, no closers");
        assert!(
            !offered.contains(&card_label(ENFORCER)),
            "on the board, not in hand"
        );
        assert_eq!(
            phases::end_turn(&mut ctx),
            Err(Refusal::PromptOpen),
            "the turn cannot end around the discard"
        );
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::HAND_GEAR)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.chain.is_empty(),
            "one discard and the trigger is done"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} discards {{card {}}}",
            fixtures::HAND_GEAR
        )));
        assert!(ctx.on_board(ENFORCER));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_cannot_discard_for_me_and_only_the_hand_is_offered() {
        let mut fixture = precinct();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ENFORCER).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        let count = fixtures::labels(&ctx).len() as u16;
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt,
                    option: count
                }
            ),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: count,
                count: usize::from(count)
            }))
        );
        assert!(ctx.blob.prompt.is_some());
        assert_eq!(ctx.hand_of(0).len(), usize::from(count));
    }

    #[test]
    fn with_an_empty_hand_there_is_nothing_to_discard_and_the_trigger_just_finishes() {
        let mut fixture = precinct();
        fixture.table.cards.retain(|card| {
            !(card.owner == 0 && card.zone == Some(fixtures::HAND) && card.id != ENFORCER)
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ENFORCER).unwrap();
        assert!(ctx.hand_of(0).is_empty());
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "409.4 · nothing to discard is ignored"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(ENFORCER));
        assert!(ctx.fault.is_none());
    }
}
