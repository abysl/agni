use super::prelude::{channel_exhausted, done, play, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const RUNES: usize = 1;

fn roar(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    channel_exhausted(ctx, item.controller, RUNES);
    done()
}

pub static CARD: Card = unit("Stormclaw Ursine", &[Keyword::Tank], &[play(&[], roar)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::ItemKind;

    const URSINE: u32 = 90;
    const TOP_RUNE: u32 = 32;
    const SPARE_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn den() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut ursine = fixtures::unit(URSINE, fixtures::HAND, 0, "Stormclaw Ursine", 6);
        ursine.domain = vec!["Body".into()];
        ursine.energy = Some(7);
        fixture.table.cards.push(ursine);
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn pool(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, seat).count()
    }

    #[test]
    fn the_script_is_a_tank_whose_play_trigger_channels_one_rune_exhausted() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Stormclaw Ursine").unwrap(),
            &CARD
        ));
        let fixture = den();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(URSINE).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Tank]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(RUNES, 1);
    }

    #[test]
    fn playing_the_ursine_channels_the_top_rune_exhausted_when_the_trigger_resolves() {
        let mut fixture = den();
        let mut ctx = fixture.ctx();
        let before = pool(&ctx, 0);
        let deck = ctx.table.held(fixtures::RUNE_DECK, 0).count();
        fixtures::play_from_hand(&mut ctx, 0, URSINE).unwrap();
        assert!(ctx.on_board(URSINE));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "seven energy from eight ready runes"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == URSINE
        ));
        assert!(ctx.blob.prompt.is_none(), "the bear asks nothing");
        assert_eq!(pool(&ctx, 0), before, "the rune waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool(&ctx, 0), before + 1);
        assert_eq!(ctx.table.held(fixtures::RUNE_DECK, 0).count(), deck - 1);
        assert_eq!(ctx.card(TOP_RUNE).unwrap().zone, Some(fixtures::RUNE_POOL));
        assert!(
            ctx.card(TOP_RUNE).unwrap().exhausted,
            "430.2 · it arrives exhausted"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "an exhausted rune adds nothing to pay with"
        );
        assert_eq!(pool(&ctx, 1), 2, "the opponent channels nothing");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_rune_deck_the_trigger_channels_nothing_and_says_so_by_silence() {
        let mut fixture = den();
        fixture
            .table
            .cards
            .retain(|card| !(card.owner == 0 && card.zone == Some(fixtures::RUNE_DECK)));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let before = pool(&ctx, 0);
        fixtures::play_from_hand(&mut ctx, 0, URSINE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(pool(&ctx, 0), before, "430.3 · as many as possible is none");
        assert!(!ctx.blob.log.iter().any(|line| line.contains("channels")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_cannot_play_my_ursine_and_a_bear_already_on_the_board_channels_nothing() {
        let mut fixture = den();
        let mut ctx = fixture.ctx();
        assert!(
            fixtures::play_from_hand(&mut ctx, 1, URSINE).is_err(),
            "not their card"
        );
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            URSINE,
            fixtures::BASE,
            0,
            "Stormclaw Ursine",
            6,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let before = pool(&ctx, 0);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "only a play triggers it");
        assert_eq!(pool(&ctx, 0), before);
    }
}
