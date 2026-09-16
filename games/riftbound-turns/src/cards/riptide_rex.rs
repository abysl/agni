use super::prelude::{a_card, card_target, deal, done, play, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 6;

pub const ENEMY_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::AtBattlefield]);

const VICTIM: TargetSpec = a_card(
    ENEMY_UNIT_AT_A_BATTLEFIELD,
    "an enemy unit at a battlefield",
);

fn riptide(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        deal(ctx, item, unit, DAMAGE);
    }
    done()
}

pub static CARD: Card = unit("Riptide Rex", &[], &[play(&[VICTIM], riptide)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::{Cause, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const REX: u32 = 90;
    const THEIR_BRUTE: u32 = 91;
    const MIND_RUNE: u32 = 46;

    fn rex(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(6),
            domain: vec!["Mind".into()],
            ..fixtures::card(id, zone, seat, "Riptide Rex", "Unit")
        }
    }

    fn harbour() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rex(REX, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 7));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn sail(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, REX, Origin::Hand, Some(Location::Base(0)))?;
        settle(ctx)
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
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_one_enemy_unit_at_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Riptide Rex").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Riptide Rex");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets.len(), 1);
        let spec = &ability.targets[0];
        assert_eq!((spec.min, spec.max), (1, 1));
        assert_eq!(spec.kind, TargetKind::Card);
        assert_eq!(spec.filter, ENEMY_UNIT_AT_A_BATTLEFIELD);
        assert_eq!(DAMAGE, 6);
    }

    #[test]
    fn playing_rex_offers_only_enemy_units_at_battlefields_and_deals_six_to_the_pick() {
        let mut fixture = harbour();
        let action = fixtures::move_action(REX, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        sail(&mut ctx).unwrap();
        assert_eq!(ctx.location(REX), Some(Location::Base(0)));
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {THEIR_BRUTE}}}")
            ],
            "Vi is friendly and Jinx is in base"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {REX}}}: choose an enemy unit at a battlefield (0 of 1)")
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit in its base is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == REX
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_BRUTE)]);
        assert_eq!(
            ctx.damage_on(THEIR_BRUTE),
            0,
            "the damage waits for the trigger"
        );
        let chain_item = ctx.blob.chain[0].id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 6);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: THEIR_BRUTE,
            n: DAMAGE,
            source: Cause::Item(chain_item)
        }));
        assert!(ctx.on_board(THEIR_BRUTE), "7 Might survives 6");
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn six_damage_kills_a_smaller_unit_and_a_token_target_vanishes() {
        let mut fixture = harbour();
        let action = fixtures::move_action(REX, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        sail(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "3 Might dies to 6 and the token ceases to exist"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == fixtures::SPRITE
        )));
        assert!(ctx.on_board(THEIR_BRUTE));
    }

    #[test]
    fn with_no_enemy_unit_at_a_battlefield_the_trigger_fizzles_and_rex_still_lands() {
        let mut fixture = harbour();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, THEIR_BRUTE].contains(&card.id));
        fixture.resolve();
        let action = fixtures::move_action(REX, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        sail(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to ask");
        assert!(ctx.blob.chain.is_empty(), "390.3 · the trigger is removed");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {REX}}} trigger fizzles · no legal target")));
        assert_eq!(ctx.location(REX), Some(Location::Base(0)));
        assert_eq!(
            ctx.damage_on(fixtures::THEIR_UNIT),
            0,
            "the unit in base is never hit"
        );
    }

    #[test]
    fn a_target_that_left_the_battlefield_before_resolution_takes_nothing() {
        let mut fixture = harbour();
        let action = fixtures::move_action(REX, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        sail(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        ctx.table.card_mut(THEIR_BRUTE).unwrap().zone = Some(fixtures::BASE);
        ctx.table.card_mut(THEIR_BRUTE).unwrap().seat = 1;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.damage_on(THEIR_BRUTE),
            0,
            "356.3.e · a unit back in base no longer matches the spec"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
    }
}
