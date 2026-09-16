use super::prelude::{bounce, card_target, done, optional, play, target, unit};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const FRIENDLY_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::AtBattlefield]);

pub const PATIENT: TargetSpec = target(
    FRIENDLY_UNIT_AT_A_BATTLEFIELD,
    0,
    1,
    TargetKind::Card,
    "a friendly unit at a battlefield to return to its owner's hand",
);

fn treat(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let owner = ctx.owner(unit);
    if bounce(ctx, unit) {
        ctx.narrate(format!(
            "{{card {}}} returns {{card {unit}}} to {{seat {owner}}}'s hand",
            item.kind.source()
        ));
    }
    done()
}

pub static CARD: Card = unit(
    "Grim Apothecary",
    &[Keyword::Ambush],
    &[optional(play(&[PATIENT], treat))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const APOTHECARY: u32 = 90;
    const ALLY: u32 = 91;
    const THEIR_BRUTE: u32 = 92;

    fn apothecary(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(0),
            domain: vec!["Fury".into()],
            ..fixtures::unit(APOTHECARY, zone, seat, "Grim Apothecary", 3)
        }
    }

    fn clinic() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(apothecary(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn enter(ctx: &mut Ctx, to: Location) -> u16 {
        play_engine::begin(ctx, 0, APOTHECARY, Origin::Hand, Some(to)).unwrap();
        settle(ctx).unwrap();
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play asks for a friendly unit, not {other:?}"),
        }
    }

    #[test]
    fn the_script_prints_ambush_and_an_optional_play_trigger_with_a_zero_of_one_target() {
        assert!(std::ptr::eq(script_of("Grim Apothecary").unwrap(), &CARD));
        assert_eq!(CARD.name, "Grim Apothecary");
        assert_eq!(CARD.keywords, [Keyword::Ambush]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.optional, "you may");
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets, [PATIENT]);
        assert_eq!((PATIENT.min, PATIENT.max), (0, 1));
        assert_eq!(PATIENT.kind, TargetKind::Card);
        assert_eq!(PATIENT.filter, FRIENDLY_UNIT_AT_A_BATTLEFIELD);
    }

    #[test]
    fn played_to_a_battlefield_he_offers_friendly_units_at_battlefields_himself_included_and_bounces_the_pick(
    ) {
        let mut fixture = clinic();
        let mut ctx = fixture.ctx();
        let item = enter(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {ALLY}}}"),
                format!("{{card {APOTHECARY}}}"),
                "skip".to_string()
            ],
            "Vi is in base, the Brute is an enemy, he stands where he was played"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {APOTHECARY}}}: choose a friendly unit at a battlefield to return to its owner's hand (0 of 1)")
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[THEIR_BRUTE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit in base is refused"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == APOTHECARY
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(ALLY)]);
        assert!(ctx.on_board(ALLY), "the bounce waits for the trigger");
        let hand = ctx.hand_of(0).len();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(ALLY).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.effects.contains(&Effect::Move {
            card: ALLY,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.on_board(APOTHECARY));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {APOTHECARY}}} returns {{card {ALLY}}} to {{seat 0}}'s hand"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn he_can_return_himself_and_skipping_returns_nobody() {
        let mut fixture = clinic();
        let mut ctx = fixture.ctx();
        enter(&mut ctx, Location::Battlefield(fixtures::BF1));
        fixtures::choose(&mut ctx, 0, &format!("{{card {APOTHECARY}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(APOTHECARY).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.on_board(ALLY));
        drop(ctx);
        let mut fixture = clinic();
        let mut ctx = fixture.ctx();
        enter(&mut ctx, Location::Battlefield(fixtures::BF1));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(ALLY));
        assert!(ctx.on_board(APOTHECARY));
    }

    #[test]
    fn played_to_base_with_no_friendly_unit_at_a_battlefield_the_trigger_has_nothing_to_offer() {
        let mut fixture = clinic();
        fixture.table.cards.retain(|card| card.id != ALLY);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            APOTHECARY,
            Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no candidate, no prompt");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.on_board(APOTHECARY));
    }

    #[test]
    fn a_pick_that_left_the_battlefield_before_resolution_stays_where_it_is() {
        let mut fixture = clinic();
        let mut ctx = fixture.ctx();
        enter(&mut ctx, Location::Battlefield(fixtures::BF1));
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        ctx.table.card_mut(ALLY).unwrap().zone = Some(fixtures::BASE);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(ALLY).unwrap().zone,
            Some(fixtures::BASE),
            "356.3.e · a unit back in base no longer matches the spec"
        );
    }
}
