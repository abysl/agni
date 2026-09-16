use super::prelude::{done, might_this_turn, on_you_play_card, unit, when, with_statics};
use super::{Card, Flow, Grant, Item, Keyword, Scope, Source, Stage, Static, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};

pub const DREAM_MIGHT: i16 = 1;

pub fn a_token_unit_of_yours(ctx: &Ctx, event: &Event, source: Source) -> bool {
    matches!(
        event,
        Event::Played { card, controller, kind, .. }
            if kind == KIND_UNIT
                && ctx.is_token(*card)
                && *controller == ctx.controller(source.card)
    )
}

fn a_friendly_token(ctx: &Ctx, _: u32, unit: u32) -> bool {
    ctx.is_token(unit)
}

fn dream(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    might_this_turn(ctx, item, me, DREAM_MIGHT, None);
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Lillia - Protector of Dreams",
        &[],
        &[when(on_you_play_card(&[], dream), a_token_unit_of_yours)],
    ),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: a_friendly_token,
        grants: &[Grant::Keyword(Keyword::Tank)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle, statics};
    use crate::state::{ChainItem, ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const LILLIA: u32 = 90;

    fn lillia(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(LILLIA, zone, seat, "Lillia - Protector of Dreams", 4);
        card.domain = vec!["Calm".into()];
        card.energy = Some(5);
        card
    }

    fn dreamscape(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(lillia(zone, 0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LILLIA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn played(card: u32, controller: u8, kind: &str) -> Event {
        Event::Played {
            card,
            controller,
            kind: kind.into(),
            origin: Origin::Board,
            paid_additional: false,
        }
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_gated_you_play_card_trigger_and_a_tank_aura_over_your_tokens() {
        assert!(std::ptr::eq(
            script_of("Lillia - Protector of Dreams").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlayCard);
        assert!(ability.condition.is_some(), "a token unit, not every card");
        assert!(ability.targets.is_empty() && !ability.optional);
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(
            CARD.statics[0],
            Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Keyword(Keyword::Tank)],
                ..
            }
        ));
        assert_eq!(DREAM_MIGHT, 1);
    }

    #[test]
    fn the_condition_reads_a_token_unit_played_for_her_controller_and_nothing_else() {
        let mut fixture = dreamscape(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let source = Source {
            card: LILLIA,
            ability: 0,
        };
        let mine = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        let theirs = spawn(&mut ctx, 1, Token::Sprite, Location::Base(1), true).unwrap();
        let gold = spawn(&mut ctx, 0, Token::Gold, Location::Base(0), true).unwrap();
        assert!(a_token_unit_of_yours(
            &ctx,
            &played(mine, 0, KIND_UNIT),
            source
        ));
        assert!(
            !a_token_unit_of_yours(&ctx, &played(theirs, 1, KIND_UNIT), source),
            "an opponent's token"
        );
        assert!(
            !a_token_unit_of_yours(&ctx, &played(gold, 0, "Gear"), source),
            "a Gold is a token gear"
        );
        assert!(
            !a_token_unit_of_yours(&ctx, &played(fixtures::HAND_UNIT, 0, KIND_UNIT), source),
            "a card unit is not a token"
        );
    }

    #[test]
    fn her_trigger_gives_her_one_might_this_turn_when_it_resolves() {
        let mut fixture = dreamscape(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let item = ChainItem::new(
            7,
            ItemKind::Trigger {
                source: LILLIA,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(ctx.current_might(LILLIA), 4);
        assert_eq!(dream(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(ctx.current_might(LILLIA), 5);
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(LILLIA), 4, "this turn only");
    }

    #[test]
    fn your_tokens_have_tank_while_she_is_in_play_and_enemy_tokens_and_cards_do_not() {
        let mut fixture = dreamscape(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let mine = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        assert!(ctx.has_keyword(mine, Keyword::Tank));
        assert!(
            !ctx.has_keyword(fixtures::SPRITE, Keyword::Tank),
            "the enemy Sprite is not yours"
        );
        assert!(
            !ctx.has_keyword(fixtures::VI, Keyword::Tank),
            "Vi is a card, not a token"
        );
        assert!(!ctx.has_keyword(LILLIA, Keyword::Tank));
        assert!(statics::grants_on(&ctx, mine)
            .iter()
            .any(|grant| matches!(grant, Grant::Keyword(Keyword::Tank))));
        drop(ctx);
        let mut fixture = dreamscape(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let mine = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        assert!(
            !ctx.has_keyword(mine, Keyword::Tank),
            "in hand she projects nothing"
        );
    }

    #[test]
    fn a_card_unit_you_play_does_not_trigger_her() {
        let mut fixture = dreamscape(fixtures::BASE);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(
            ctx.blob.chain.iter().all(|item| !matches!(
                item.kind,
                ItemKind::Trigger { source, .. } if source == LILLIA
            )),
            "a card unit is not a token unit"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(LILLIA), 4);
    }

    #[test]
    fn a_token_unit_played_for_you_gives_her_one_might_this_turn() {
        let mut fixture = dreamscape(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).is_some());
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(LILLIA), 5);
    }
}
