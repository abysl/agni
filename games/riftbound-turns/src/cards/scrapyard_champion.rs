use super::prelude::{ask_discard, done, draw, legion, play, unit, when};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const DISCARDS: u8 = 2;
pub const DRAWS: usize = 2;

pub fn legion_on_entry(ctx: &Ctx, _: &Event, source: Source) -> bool {
    legion(ctx, ctx.controller(source.card))
}

fn scavenge(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let discarded = stage.0;
    if discarded < DISCARDS {
        if let Some(ask) = ask_discard(ctx, item, discarded + 1) {
            return Flow::Ask(ask);
        }
    }
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = unit(
    "Scrapyard Champion",
    &[Keyword::Legion],
    &[when(play(&[], scavenge), legion_on_entry)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::prompts;
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const CHAMPION: u32 = 90;
    const SPARE_RUNES: [u32; 2] = [46, 47];

    fn scrapyard(legion_on: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut champion = fixtures::unit(CHAMPION, fixtures::HAND, 0, "Scrapyard Champion", 5);
        champion.domain = vec!["Fury".into()];
        champion.energy = Some(5);
        champion.power = Some(1);
        fixture.table.cards.push(champion);
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.seat_mut(0).played_main = legion_on;
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
            ItemKind::Trigger { source, index: 0 } if source == CHAMPION
        ));
        top.id
    }

    fn discarding(ctx: &Ctx, item: u16, stage: u8) {
        assert_eq!(ctx.blob.why, Some(PromptWhy::Discard { item, stage }));
        assert_eq!(ctx.blob.prompt.as_ref().map(|p| p.seat), Some(0));
    }

    fn draws(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
            .count()
    }

    #[test]
    fn the_script_is_a_legion_unit_whose_gated_play_trigger_discards_two_then_draws_two() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Scrapyard Champion").unwrap(),
            &CARD
        ));
        let fixture = scrapyard(true);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CHAMPION).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Legion]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_some(), "812.1.b.1 · the Legion gate");
        assert_eq!((DISCARDS, DRAWS), (2, 2));
    }

    #[test]
    fn with_legion_on_the_champion_asks_for_two_discards_and_then_draws_two() {
        let mut fixture = scrapyard(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHAMPION).unwrap();
        let hand = ctx.hand_of(0).len();
        let item = trigger_id(&ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        discarding(&ctx, item, 1);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
        assert_eq!(fixtures::labels(&ctx).len(), hand);
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::HAND_SPELL)).unwrap();
        discarding(&ctx, item, 2);
        assert_eq!(draws(&ctx), 0, "the draws wait for the second discard");
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::HAND_GEAR)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(draws(&ctx), 2);
        assert_eq!(ctx.hand_of(0).len(), hand, "two out, two in");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn as_the_first_card_of_the_turn_the_champion_triggers_nothing() {
        let mut fixture = scrapyard(false);
        let mut ctx = fixture.ctx();
        assert!(!legion(&ctx, 0));
        fixtures::play_from_hand(&mut ctx, 0, CHAMPION).unwrap();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.on_board(CHAMPION));
        assert!(ctx.blob.chain.is_empty(), "no Legion, no trigger");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(draws(&ctx), 0);
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_one_card_hand_discards_once_and_still_draws_two_and_an_empty_hand_only_draws() {
        let mut fixture = scrapyard(true);
        fixture.table.cards.retain(|card| {
            !(card.owner == 0
                && card.zone == Some(fixtures::HAND)
                && ![CHAMPION, fixtures::HAND_UNIT].contains(&card.id))
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHAMPION).unwrap();
        let item = trigger_id(&ctx);
        fixtures::pass_until_open(&mut ctx);
        discarding(&ctx, item, 1);
        assert_eq!(fixtures::labels(&ctx), [card_label(fixtures::HAND_UNIT)]);
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::HAND_UNIT)).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "409.4 · the second discard has nothing to take"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(draws(&ctx), 2);
        assert_eq!(ctx.hand_of(0).len(), 2);
        drop(ctx);

        let mut fixture = scrapyard(true);
        fixture.table.cards.retain(|card| {
            !(card.owner == 0 && card.zone == Some(fixtures::HAND) && card.id != CHAMPION)
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHAMPION).unwrap();
        assert!(ctx.hand_of(0).is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(draws(&ctx), 2);
        assert_eq!(ctx.hand_of(0).len(), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_cannot_answer_the_discard_and_their_own_legion_is_not_mine() {
        let mut fixture = scrapyard(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHAMPION).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert!(ctx.blob.prompt.is_some());
        drop(ctx);

        let mut fixture = scrapyard(false);
        fixture.blob.seat_mut(1).played_main = true;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHAMPION).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(draws(&ctx), 0);
    }
}
