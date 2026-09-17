use super::prelude::{a_card, card_target, deal, done, play, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ENEMY_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::AtBattlefield]);

const PREY: TargetSpec = a_card(
    ENEMY_UNIT_AT_A_BATTLEFIELD,
    "an enemy unit at a battlefield",
);

fn snap(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    let Some(prey) = card_target(ctx, item, 0) else {
        return done();
    };
    let to_prey = u8::try_from(ctx.current_might(me).max(0)).unwrap_or(u8::MAX);
    let to_me = u8::try_from(ctx.current_might(prey).max(0)).unwrap_or(u8::MAX);
    deal(ctx, item, prey, to_prey);
    deal(ctx, item, me, to_me);
    done()
}

pub static CARD: Card = unit("Carnivorous Snapvine", &[], &[play(&[PREY], snap)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::{Cause, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SNAPVINE: u32 = 90;
    const THEIR_BRUTE: u32 = 91;
    const BODY_RUNE: u32 = 46;

    fn snapvine(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(6),
            domain: vec!["Body".into()],
            ..fixtures::card(id, zone, seat, "Carnivorous Snapvine", "Unit")
        }
    }

    fn thicket() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(snapvine(SNAPVINE, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 8));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn plant(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, SNAPVINE, Origin::Hand, Some(Location::Base(0)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
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

    fn dealt(ctx: &Ctx, card: u32) -> Vec<u8> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt { card: hit, n, .. } if *hit == card => Some(*n),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_one_enemy_unit_at_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Carnivorous Snapvine").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Carnivorous Snapvine");
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
    }

    #[test]
    fn the_snapvine_and_its_prey_deal_their_mights_to_each_other_when_the_trigger_resolves() {
        let mut fixture = thicket();
        let action = fixtures::move_action(SNAPVINE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        plant(&mut ctx).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {THEIR_BRUTE}}}")
            ],
            "enemy units at battlefields; Jinx in her base is out"
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
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SNAPVINE
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_BRUTE)]);
        assert_eq!(
            ctx.damage_on(THEIR_BRUTE),
            0,
            "the bite waits for the trigger"
        );
        let chain_item = ctx.blob.chain[0].id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 6, "the snapvine's Might");
        assert_eq!(dealt(&ctx, SNAPVINE), [8], "the brute's Might");
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: THEIR_BRUTE,
            n: 6,
            source: Cause::Item(chain_item)
        }));
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: SNAPVINE,
            n: 8,
            source: Cause::Item(chain_item)
        }));
        assert!(ctx.on_board(THEIR_BRUTE), "8 Might survives 6");
        assert!(!ctx.on_board(SNAPVINE), "6 Might dies to 8");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: true, .. } if *card == SNAPVINE
        )));
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_this_turn_might_bonus_is_read_at_resolution_and_a_small_prey_dies_to_the_bite() {
        let mut fixture = thicket();
        let action = fixtures::move_action(SNAPVINE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        plant(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        let until = crate::state::Expiry::EndOfTurn(ctx.turn());
        ctx.might(SNAPVINE, 1, until, None, 0);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::SPRITE).is_none(), "3 Might dies to 7");
        assert_eq!(dealt(&ctx, fixtures::SPRITE), [7]);
        assert_eq!(dealt(&ctx, SNAPVINE), [3], "the Sprite bites back for 3");
        assert_eq!(ctx.damage_on(SNAPVINE), 3);
        assert!(ctx.on_board(SNAPVINE));
    }

    #[test]
    fn a_snapvine_off_the_board_bites_nobody_and_takes_nothing() {
        let mut fixture = thicket();
        let action = fixtures::move_action(SNAPVINE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        plant(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        ctx.kill(SNAPVINE, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(SNAPVINE));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 0);
        assert!(dealt(&ctx, THEIR_BRUTE).is_empty());
        assert!(dealt(&ctx, SNAPVINE).is_empty());
    }

    #[test]
    fn with_no_enemy_unit_at_a_battlefield_the_trigger_fizzles() {
        let mut fixture = thicket();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, THEIR_BRUTE].contains(&card.id));
        fixture.resolve();
        let action = fixtures::move_action(SNAPVINE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        plant(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "390.3 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SNAPVINE}}} trigger fizzles · no legal target"
        )));
        assert_eq!(ctx.location(SNAPVINE), Some(Location::Base(0)));
        assert_eq!(ctx.damage_on(SNAPVINE), 0);
    }
}
