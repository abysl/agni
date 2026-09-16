use super::prelude::{card_target, done, kill, play, spell, target};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::{Ctx, Killed};

pub const MIGHT_LIMIT: u8 = 3;

pub const VICTIM: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::AtBattlefield,
        Filter::MightAtMost(MIGHT_LIMIT),
    ]),
    1,
    1,
    TargetKind::Card,
    "a unit at a battlefield with 3 Might or less",
);

fn harvest(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if kill(ctx, item, unit) == Killed::Yes {
        ctx.narrate(format!("{{card {unit}}} dies"));
    }
    done()
}

pub static CARD: Card = spell("Soul Harvest", &[], &[play(&[VICTIM], harvest)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const HARVEST: u32 = 90;
    const THEIR_HARVEST: u32 = 91;
    const BRUTE: u32 = 92;
    const ORDER_RUNE: u32 = 100;

    fn harvest_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Soul Harvest", 2, 1);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(harvest_card(HARVEST, 0));
        fixture.table.cards.push(harvest_card(THEIR_HARVEST, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
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

    #[test]
    fn the_script_is_a_plain_spell_over_a_small_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Soul Harvest").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, [VICTIM]);
        assert_eq!((VICTIM.min, VICTIM.max), (1, 1));
        assert_eq!(MIGHT_LIMIT, 3);
    }

    #[test]
    fn a_three_might_unit_at_a_battlefield_dies_friend_or_foe() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HARVEST).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "cancel"],
            "Vi and the Sprite at 3 Might; the 4-Might Brute and the Jinx in her base are not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert!(ctx.on_board(fixtures::SPRITE), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == fixtures::SPRITE
        )));
        assert!(ctx.blob.log.contains(&"{card 60} dies".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(HARVEST).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_grew_past_three_before_resolution_is_left_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HARVEST).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let spell = ctx.blob.chain[0].clone();
        crate::cards::prelude::might_this_turn(&mut ctx, &spell, fixtures::VI, 1, None);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.on_board(fixtures::VI),
            "356.3.e · at 4 Might she no longer matches"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert_eq!(ctx.card(HARVEST).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn units_in_a_base_or_above_three_might_are_refused_and_the_other_seat_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_HARVEST)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, HARVEST).unwrap();
        for wrong in [BRUTE, fixtures::THEIR_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is too mighty, in a base or not a unit"
            );
        }
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(HARVEST).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
