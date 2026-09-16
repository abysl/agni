use super::prelude::{a_card, card_target, done, play, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetSpec, KIND_UNIT};
use crate::engine::ctx::Ctx;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const FRIENDLY_UNIT_CARD_IN_TRASH: Filter =
    Filter::And(&[Filter::Kind(KIND_UNIT), Filter::InTrash, Filter::Friendly]);

const BURIED: TargetSpec = a_card(
    FRIENDLY_UNIT_CARD_IN_TRASH,
    "a unit in your trash to return to your hand",
);

fn return_from_trash(ctx: &mut Ctx, card: u32) -> bool {
    if !ctx.in_trash(card) {
        return false;
    }
    let Some(hand) = ctx.zones.hand else {
        return false;
    };
    let owner = ctx.owner(card);
    ctx.emit(Effect::Move {
        card,
        zone: hand,
        seat: owner,
        index: TOP,
    });
    ctx.narrate(format!(
        "{{seat {owner}}} returns {{card {card}}} from the trash to their hand"
    ));
    true
}

fn exhume(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(card) = card_target(ctx, item, 0) {
        return_from_trash(ctx, card);
    }
    done()
}

pub static CARD: Card = unit("Cemetery Attendant", &[], &[play(&[BURIED], exhume)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ATTENDANT: u32 = 90;
    const BURIED_UNIT: u32 = 91;
    const BURIED_SPELL: u32 = 92;
    const THEIR_BURIED_UNIT: u32 = 93;
    const BURIED_DOG: u32 = 94;
    const CHAOS_RUNE: u32 = 46;

    fn attendant(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            might: Some(3),
            domain: vec!["Chaos".into()],
            ..fixtures::card(id, zone, seat, "Cemetery Attendant", "Unit")
        }
    }

    fn cemetery() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(attendant(ATTENDANT, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(BURIED_UNIT, fixtures::TRASH, 0, "Jinx", 2));
        fixture.table.cards.push(fixtures::spell(
            BURIED_SPELL,
            fixtures::TRASH,
            0,
            "Spark",
            1,
            0,
        ));
        fixture.table.cards.push(fixtures::unit(
            THEIR_BURIED_UNIT,
            fixtures::TRASH,
            1,
            "Brute",
            4,
        ));
        fixture.resolve();
        fixture
    }

    fn two_deep() -> Fixture {
        let mut fixture = cemetery();
        fixture
            .table
            .cards
            .push(fixtures::unit(BURIED_DOG, fixtures::TRASH, 0, "Dog", 1));
        fixture.resolve();
        fixture
    }

    fn hire(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, ATTENDANT, Origin::Hand, Some(Location::Base(0)))?;
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
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_a_unit_card_in_its_controllers_trash()
    {
        assert!(std::ptr::eq(
            crate::cards::script_of("Cemetery Attendant").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Cemetery Attendant");
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
        assert_eq!(spec.filter, FRIENDLY_UNIT_CARD_IN_TRASH);
    }

    #[test]
    fn the_attendant_offers_only_our_unit_cards_in_the_trash_and_returns_the_pick_to_hand() {
        let mut fixture = two_deep();
        let action = fixtures::move_action(ATTENDANT, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        hire(&mut ctx).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BURIED_UNIT}}}"),
                format!("{{card {BURIED_DOG}}}")
            ],
            "not the spell beside them, not the enemy's unit, not a unit on the board"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "{{card {ATTENDANT}}}: choose a unit in your trash to return to your hand (0 of 1)"
            )
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[BURIED_SPELL]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a spell in the trash is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[THEIR_BURIED_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the enemy's trash is not ours"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit on the board is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BURIED_UNIT}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ATTENDANT
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BURIED_UNIT)]);
        assert!(
            ctx.in_trash(BURIED_UNIT),
            "the return waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BURIED_UNIT).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.in_hand(BURIED_UNIT));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.effects.contains(&Effect::Move {
            card: BURIED_UNIT,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} returns {{card {BURIED_UNIT}}} from the trash to their hand"
        )));
        assert!(ctx.in_trash(BURIED_SPELL), "the spell stays buried");
        assert!(ctx.in_trash(BURIED_DOG), "only the pick returns");
        assert!(ctx.in_trash(THEIR_BURIED_UNIT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lone_unit_card_in_the_trash_is_chosen_without_a_click() {
        let mut fixture = cemetery();
        let action = fixtures::move_action(ATTENDANT, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        hire(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "one candidate answers itself");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BURIED_UNIT)]);
        let hand = ctx.hand_of(0).len();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(BURIED_UNIT));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
    }

    #[test]
    fn a_pick_that_left_the_trash_before_resolution_is_not_returned() {
        let mut fixture = cemetery();
        let action = fixtures::move_action(ATTENDANT, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        hire(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BURIED_UNIT)]);
        ctx.banish(BURIED_UNIT);
        assert!(!ctx.in_trash(BURIED_UNIT));
        let hand = ctx.hand_of(0).len();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(BURIED_UNIT).unwrap().zone,
            Some(fixtures::BANISHMENT),
            "356.3.e · a banished card no longer matches the spec"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(!ctx.effects.contains(&Effect::Move {
            card: BURIED_UNIT,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
    }

    #[test]
    fn with_no_unit_card_in_our_trash_the_trigger_fizzles_and_the_attendant_still_lands() {
        let mut fixture = cemetery();
        fixture.table.cards.retain(|card| card.id != BURIED_UNIT);
        fixture.resolve();
        let action = fixtures::move_action(ATTENDANT, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let hand = ctx.hand_of(0).len();
        hire(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "390.3 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {ATTENDANT}}} trigger fizzles · no legal target"
        )));
        assert_eq!(ctx.location(ATTENDANT), Some(Location::Base(0)));
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.in_trash(BURIED_SPELL));
        assert!(ctx.in_trash(THEIR_BURIED_UNIT));
    }
}
