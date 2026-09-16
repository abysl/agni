use super::prelude::{a_card, card_target, done, play, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetSpec, KIND_GEAR};
use crate::engine::ctx::Ctx;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const FRIENDLY_GEAR_IN_TRASH: Filter =
    Filter::And(&[Filter::Kind(KIND_GEAR), Filter::InTrash, Filter::Friendly]);

pub const SCRAP: TargetSpec = a_card(
    FRIENDLY_GEAR_IN_TRASH,
    "a gear in your trash to return to your hand",
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

fn salvage(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(gear) = card_target(ctx, item, 0) {
        return_from_trash(ctx, gear);
    }
    done()
}

pub static CARD: Card = unit("Aspiring Engineer", &[], &[play(&[SCRAP], salvage)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ENGINEER: u32 = 90;
    const SCRAPPED_GEAR: u32 = 91;
    const BURIED_UNIT: u32 = 92;
    const THEIR_SCRAPPED_GEAR: u32 = 93;
    const SECOND_SCRAPPED_GEAR: u32 = 94;
    const BOARD_GEAR: u32 = 95;
    const MIND_RUNE: u32 = 46;

    fn engineer(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(ENGINEER, zone, 0, "Aspiring Engineer", 3)
        }
    }

    fn workshop() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(engineer(fixtures::HAND));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::gear(SCRAPPED_GEAR, fixtures::TRASH, 0, "Loot", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(BURIED_UNIT, fixtures::TRASH, 0, "Jinx", 2));
        fixture.table.cards.push(fixtures::gear(
            THEIR_SCRAPPED_GEAR,
            fixtures::TRASH,
            1,
            "Trinket",
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::gear(BOARD_GEAR, fixtures::BASE, 0, "Anvil", 1));
        fixture.resolve();
        fixture
    }

    fn two_deep() -> Fixture {
        let mut fixture = workshop();
        fixture.table.cards.push(fixtures::gear(
            SECOND_SCRAPPED_GEAR,
            fixtures::TRASH,
            0,
            "Gadget",
            3,
        ));
        fixture.resolve();
        fixture
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
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_a_gear_card_in_its_controllers_trash()
    {
        assert!(std::ptr::eq(script_of("Aspiring Engineer").unwrap(), &CARD));
        assert_eq!(CARD.name, "Aspiring Engineer");
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
        assert_eq!(ability.targets, &[SCRAP]);
        assert_eq!((SCRAP.min, SCRAP.max), (1, 1));
        assert_eq!(SCRAP.kind, TargetKind::Card);
        assert_eq!(SCRAP.filter, FRIENDLY_GEAR_IN_TRASH);
    }

    #[test]
    fn she_offers_only_our_gear_cards_in_the_trash_and_returns_the_pick_to_hand() {
        let mut fixture = two_deep();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ENGINEER).unwrap();
        assert_eq!(ctx.location(ENGINEER), Some(Location::Base(0)));
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {SCRAPPED_GEAR}}}"),
                format!("{{card {SECOND_SCRAPPED_GEAR}}}")
            ],
            "not the unit beside them, not the enemy's gear, not a gear on the board"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "{{card {ENGINEER}}}: choose a gear in your trash to return to your hand (0 of 1)"
            )
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[BURIED_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in the trash is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[THEIR_SCRAPPED_GEAR]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the enemy's trash is not ours"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[BOARD_GEAR]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a gear on the board is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCRAPPED_GEAR}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ENGINEER
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(SCRAPPED_GEAR)]);
        assert!(
            ctx.in_trash(SCRAPPED_GEAR),
            "the return waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(SCRAPPED_GEAR));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.effects.contains(&Effect::Move {
            card: SCRAPPED_GEAR,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} returns {{card {SCRAPPED_GEAR}}} from the trash to their hand"
        )));
        assert!(ctx.in_trash(SECOND_SCRAPPED_GEAR), "only the pick returns");
        assert!(ctx.in_trash(BURIED_UNIT));
        assert!(ctx.in_trash(THEIR_SCRAPPED_GEAR));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lone_gear_card_in_the_trash_is_chosen_without_a_click() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ENGINEER).unwrap();
        assert!(ctx.blob.prompt.is_none(), "one candidate answers itself");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(SCRAPPED_GEAR)]);
        let hand = ctx.hand_of(0).len();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(SCRAPPED_GEAR));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
    }

    #[test]
    fn a_pick_that_left_the_trash_before_resolution_is_not_returned() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ENGINEER).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(SCRAPPED_GEAR)]);
        ctx.banish(SCRAPPED_GEAR);
        let hand = ctx.hand_of(0).len();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(SCRAPPED_GEAR).unwrap().zone,
            Some(fixtures::BANISHMENT),
            "356.3.e · a banished card no longer matches the spec"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn with_no_gear_card_in_our_trash_the_trigger_fizzles_and_she_still_lands() {
        let mut fixture = workshop();
        fixture.table.cards.retain(|card| card.id != SCRAPPED_GEAR);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, ENGINEER).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "402.4 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {ENGINEER}}} trigger fizzles · no legal target"
        )));
        assert_eq!(ctx.location(ENGINEER), Some(Location::Base(0)));
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.in_trash(BURIED_UNIT));
        assert!(ctx.in_trash(THEIR_SCRAPPED_GEAR));
    }
}
