use super::prelude::{ask_discard, done, play, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DISCARDS: u8 = 2;

fn demolish(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let discarded = stage.0;
    if discarded >= DISCARDS {
        return done();
    }
    match ask_discard(ctx, item, discarded + 1) {
        Some(ask) => Flow::Ask(ask),
        None => done(),
    }
}

pub static CARD: Card = unit(
    "Jinx - Demolitionist",
    &[Keyword::Accelerate, Keyword::Assault(2)],
    &[play(&[], demolish)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent};
    use crate::engine::{act, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const JINX: u32 = 90;
    const SPARE_RUNES: [u32; 2] = [46, 47];

    fn workshop() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut jinx = fixtures::unit(JINX, fixtures::HAND, 0, "Jinx - Demolitionist", 4);
        jinx.domain = vec!["Fury".into()];
        jinx.energy = Some(3);
        jinx.power = Some(1);
        fixture.table.cards.push(jinx);
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
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
            ItemKind::Trigger { source, index: 0 } if source == JINX
        ));
        top.id
    }

    fn discarding(ctx: &Ctx, item: u16, stage: u8) {
        assert_eq!(ctx.blob.why, Some(PromptWhy::Discard { item, stage }));
        assert_eq!(ctx.blob.prompt.as_ref().map(|p| p.seat), Some(0));
        assert_eq!(
            prompts::status(ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
    }

    #[test]
    fn the_script_is_an_accelerate_assault_two_champion_whose_play_trigger_discards_two() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Jinx - Demolitionist").unwrap(),
            &CARD
        ));
        let fixture = workshop();
        assert!(std::ptr::eq(fixture.scripts.of_card(JINX).unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Accelerate, Keyword::Assault(2)]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(DISCARDS, 2);
    }

    #[test]
    fn declining_accelerate_she_enters_exhausted_and_her_trigger_asks_for_two_discards_in_turn() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JINX).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "Accelerate is offered with six ready runes: {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.on_board(JINX));
        assert!(ctx.card(JINX).unwrap().exhausted);
        let hand = ctx.hand_of(0).len();
        let item = trigger_id(&ctx);
        fixtures::pass_until_open(&mut ctx);
        discarding(&ctx, item, 1);
        assert_eq!(fixtures::labels(&ctx).len(), hand, "every card in hand");
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        discarding(&ctx, item, 2);
        assert_eq!(
            fixtures::labels(&ctx).len(),
            hand - 1,
            "the first discard is gone from the offer"
        );
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::HAND_GEAR)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.blob.prompt.is_none(), "two is all she asks");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand - 2);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} discards {{card {}}}",
            fixtures::HAND_SPELL
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} discards {{card {}}}",
            fixtures::HAND_GEAR
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_hand_to_trash_drag_answers_each_discard_as_a_gesture() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JINX).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        let item = trigger_id(&ctx);
        fixtures::pass_until_open(&mut ctx);
        discarding(&ctx, item, 1);
        let table = ctx.table.clone();
        let blob = ctx.blob.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = blob;
        let action = fixtures::move_action(fixtures::HAND_UNIT, fixtures::TRASH, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.unwrap();
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Ok(Intent::Discard {
                card: fixtures::HAND_UNIT
            })
        );
        act(
            &mut ctx,
            0,
            Intent::Discard {
                card: fixtures::HAND_UNIT,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        discarding(&ctx, item, 2);
        assert_eq!(
            ctx.card(fixtures::HAND_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::HAND_GEAR)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_other_seat_cannot_answer_and_a_one_card_hand_discards_once_then_stops() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JINX).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::pass_until_open(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert!(ctx.blob.prompt.is_some());
        drop(ctx);

        let mut fixture = workshop();
        fixture.table.cards.retain(|card| {
            !(card.owner == 0
                && card.zone == Some(fixtures::HAND)
                && ![JINX, fixtures::HAND_GEAR].contains(&card.id))
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JINX).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        let item = trigger_id(&ctx);
        fixtures::pass_until_open(&mut ctx);
        discarding(&ctx, item, 1);
        assert_eq!(fixtures::labels(&ctx), [card_label(fixtures::HAND_GEAR)]);
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::HAND_GEAR)).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "409.4 · the second discard has nothing to take"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.hand_of(0).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_hand_the_trigger_asks_nothing_at_all() {
        let mut fixture = workshop();
        fixture.table.cards.retain(|card| {
            !(card.owner == 0 && card.zone == Some(fixtures::HAND) && card.id != JINX)
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JINX).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.on_board(JINX));
        assert!(
            !ctx.card(JINX).unwrap().exhausted,
            "Accelerate paid: she enters ready"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }
}
