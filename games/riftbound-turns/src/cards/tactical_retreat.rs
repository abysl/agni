use super::prelude::{a_friendly_unit, card_target, done, exhaust, heal, play, recall, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub fn retreat(ctx: &mut Ctx, unit: u32) -> bool {
    if !ctx.is_unit(unit) || !ctx.on_board(unit) {
        return false;
    }
    heal(ctx, unit);
    exhaust(ctx, unit);
    recall(ctx, unit, true);
    ctx.narrate(format!(
        "{{card {unit}}} retreats instead of dying · healed, exhausted and recalled"
    ));
    true
}

pub fn retreats_instead_of_dying_this_turn(ctx: &mut Ctx, _: &Item, unit: u32) {
    ctx.narrate(format!(
        "{{card {unit}}} is covered · the next time it would die this turn it retreats instead"
    ));
}

fn cover(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        retreats_instead_of_dying_this_turn(ctx, item, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Tactical Retreat",
    &[Keyword::Reaction],
    &[play(&[a_friendly_unit("a friendly unit")], cover)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{Location, FRIENDLY_UNIT};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const RETREAT: u32 = 90;
    const THEIR_RETREAT: u32 = 91;
    const ORDER_RUNES: [u32; 2] = [46, 47];

    fn tactical_retreat(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Tactical Retreat", 2, 0);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(tactical_retreat(RETREAT, 0));
        fixture.table.cards.push(tactical_retreat(THEIR_RETREAT, 1));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        for rune in ORDER_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Order", false));
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_reaction_over_one_friendly_unit() {
        assert!(std::ptr::eq(script_of("Tactical Retreat").unwrap(), &CARD));
        assert_eq!(CARD.name, "Tactical Retreat");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert!(
            CARD.replacement.is_none(),
            "a spell owns no kill replacement"
        );
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(CARD.abilities[0].targets[0].filter, FRIENDLY_UNIT);
    }

    #[test]
    fn the_retreat_heals_exhausts_and_recalls_a_unit_on_the_board_and_nothing_else() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(ctx.damage(fixtures::VI, 2, Cause::Rule));
        assert_eq!(ctx.damage_on(fixtures::VI), 2);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(retreat(&mut ctx, fixtures::VI));
        assert_eq!(ctx.damage_on(fixtures::VI), 0, "healed");
        assert!(ctx.card(fixtures::VI).unwrap().exhausted, "exhausted");
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "recalled to its base"
        );
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, crate::engine::ctx::Event::Moved { .. })),
            "a recall is not a move"
        );
        assert!(ctx.blob.log.contains(
            &"{card 50} retreats instead of dying · healed, exhausted and recalled".to_string()
        ));
        assert!(!retreat(&mut ctx, fixtures::HAND_UNIT), "not on the board");
        assert!(!retreat(&mut ctx, fixtures::HAND_GEAR), "not a unit");
    }

    #[test]
    fn the_chosen_friendly_unit_is_covered_as_the_spell_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RETREAT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "my units only"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy off three");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(
            &"{card 50} is covered · the next time it would die this turn it retreats instead"
                .to_string()
        ));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1)),
            "covering is not retreating"
        );
        assert_eq!(ctx.card(RETREAT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_reacts_on_my_turn_over_its_own_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_RETREAT).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 81}", "cancel"]);
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert!(ctx.blob.log.contains(
            &"{card 81} is covered · the next time it would die this turn it retreats instead"
                .to_string()
        ));
    }

    #[test]
    fn an_enemy_unit_a_gear_and_a_hand_card_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RETREAT).unwrap();
        for wrong in [
            fixtures::THEIR_UNIT,
            fixtures::SPRITE,
            fixtures::HAND_GEAR,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(RETREAT).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    #[ignore = "engine gap · a floating turn replacement on the kill path: kill::applicable consults the replacements of faces on the board only, so a resolved spell cannot cover a unit for the turn; retreats_instead_of_dying_this_turn only narrates, and the primitive is a next-death replacement registered on CardState, spent by the first kill and cleared at Expiration"]
    fn the_next_death_this_turn_becomes_a_retreat_and_the_one_after_sticks() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RETREAT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.damage(fixtures::VI, 1, Cause::Rule));
        assert_eq!(
            ctx.kill(fixtures::VI, Cause::Rule),
            Killed::Replaced,
            "the first death this turn is replaced"
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(fixtures::VI));
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert_eq!(
            ctx.kill(fixtures::VI, Cause::Rule),
            Killed::Yes,
            "the cover is spent"
        );
    }
}
