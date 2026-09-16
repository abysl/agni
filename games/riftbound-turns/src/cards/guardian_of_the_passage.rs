use super::morbid_return::return_from_trash;
use super::prelude::{card_target, done, on_hold_me, target, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec, KIND_GEAR, KIND_UNIT};
use crate::engine::ctx::Ctx;

pub const UNIT_OR_GEAR_IN_YOUR_TRASH: Filter = Filter::And(&[
    Filter::Or(&[Filter::Kind(KIND_UNIT), Filter::Kind(KIND_GEAR)]),
    Filter::InTrash,
    Filter::Friendly,
]);

pub const PASSAGE: TargetSpec = target(
    UNIT_OR_GEAR_IN_YOUR_TRASH,
    0,
    1,
    TargetKind::Card,
    "a unit or gear in your trash to return to your hand",
);

fn usher(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(card) = card_target(ctx, item, 0) {
        return_from_trash(ctx, card);
    }
    done()
}

pub static CARD: Card = unit(
    "Guardian of the Passage",
    &[],
    &[on_hold_me(&[PASSAGE], usher)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cleanup, play as play_engine, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const GUARDIAN: u32 = 90;
    const BURIED_UNIT: u32 = 91;
    const BURIED_GEAR: u32 = 92;
    const BURIED_SPELL: u32 = 93;
    const THEIR_BURIED_UNIT: u32 = 94;

    fn guardian(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(6),
            domain: vec!["Calm".into()],
            ..fixtures::unit(GUARDIAN, zone, 0, "Guardian of the Passage", 6)
        }
    }

    fn crypt(zone: u16, holder: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(guardian(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(BURIED_UNIT, fixtures::TRASH, 0, "Jinx", 2));
        fixture.table.cards.push(fixtures::gear(
            BURIED_GEAR,
            fixtures::TRASH,
            0,
            "Trinket",
            1,
        ));
        fixture.table.cards.push(fixtures::spell(
            BURIED_SPELL,
            fixtures::TRASH,
            0,
            "Spark",
            2,
            1,
        ));
        fixture.table.cards.push(fixtures::unit(
            THEIR_BURIED_UNIT,
            fixtures::TRASH,
            1,
            "Jinx",
            2,
        ));
        fixture.blob.set_holder(fixtures::BF1, holder);
        fixture.resolve();
        fixture
    }

    fn hold(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
    }

    fn pending(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the hold asks for a card in the trash, not {other:?}"),
        }
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_hold_offers_one_unit_or_gear_card_from_its_trash() {
        assert!(std::ptr::eq(
            script_of("Guardian of the Passage").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Guardian of the Passage");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let hold = &CARD.abilities[0];
        assert_eq!(hold.trigger, Trigger::Hold(Who::Me));
        assert!(!hold.optional, "the may is the 0-of-1 target");
        assert!(hold.cost.is_none());
        assert!(hold.condition.is_none());
        assert_eq!(hold.targets.len(), 1);
        let spec = &hold.targets[0];
        assert_eq!(spec.filter, UNIT_OR_GEAR_IN_YOUR_TRASH);
        assert_eq!((spec.min, spec.max), (0, 1));
        assert_eq!(spec.kind, TargetKind::Card);
    }

    #[test]
    fn holding_with_him_offers_our_buried_unit_and_gear_and_the_pick_returns_to_hand() {
        let mut fixture = crypt(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        hold(&mut ctx);
        assert_eq!(ctx.points(0), 1, "the hold itself");
        let item = pending(&ctx);
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BURIED_UNIT}}}"),
                format!("{{card {BURIED_GEAR}}}"),
                "skip".to_string()
            ],
            "not the spell beside them, not the enemy's unit"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "{{card {GUARDIAN}}}: choose a unit or gear in your trash to return to your hand (0 of 1)"
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
            "a unit on the board is not in the trash"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BURIED_GEAR}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == GUARDIAN
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BURIED_GEAR)]);
        assert!(ctx.in_trash(BURIED_GEAR), "the return waits for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BURIED_GEAR).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.effects.contains(&Effect::Move {
            card: BURIED_GEAR,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BURIED_GEAR}}} returns from the trash to hand"
        )));
        assert!(ctx.in_trash(BURIED_UNIT), "only the pick returns");
        assert!(ctx.in_trash(BURIED_SPELL));
        assert!(ctx.in_trash(THEIR_BURIED_UNIT));
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_returns_nothing_and_an_empty_trash_resolves_without_a_prompt() {
        let mut fixture = crypt(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        hold(&mut ctx);
        pending(&ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.in_trash(BURIED_UNIT) && ctx.in_trash(BURIED_GEAR));
        drop(ctx);
        let mut bare = crypt(fixtures::BF1, Some(0));
        bare.table
            .cards
            .retain(|card| ![BURIED_UNIT, BURIED_GEAR].contains(&card.id));
        bare.resolve();
        let mut ctx = bare.ctx();
        hold(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing to offer, nothing asked");
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the trigger still goes on the chain"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_is_not_a_hold_and_a_hold_without_him_asks_nothing() {
        let mut fixture = crypt(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "469.2 · conquering is not holding"
        );
        assert!(ctx.blob.prompt.is_none());
        drop(ctx);
        let mut away = crypt(fixtures::BASE, Some(0));
        away.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        away.resolve();
        let mut ctx = away.ctx();
        hold(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "Vi holds it; he sits in the base"
        );
        assert!(ctx.in_trash(BURIED_UNIT) && ctx.in_trash(BURIED_GEAR));
    }
}
