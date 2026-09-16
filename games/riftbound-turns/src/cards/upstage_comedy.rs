use super::prelude::{a_unit, card_target, done, play, ready, spell};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};

fn upstage(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if ready(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} readies"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Upstage Comedy",
    &[Keyword::Repeat(REPEAT)],
    &[play(&[a_unit("a unit to ready")], upstage)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const COMEDY: u32 = 90;
    const THEIR_COMEDY: u32 = 91;
    const EXTRA_RUNE: u32 = 100;

    fn comedy(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Upstage Comedy", 2, 0);
        card.domain = vec!["Fury".into()];
        card
    }

    fn stage() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(comedy(COMEDY, 0));
        fixture.table.cards.push(comedy(THEIR_COMEDY, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(EXTRA_RUNE, 0, "Fury", false));
        for unit in [fixtures::VI, fixtures::THEIR_UNIT] {
            fixture.table.card_mut(unit).unwrap().exhausted = true;
        }
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
    fn the_script_is_a_repeatable_sorcery_spell_over_one_unit() {
        assert!(std::ptr::eq(script_of("Upstage Comedy").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Repeat(REPEAT)]);
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(REPEAT.energy, 2);
        assert!(REPEAT.power.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn declined_it_readies_one_unit_of_either_side_and_raises_readied() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, COMEDY).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, ready or not, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "two energy of the four ready runes"
        );
        assert!(ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        assert!(ctx.events.contains(&Event::Readied {
            card: fixtures::THEIR_UNIT,
            by: 0
        }));
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "one unit only without the Repeat"
        );
        assert!(ctx.blob.log.contains(&"{card 81} readies".to_string()));
        assert_eq!(ctx.card(COMEDY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_for_two_more_it_asks_a_second_unit_and_readies_both() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, COMEDY).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(item.spec_counts, [1, 1]);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "two energy for the spell and two for the Repeat"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(!ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Readied { by: 0, .. }))
                .count(),
            2
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_already_ready_unit_readies_nothing_and_an_unaffordable_repeat_is_not_offered() {
        let mut fixture = stage();
        fixture.table.cards.retain(|card| card.id != EXTRA_RUNE);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = false;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, COMEDY).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 1, spec: 0 }),
            "three ready runes pay the spell but not the Repeat"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { .. })));
        assert!(!ctx.blob.log.contains(&"{card 50} readies".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_copy_waits_for_their_turn_and_a_gear_or_a_hand_card_is_refused() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_COMEDY)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, COMEDY).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        for wrong in [fixtures::GROUNDS, fixtures::HAND_UNIT, fixtures::HAND_GEAR] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(COMEDY).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
