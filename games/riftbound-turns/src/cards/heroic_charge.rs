use super::prelude::{
    a_card, a_friendly_unit, card_target, done, might_this_turn, play, spell, stun,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
const CHARGER: usize = 0;
const FOE: usize = 1;

pub const ENEMY_UNIT_AT_ITS_LOCATION: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::SameLocationAs(0)]);

fn charge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, CHARGER) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} might this turn"));
    }
    if let Some(foe) = card_target(ctx, item, FOE) {
        stun(ctx, foe);
    }
    done()
}

pub static CARD: Card = spell(
    "Heroic Charge",
    &[Keyword::Action],
    &[play(
        &[
            a_friendly_unit("a friendly unit"),
            a_card(ENEMY_UNIT_AT_ITS_LOCATION, "an enemy unit at its location"),
        ],
        charge,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const CHARGE: u32 = 90;
    const THEIR_CHARGE: u32 = 91;
    const ALLY: u32 = 92;
    const FOE_AT_BF1: u32 = 93;

    fn charge_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Heroic Charge", 3, 0);
        card.domain = vec!["Order".into()];
        card
    }

    fn field() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(charge_card(CHARGE, 0));
        fixture.table.cards.push(charge_card(THEIR_CHARGE, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Vanguard", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(FOE_AT_BF1, fixtures::BF1, 1, "Brute", 4));
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
    fn the_script_is_an_action_over_a_friendly_unit_and_an_enemy_unit_where_it_stands() {
        assert!(std::ptr::eq(script_of("Heroic Charge").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Hidden));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(
            ability.targets[0].filter,
            crate::cards::prelude::FRIENDLY_UNIT
        );
        assert_eq!(ability.targets[1].filter, ENEMY_UNIT_AT_ITS_LOCATION);
        assert_eq!((ability.targets[1].min, ability.targets[1].max), (1, 1));
        assert_eq!(MIGHT, 1);
    }

    #[test]
    fn the_friendly_unit_gets_one_might_and_the_enemy_beside_it_is_stunned() {
        let mut fixture = field();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHARGE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 92}", "cancel"],
            "your units only"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {FOE_AT_BF1}}}"), "cancel".to_string()],
            "the Sprite at the other battlefield and Jinx in their base are elsewhere"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {FOE_AT_BF1}}}")).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(ALLY), TargetRef::Card(FOE_AT_BF1)]
        );
        assert_eq!(ctx.current_might(ALLY), 2, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(ALLY), 3);
        assert!(ctx.is_stunned(FOE_AT_BF1));
        assert_eq!(ctx.combat_might(FOE_AT_BF1), 0);
        assert!(!ctx.is_stunned(ALLY));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ALLY}}} gets +1 might this turn")));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FOE_AT_BF1}}} is stunned")));
        assert_eq!(ctx.card(CHARGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_enemy_that_left_the_location_before_resolution_is_not_stunned_but_the_buff_lands() {
        let mut fixture = field();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHARGE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {FOE_AT_BF1}}}")).unwrap();
        ctx.actor = 1;
        ctx.move_unit(FOE_AT_BF1, Location::Base(1), MoveCause::Effect);
        ctx.actor = 0;
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(ALLY), 3);
        assert!(
            !ctx.is_stunned(FOE_AT_BF1),
            "356.3.e · it no longer stands at the charger's location"
        );
        assert_eq!(ctx.card(CHARGE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn it_waits_for_your_turn_and_refuses_a_far_enemy_or_a_friendly_second_pick() {
        let mut fixture = field();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CHARGE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, CHARGE).unwrap();
        for wrong in [fixtures::SPRITE, FOE_AT_BF1, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        for wrong in [fixtures::SPRITE, fixtures::THEIR_UNIT, fixtures::VI, ALLY] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit at the charger's location"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CHARGE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        let mut lonely = field();
        lonely.table.cards.retain(|card| card.id != FOE_AT_BF1);
        lonely.resolve();
        let mut ctx = lonely.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHARGE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "no enemy beside the Vanguard: only the way out"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CHARGE).unwrap().zone, Some(fixtures::HAND));
    }
}
