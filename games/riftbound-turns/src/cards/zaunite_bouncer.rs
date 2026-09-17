use super::prelude::{a_card, bounce, card_target, done, play, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ANOTHER_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::AtBattlefield, Filter::NotSelf]);

const PATRON: TargetSpec = a_card(
    ANOTHER_UNIT_AT_A_BATTLEFIELD,
    "another unit at a battlefield to return to hand",
);

fn throw_out(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        bounce(ctx, unit);
    }
    done()
}

pub static CARD: Card = unit("Zaunite Bouncer", &[], &[play(&[PATRON], throw_out)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const BOUNCER: u32 = 90;
    const ALLY: u32 = 91;
    const CHAOS_RUNE: u32 = 46;

    fn bouncer(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(2),
            domain: vec!["Chaos".into()],
            ..fixtures::card(id, zone, seat, "Zaunite Bouncer", "Unit")
        }
    }

    fn bar() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(bouncer(BOUNCER, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Jinx", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn hire(ctx: &mut Ctx, at: Location) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, BOUNCER, Origin::Hand, Some(at))?;
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

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_another_unit_at_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Zaunite Bouncer").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Zaunite Bouncer");
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
        assert_eq!(spec.filter, ANOTHER_UNIT_AT_A_BATTLEFIELD);
    }

    #[test]
    fn the_bouncer_offers_units_at_battlefields_of_either_side_and_returns_the_pick_to_hand() {
        let mut fixture = bar();
        let action = fixtures::move_action(BOUNCER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let their_hand = ctx.hand_of(1).len();
        hire(&mut ctx, Location::Base(0)).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {ALLY}}}")
            ],
            "the enemy token at BF2 and the friendly unit at BF1; the base units stay out"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in its base is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BOUNCER
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(ALLY)]);
        assert!(ctx.on_board(ALLY), "the bounce waits for the trigger");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(ALLY).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.effects.contains(&Effect::Move {
            card: ALLY,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ALLY}}} returns to hand")));
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Died { .. })),
            "a bounce is not a death"
        );
        assert_eq!(ctx.hand_of(1).len(), their_hand);
        assert!(ctx.on_board(fixtures::SPRITE), "only the pick goes home");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_token_pick_ceases_to_exist_instead_of_reaching_a_hand() {
        let mut fixture = bar();
        let action = fixtures::move_action(BOUNCER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let their_hand = ctx.hand_of(1).len();
        hire(&mut ctx, Location::Base(0)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert!(ctx.effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert_eq!(
            ctx.hand_of(1).len(),
            their_hand,
            "a token never reaches a hand"
        );
        assert!(ctx.on_board(ALLY));
    }

    #[test]
    fn the_bouncer_played_to_a_battlefield_never_offers_itself() {
        let mut fixture = bar();
        let action = fixtures::move_action(BOUNCER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        hire(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(BOUNCER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let item = pending(&ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {ALLY}}}")
            ],
            "another unit: the bouncer at the same battlefield is not offered"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[BOUNCER]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "naming itself is refused"
        );
    }

    #[test]
    fn with_no_other_unit_at_a_battlefield_the_trigger_fizzles() {
        let mut fixture = bar();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, ALLY].contains(&card.id));
        fixture.resolve();
        let action = fixtures::move_action(BOUNCER, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        hire(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "390.3 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BOUNCER}}} trigger fizzles · no legal target"
        )));
        assert_eq!(
            ctx.location(BOUNCER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
    }
}
