use super::prelude::{bounce, card_target, done, optional, play, target, unit, HIDDEN};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT_LIMIT: u8 = 3;

pub const ANOTHER_SMALL_UNIT_AT_A_BATTLEFIELD: Filter = Filter::And(&[
    Filter::Unit,
    Filter::NotSelf,
    Filter::AtBattlefield,
    Filter::MightAtMost(MIGHT_LIMIT),
]);

pub const GUST: TargetSpec = target(
    ANOTHER_SMALL_UNIT_AT_A_BATTLEFIELD,
    0,
    1,
    TargetKind::Card,
    "another unit at a battlefield with 3 Might or less to return",
);

fn sing(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        bounce(ctx, unit);
    }
    done()
}

pub static CARD: Card = unit("Windsinger", HIDDEN, &[optional(play(&[GUST], sing))]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SINGER: u32 = 90;
    const THEIR_BRUTE: u32 = 91;
    const ALLY: u32 = 92;
    const CHAOS_RUNE: u32 = 46;

    fn singer(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(SINGER, zone, 0, "Windsinger", 1)
        }
    }

    fn breeze() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(singer(fixtures::HAND));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Scout", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn pending(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the play trigger is pending")
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_hidden_unit_whose_play_trigger_may_return_one_small_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Windsinger").unwrap(), &CARD));
        assert_eq!(CARD.name, "Windsinger");
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.optional, "you may");
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets, &[GUST]);
        assert_eq!((GUST.min, GUST.max), (0, 1), "the may is a 0-of-1 target");
        assert_eq!(GUST.kind, TargetKind::Card);
        assert_eq!(GUST.filter, ANOTHER_SMALL_UNIT_AT_A_BATTLEFIELD);
        assert_eq!(MIGHT_LIMIT, 3);
    }

    #[test]
    fn playing_it_offers_small_units_at_battlefields_of_either_side_and_the_pick_goes_home() {
        let mut fixture = breeze();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SINGER).unwrap();
        assert_eq!(ctx.location(SINGER), Some(Location::Base(0)));
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {ALLY}}}"),
                "skip".to_string()
            ],
            "the brute is too big, Vi and Jinx are in their bases, the singer is itself"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "{{card {SINGER}}}: choose another unit at a battlefield with 3 Might or less to return (0 of 1)"
            )
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[THEIR_BRUTE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "4 Might is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in its base is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[SINGER]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "another · not itself"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SINGER
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(ALLY)]);
        assert!(ctx.on_board(ALLY), "the return waits for the trigger");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(ALLY));
        assert_eq!(ctx.card(ALLY).unwrap().seat, 0, "to its owner's hand");
        assert!(ctx.on_board(fixtures::SPRITE));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_the_may_puts_nothing_on_the_chain() {
        let mut fixture = breeze();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SINGER).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(ALLY));
        assert!(ctx.on_board(fixtures::SPRITE));
        assert_eq!(ctx.location(SINGER), Some(Location::Base(0)));
    }

    #[test]
    fn a_bounced_enemy_token_ceases_to_exist_and_a_pick_that_grew_survives() {
        let mut fixture = breeze();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SINGER).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::SPRITE).is_none(), "a token vanishes");

        let mut fixture = breeze();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SINGER).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.might(ALLY, 2, until, None, 0);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.on_board(ALLY),
            "356.3.e · at 4 Might it no longer matches the spec"
        );
    }

    #[test]
    fn with_no_small_unit_at_a_battlefield_nothing_is_asked_and_it_still_lands() {
        let mut fixture = breeze();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, ALLY].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SINGER).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "a 0-of-1 spec with no candidate asks nothing"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(SINGER), Some(Location::Base(0)));
        assert!(ctx.on_board(THEIR_BRUTE));
    }
}
