use super::prelude::{a_unit_at_a_battlefield, card_target, deal, done, play, spell};
use super::{base_name, Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const NAME: &str = "Consuming Curse";
pub const DAMAGE: u8 = 2;
pub const BONUS_PER_NAMESAKE: u8 = 1;

pub fn namesakes_in_trash(ctx: &Ctx, seat: u8) -> u8 {
    let count = ctx
        .trash_of(seat)
        .into_iter()
        .filter_map(|card| ctx.card(card))
        .filter(|held| base_name(&held.name) == NAME)
        .count();
    u8::try_from(count).unwrap_or(u8::MAX)
}

pub fn damage_with(namesakes: u8) -> u8 {
    DAMAGE.saturating_add(namesakes.saturating_mul(BONUS_PER_NAMESAKE))
}

fn curse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let namesakes = namesakes_in_trash(ctx, item.controller);
    let amount = damage_with(namesakes);
    if deal(ctx, item, unit, amount) {
        ctx.narrate(format!(
            "{{card {unit}}} takes {amount} · {namesakes} Bonus Damage from the trash"
        ));
    }
    done()
}

pub static CARD: Card = spell(
    NAME,
    &[Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        curse,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const CURSE: u32 = 90;
    const SPENT: [u32; 2] = [91, 92];
    const THEIR_SPENT: u32 = 93;
    const BRUTE: u32 = 94;

    fn curse_card(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Fury".into()],
            ..fixtures::spell(id, zone, seat, NAME, 2, 0)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(curse_card(CURSE, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(CURSE).unwrap(), &CARD));
        fixture
    }

    fn with_spent_curses(mut fixture: Fixture) -> Fixture {
        for id in SPENT {
            fixture.table.cards.push(curse_card(id, fixtures::TRASH, 0));
        }
        fixture
            .table
            .cards
            .push(curse_card(THEIR_SPENT, fixtures::TRASH, 1));
        fixture.resolve();
        fixture
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, CURSE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_script_is_an_action_over_a_unit_at_a_battlefield_with_a_trash_scaled_deal() {
        assert!(std::ptr::eq(script_of(NAME).unwrap(), &CARD));
        assert_eq!(CARD.name, "Consuming Curse");
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(damage_with(0), 2);
        assert_eq!(damage_with(3), 5);
        assert_eq!(damage_with(u8::MAX), u8::MAX, "saturates rather than wraps");
    }

    #[test]
    fn with_an_empty_trash_it_deals_two_and_the_spell_on_the_chain_is_not_a_namesake() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(namesakes_in_trash(&ctx, 0), 0);
        cast_at(&mut ctx, BRUTE);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: 2,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(BRUTE), 2);
        assert!(ctx.on_board(BRUTE), "two on four Might survives");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 94} takes 2 · 0 Bonus Damage from the trash".to_string()));
        assert_eq!(
            ctx.card(CURSE).unwrap().zone,
            Some(fixtures::TRASH),
            "the first copy lands in the trash for the next"
        );
        assert_eq!(namesakes_in_trash(&ctx, 0), 1);
    }

    #[test]
    fn each_namesake_in_your_own_trash_adds_one_bonus_damage_read_at_resolution() {
        let mut fixture = with_spent_curses(armed());
        let mut ctx = fixture.ctx();
        assert_eq!(namesakes_in_trash(&ctx, 0), 2);
        assert_eq!(
            namesakes_in_trash(&ctx, 1),
            1,
            "the opponent's copy counts for the opponent only"
        );
        cast_at(&mut ctx, BRUTE);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: 4,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(BRUTE), "four kills the 4-Might Brute");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 94} takes 4 · 2 Bonus Damage from the trash".to_string()));
        let mut late = armed();
        let mut ctx = late.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CURSE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.trash(fixtures::HAND_SPELL);
        let spare = ctx.table.card_mut(fixtures::HAND_SPELL).unwrap();
        spare.name = NAME.into();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.damage_on(BRUTE),
            3,
            "a namesake that reached the trash in response counts"
        );
    }

    #[test]
    fn a_unit_in_a_base_is_refused_and_a_target_that_left_takes_nothing() {
        let mut fixture = with_spent_curses(armed());
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CURSE).unwrap();
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base or not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.recall(BRUTE, false);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert_eq!(ctx.card(CURSE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }
}
