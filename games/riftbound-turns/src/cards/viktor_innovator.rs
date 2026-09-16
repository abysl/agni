use super::faithful_manufactor::play_recruits;
use super::prelude::{a_card_not_a_token, done, on_you_play_card, unit, when, Location};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub fn on_an_opponents_turn(ctx: &Ctx, event: &Event, source: Source) -> bool {
    a_card_not_a_token(ctx, event) && ctx.turn_player() != ctx.controller(source.card)
}

fn innovate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_recruits(ctx, seat, Location::Base(seat), 1);
    done()
}

pub static CARD: Card = unit(
    "Viktor - Innovator",
    &[],
    &[when(on_you_play_card(&[], innovate), on_an_opponents_turn)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::{order_unit, recruits_of};
    use crate::cards::prelude::{play, spawn, spell, Token};
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::engine::settle;
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::Effect;

    const VIKTOR: u32 = 90;
    const THEIR_SPELL: u32 = 91;

    static REACT: Card = spell(
        "Spark",
        &[Keyword::Reaction],
        &[play(&[], |_, _, _| done())],
    );

    fn lab(turn_of: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut viktor = order_unit(VIKTOR, fixtures::BASE, 0, "Viktor - Innovator", 3);
        viktor.domain = vec!["Mind".into()];
        viktor.energy = Some(4);
        viktor.power = Some(1);
        fixture.table.cards.push(viktor);
        let mut theirs = fixtures::spell(THEIR_SPELL, fixtures::HAND, 1, "Their Spark", 1, 0);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        if turn_of == 1 {
            fixture.blob.core_mut().unwrap().advance();
        }
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &REACT);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(VIKTOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn viktor_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(
                |item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == VIKTOR),
            )
            .count()
    }

    #[test]
    fn the_script_is_a_plain_champion_watching_its_controllers_plays_off_turn() {
        assert!(std::ptr::eq(
            script_of("Viktor - Innovator").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlayCard);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_some(), "an opponent's turn");
        let mut fixture = lab(1);
        let ctx = fixture.ctx();
        let source = Source {
            card: VIKTOR,
            ability: 0,
        };
        let event = Event::PlayedSpell {
            item: 1,
            controller: 0,
            nth: 1,
        };
        assert!(on_an_opponents_turn(&ctx, &event, source));
        drop(ctx);
        let mut fixture = lab(0);
        let ctx = fixture.ctx();
        assert!(!on_an_opponents_turn(&ctx, &event, source));
    }

    #[test]
    fn a_reaction_played_on_the_opponents_turn_plays_an_exhausted_recruit_in_the_base() {
        let mut fixture = lab(1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        fixtures::play_from_hand(&mut ctx, 1, THEIR_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        while priority::holder(&ctx) != Some(0) {
            let holder = priority::holder(&ctx).expect("someone holds priority");
            priority::pass(&mut ctx, holder).unwrap();
        }
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "the Reaction sits over their spell"
        );
        assert_eq!(
            viktor_items(&ctx),
            0,
            "377.2.a · a spell is played when it resolves"
        );
        let holder = priority::holder(&ctx).unwrap();
        priority::pass(&mut ctx, holder).unwrap();
        priority::pass(&mut ctx, 1 - holder).unwrap();
        assert_eq!(viktor_items(&ctx), 1, "the resolved Reaction wakes Viktor");
        assert_eq!(
            ctx.blob
                .chain
                .iter()
                .find(|item| matches!(item.kind, ItemKind::Trigger { .. }))
                .unwrap()
                .controller,
            0
        );
        assert!(
            recruits_of(&ctx, 0).is_empty(),
            "the Recruit waits for the trigger"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits.len(), 1);
        let recruit = recruits[0];
        assert_eq!(ctx.location(recruit), Some(Location::Base(0)));
        assert!(ctx.is_token(recruit));
        assert!(ctx.card(recruit).unwrap().exhausted);
        assert_eq!(ctx.controller(recruit), 0);
        assert!(recruits_of(&ctx, 1).is_empty());
        assert!(
            viktor_items(&ctx) == 0 && ctx.blob.queue.is_empty(),
            "185 · the Recruit he plays is a token, not a card, so he does not wake himself"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_token_played_on_the_opponents_turn_is_not_a_card_you_play() {
        let mut fixture = lab(1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 1);
        assert!(spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), false).is_some());
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_card_played_on_your_own_turn_and_the_opponents_own_spell_wake_nobody() {
        let mut fixture = lab(0);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 0);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "your own turn is not an opponent's"
        );
        assert!(recruits_of(&ctx, 0).is_empty());
        drop(ctx);

        let mut fixture = lab(0);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(
            ctx.blob.chain.is_empty(),
            "a unit on your turn is no different"
        );
        drop(ctx);

        let mut fixture = lab(1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "the opponent's spell is not one you play"
        );
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        assert!(ctx.fault.is_none());
    }
}
