use super::prelude::{
    a_card, an_enemy_unit, bounce, card_target, done, play, unit, ANOTHER_FRIENDLY_UNIT_THAN_ME,
};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

const FRIENDLY: usize = 0;
const ENEMY: usize = 1;

pub const ANOTHER_FRIENDLY: TargetSpec = a_card(
    ANOTHER_FRIENDLY_UNIT_THAN_ME,
    "another friendly unit to return",
);
pub const AN_ENEMY: TargetSpec = an_enemy_unit("an enemy unit to return");

fn surface(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for index in [FRIENDLY, ENEMY] {
        if let Some(unit) = card_target(ctx, item, index) {
            bounce(ctx, unit);
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Beast Below",
    &[],
    &[play(&[ANOTHER_FRIENDLY, AN_ENEMY], surface)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::ENEMY_UNIT;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BEAST: u32 = 90;
    const DECKHAND: u32 = 91;
    const CHAOS_RUNES: [u32; 5] = [100, 101, 102, 103, 104];

    fn beast(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(7),
            power: Some(2),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(BEAST, zone, 0, "Beast Below", 8)
        }
    }

    fn depths() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(beast(fixtures::HAND));
        for rune in CHAOS_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.resolve();
        fixture
    }

    fn crewed() -> Fixture {
        let mut fixture = depths();
        fixture
            .table
            .cards
            .push(fixtures::unit(DECKHAND, fixtures::BF1, 0, "Deckhand", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn land(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, BEAST).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })) {
            fixtures::choose(ctx, 0, "your base").unwrap();
        }
    }

    fn pending(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the play trigger is pending")
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_another_friendly_and_an_enemy_unit() {
        assert!(std::ptr::eq(script_of("Beast Below").unwrap(), &CARD));
        assert_eq!(CARD.name, "Beast Below");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets, &[ANOTHER_FRIENDLY, AN_ENEMY]);
        for spec in ability.targets {
            assert_eq!((spec.min, spec.max), (1, 1));
            assert_eq!(spec.kind, TargetKind::Card);
        }
        assert_eq!(ANOTHER_FRIENDLY.filter, ANOTHER_FRIENDLY_UNIT_THAN_ME);
        assert_eq!(AN_ENEMY.filter, ENEMY_UNIT);
    }

    #[test]
    fn it_asks_for_another_friendly_unit_then_an_enemy_and_both_go_home_when_it_resolves() {
        let mut fixture = crewed();
        let mut ctx = fixture.ctx();
        land(&mut ctx);
        assert_eq!(ctx.location(BEAST), Some(Location::Base(0)));
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {DECKHAND}}}")
            ],
            "friendly units other than the beast"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[BEAST]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "another · not itself"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the first pick is friendly"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {DECKHAND}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT)
            ],
            "enemy units wherever they stand"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 1, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the second pick is an enemy"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 1, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "neither target is optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BEAST
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(DECKHAND),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
        assert!(ctx.on_board(DECKHAND), "the return waits for the trigger");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(DECKHAND));
        assert_eq!(ctx.card(DECKHAND).unwrap().seat, 0);
        assert!(ctx.in_hand(fixtures::THEIR_UNIT));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().seat,
            1,
            "each to its owner's hand"
        );
        assert!(ctx.on_board(fixtures::VI), "the unpicked friendly stays");
        assert!(ctx.on_board(fixtures::SPRITE), "the unpicked enemy stays");
        assert_eq!(ctx.location(BEAST), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lone_other_friendly_answers_itself_and_a_bounced_enemy_token_ceases_to_exist() {
        let mut fixture = depths();
        let mut ctx = fixture.ctx();
        land(&mut ctx);
        let item = pending(&ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item, spec: 1 }),
            "Vi is the only other friendly unit"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(fixtures::VI));
        assert!(ctx.card(fixtures::SPRITE).is_none(), "a token vanishes");
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
    }

    #[test]
    fn with_no_other_friendly_unit_the_whole_trigger_is_removed_even_with_enemies_about() {
        let mut fixture = depths();
        fixture.table.cards.retain(|card| card.id != fixtures::VI);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        land(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob.chain.is_empty(),
            "402.4 · not enough options for every choice"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BEAST}}} trigger fizzles · no legal target"
        )));
        assert_eq!(ctx.location(BEAST), Some(Location::Base(0)));
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.on_board(fixtures::SPRITE));
    }

    #[test]
    fn a_pick_gone_before_resolution_is_skipped_and_the_other_still_returns() {
        let mut fixture = crewed();
        let mut ctx = fixture.ctx();
        land(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {DECKHAND}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        ctx.kill(DECKHAND, crate::engine::ctx::Cause::Rule);
        assert!(ctx.in_trash(DECKHAND));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_trash(DECKHAND), "the dead unit is not fished out");
        assert!(ctx.in_hand(fixtures::THEIR_UNIT));
    }
}
