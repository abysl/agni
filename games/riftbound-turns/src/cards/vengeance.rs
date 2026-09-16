use super::prelude::{a_unit, card_target, done, kill, play, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::{Ctx, Killed};

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if kill(ctx, item, unit) == Killed::Yes {
            ctx.narrate(format!("{{card {unit}}} dies"));
        }
    }
    done()
}

pub static CARD: Card = spell("Vengeance", &[], &[play(&[a_unit("a unit")], resolve)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const VENGEANCE: u32 = 90;
    const THEIR_VENGEANCE: u32 = 91;
    const ORDER_A: u32 = 100;
    const ORDER_B: u32 = 101;

    fn vengeance(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Vengeance", 4, 2);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(vengeance(VENGEANCE, 0));
        fixture.table.cards.push(vengeance(THEIR_VENGEANCE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_A, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_B, 0, "Order", false));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_spell_that_targets_one_unit_anywhere() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Vengeance").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Vengeance");
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, UNIT);
    }

    #[test]
    fn vengeance_kills_the_chosen_unit_in_a_base_or_at_a_battlefield() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, VENGEANCE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit on the board, friendly or enemy, in a base or at a battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(ctx.events.contains(&Event::Chosen {
            card: fixtures::THEIR_UNIT,
            by: 0,
            item: 1
        }));
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "nothing happens before it resolves"
        );
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().seat,
            1,
            "it goes to its owner's trash"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert!(ctx.blob.log.contains(&"{card 81} dies".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(VENGEANCE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_token_at_a_battlefield_dies_and_vanishes_and_the_battlefield_falls_open() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, VENGEANCE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        both_pass(&mut ctx);
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert!(ctx
            .effects
            .contains(&agni_plugin_sdk::decide::Effect::Despawn {
                card: fixtures::SPRITE
            }));
        assert!(ctx.blob.log.contains(&"{card 60} dies".to_string()));
        assert_eq!(
            ctx.blob.holder(fixtures::BF2),
            None,
            "the battlefield falls open once no unit of its holder stands there"
        );
    }

    #[test]
    fn a_target_that_left_the_board_before_resolution_is_left_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, VENGEANCE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::HAND, 1),
                1,
            )
            .unwrap();
        both_pass(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert!(!ctx.blob.log.contains(&"{card 81} dies".to_string()));
        assert_eq!(ctx.card(VENGEANCE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn non_units_are_refused_and_the_spell_needs_its_two_order_power() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, VENGEANCE).unwrap();
        for wrong in [fixtures::GROUNDS, fixtures::HAND_UNIT, ORDER_A, VENGEANCE] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(VENGEANCE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_VENGEANCE)),
            Err(Refusal::NotYourTurn),
            "a plain spell is the turn player's"
        );
        drop(ctx);

        let mut poor = armed();
        poor.table.card_mut(ORDER_B).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, VENGEANCE)),
            Err(Refusal::NoPowerOf),
            "four energy is there but only one Order rune"
        );
    }
}
