use super::prelude::{a_unit, card_target, done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const FACTOR: u8 = 2;

pub fn doubled(amount: u8) -> u8 {
    amount.saturating_mul(FACTOR)
}

pub fn double_damage_to_this_turn(ctx: &mut Ctx, _: &Item, unit: u32) {
    if ctx.multiply_damage_this_turn(unit, FACTOR) {
        ctx.narrate(format!(
            "{{card {unit}}} is trapped · all damage dealt to it this turn is doubled"
        ));
    }
}

fn trap(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        double_damage_to_this_turn(ctx, item, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Lotus Trap",
    &[Keyword::Hidden, Keyword::Reaction],
    &[play(&[a_unit("a unit")], trap)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const TRAP: u32 = 90;
    const THEIR_TRAP: u32 = 91;
    const FURY_RUNE: u32 = 46;

    fn lotus_trap(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Lotus Trap", 2, 0);
        card.domain = vec!["Fury".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(lotus_trap(TRAP, 0));
        fixture.table.cards.push(lotus_trap(THEIR_TRAP, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(FURY_RUNE, 1, "Fury", false));
        fixture.resolve();
        fixture
    }

    fn unpaid(ctx: &Ctx) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .count()
    }

    #[test]
    fn the_script_is_a_hidden_reaction_over_one_unit() {
        assert!(std::ptr::eq(script_of("Lotus Trap").unwrap(), &CARD));
        assert_eq!(CARD.name, "Lotus Trap");
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(CARD.abilities[0].targets[0].filter, UNIT);
        assert_eq!(FACTOR, 2);
        assert_eq!(doubled(1), 2);
        assert_eq!(doubled(3), 6);
        assert_eq!(doubled(0), 0);
        assert_eq!(doubled(200), u8::MAX, "saturating, never wrapping");
    }

    #[test]
    fn from_hand_it_chooses_any_unit_for_two_energy_and_marks_it_as_it_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TRAP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"]
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(ctx.blob.chain[0].origin, Origin::Hand);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy off three");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(
            &"{card 81} is trapped · all damage dealt to it this turn is doubled".to_string()
        ));
        assert_eq!(ctx.card(TRAP).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_reacts_to_my_spell_with_its_own_trap() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_TRAP).unwrap();
        fixtures::choose(&mut ctx, 1, "{card 50}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert!(ctx.blob.log.contains(
            &"{card 50} is trapped · all damage dealt to it this turn is doubled".to_string()
        ));
    }

    #[test]
    fn played_from_facedown_it_costs_nothing_and_reaches_only_the_hiding_battlefield() {
        let mut fixture = armed();
        fixture.table.card_mut(TRAP).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(TRAP).hidden_at = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(TRAP, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            TRAP,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "cancel"],
            "737.1.d · only a unit at the hiding battlefield"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Vi in the base is out of reach"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        assert_eq!(
            unpaid(&ctx),
            0,
            "737.1.b · a hidden card reacts for nothing"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.log.contains(
            &"{card 81} is trapped · all damage dealt to it this turn is doubled".to_string()
        ));
        assert_eq!(ctx.card(TRAP).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn non_units_are_refused_and_a_cancelled_trap_goes_home() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TRAP).unwrap();
        for wrong in [fixtures::HAND_GEAR, fixtures::HAND_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(TRAP).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn every_damage_dealt_to_the_trapped_unit_this_turn_is_doubled_until_the_turn_ends() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(5);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, TRAP).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.damage(fixtures::THEIR_UNIT, 1, Cause::Rule));
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 2, "1 doubles to 2");
        settle(&mut ctx).unwrap();
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "two damage is not lethal for the surviving trapped unit"
        );
        ctx.expire(crate::state::Expiry::EndOfTurn(1));
        assert!(ctx.damage(fixtures::THEIR_UNIT, 1, Cause::Rule));
        assert_eq!(
            ctx.damage_on(fixtures::THEIR_UNIT),
            3,
            "the expired trap no longer doubles damage"
        );
    }
}
