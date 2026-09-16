use super::maddened_marauder::UNIT_AT_A_BATTLEFIELD_MOVABLE_TO_BASE;
use super::prelude::{battlefield, card_target, done, move_unit, target, triggered, Location};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const A_UNIT_AT_A_BATTLEFIELD: TargetSpec = target(
    UNIT_AT_A_BATTLEFIELD_MOVABLE_TO_BASE,
    0,
    1,
    TargetKind::Card,
    "a unit at a battlefield to move to its base",
);

fn may_send_a_unit_home(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        ctx.narrate(format!("{{card {}}} moves no unit", item.kind.source()));
        return done();
    };
    let home = Location::Base(ctx.controller(unit));
    move_unit(ctx, item, unit, home);
    done()
}

pub static CARD: Card = battlefield(
    "Amateur Recital",
    &[],
    &[triggered(
        Trigger::Hold(Who::You),
        &[A_UNIT_AT_A_BATTLEFIELD],
        may_send_a_unit_home,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::equipment;
    use crate::cards::prelude::attach_gear;
    use crate::cards::{script_of, Filter};
    use crate::engine::cleanup;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const RECITAL: u32 = fixtures::GROUNDS;
    const HOMEBODY: u32 = 90;

    fn recital_held_by_me() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(RECITAL).unwrap().name = "Amateur Recital".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(HOMEBODY, fixtures::BASE, 1, "Homebody", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RECITAL).unwrap(),
            &CARD
        ));
        fixture
    }

    fn recital_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == RECITAL => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn hold(ctx: &mut Ctx, seat: u8) {
        assert!(cleanup::score_holds(ctx, seat).contains(&fixtures::BF1));
        settle(ctx).unwrap();
    }

    fn card_label(card: u32) -> String {
        format!("{{card {card}}}")
    }

    #[test]
    fn the_recital_is_one_hold_trigger_with_an_optional_target_at_any_battlefield() {
        assert!(std::ptr::eq(script_of("Amateur Recital").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert_eq!(ability.targets, &[A_UNIT_AT_A_BATTLEFIELD]);
        assert_eq!(
            A_UNIT_AT_A_BATTLEFIELD.filter,
            Filter::And(&[Filter::Unit, Filter::AtBattlefield, Filter::MovableToBase])
        );
        assert_eq!(
            (A_UNIT_AT_A_BATTLEFIELD.min, A_UNIT_AT_A_BATTLEFIELD.max),
            (0, 1)
        );
        assert!(ability.condition.is_none());
        assert!(ability.cost.is_none());
        assert!(!ability.optional, "the may is the zero-minimum target");
    }

    #[test]
    fn holding_offers_every_unit_at_a_battlefield_of_either_side_and_sends_the_pick_home() {
        let mut fixture = recital_held_by_me();
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        assert_eq!(recital_items(&ctx), [0]);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                card_label(fixtures::VI),
                card_label(fixtures::SPRITE),
                "skip".to_string()
            ],
            "Vi here, the Sprite at the other battlefield, the Homebody in base is not offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "{{card {RECITAL}}}: choose a unit at a battlefield to move to its base (0 of 1)"
            )
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0);
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 1
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::SPRITE)).unwrap();
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2)),
            "not before the trigger resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(fixtures::SPRITE), Some(Location::Base(1)));
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::SPRITE,
            zone: fixtures::BASE,
            seat: 1,
            index: TOP
        }));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, to: Location::Base(1), .. } if *card == fixtures::SPRITE
        )));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.points(0), 1, "the hold scored");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_holder_may_send_their_own_last_unit_home_and_the_emptied_battlefield_settles_unheld() {
        let mut fixture = recital_held_by_me();
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        assert_eq!(ctx.points(0), 1, "the hold scored before the move");
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::VI)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} moves to their base", fixtures::VI)));
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "cleanup settles a battlefield nobody stands on as unheld"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_moves_nothing_and_with_the_sprite_gone_only_vi_is_offered() {
        let mut fixture = recital_held_by_me();
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {RECITAL}}} moves no unit")));
        drop(ctx);
        let mut alone = recital_held_by_me();
        alone.table.cards.retain(|card| card.id != fixtures::SPRITE);
        alone.resolve();
        let mut ctx = alone.ctx();
        hold(&mut ctx, 0);
        assert_eq!(recital_items(&ctx), [0]);
        assert_eq!(
            fixtures::labels(&ctx),
            [card_label(fixtures::VI), "skip".to_string()],
            "the Homebody in base is never a candidate"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_holding_gets_the_trigger_and_a_hold_elsewhere_is_not_here() {
        let mut theirs = recital_held_by_me();
        theirs.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        theirs.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        theirs.blob.set_holder(fixtures::BF1, Some(1));
        theirs.resolve();
        let mut ctx = theirs.ctx();
        hold(&mut ctx, 1);
        assert_eq!(recital_items(&ctx), [1]);
        assert_eq!(
            ctx.blob.prompt.as_ref().map(|prompt| prompt.seat),
            Some(1),
            "the holder chooses"
        );
        fixtures::choose(&mut ctx, 1, &card_label(fixtures::THEIR_UNIT)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), Some(Location::Base(1)));
        drop(ctx);
        let mut elsewhere = recital_held_by_me();
        elsewhere.blob.set_holder(fixtures::BF1, None);
        elsewhere.resolve();
        let mut ctx = elsewhere.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(
            recital_items(&ctx).is_empty(),
            "the Sprite's hold is at Rockfall Path"
        );
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_holder_can_not_send_an_enemy_jagged_cutlass_wearer_home_but_may_send_their_own() {
        const THEIR_CUTLASS: u32 = 91;
        const MY_CUTLASS: u32 = 92;
        let mut fixture = recital_held_by_me();
        fixture
            .table
            .cards
            .push(equipment(THEIR_CUTLASS, 1, "Jagged Cutlass", 3, "Body"));
        fixture
            .table
            .cards
            .push(equipment(MY_CUTLASS, 0, "Jagged Cutlass", 3, "Body"));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, THEIR_CUTLASS, fixtures::SPRITE);
        attach_gear(&mut ctx, MY_CUTLASS, fixtures::VI);
        hold(&mut ctx, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                card_label(fixtures::VI),
                card_label(fixtures::SPRITE),
                "skip".to_string()
            ],
            "both wearers are offered"
        );
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::SPRITE)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2)),
            "an enemy trigger can't move the wearer"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} can't be moved by {{card {RECITAL}}}",
            fixtures::SPRITE
        )));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert_eq!(ctx.points(0), 1, "the hold still scored");
        drop(ctx);

        let mut fixture = recital_held_by_me();
        fixture
            .table
            .cards
            .push(equipment(MY_CUTLASS, 0, "Jagged Cutlass", 3, "Body"));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, MY_CUTLASS, fixtures::VI);
        hold(&mut ctx, 0);
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::VI)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "your own trigger moves your own wearer"
        );
        assert!(ctx.fault.is_none());
    }
}
