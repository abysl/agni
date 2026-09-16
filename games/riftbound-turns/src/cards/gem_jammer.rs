use super::prelude::{a_unit, card_target, done, grant_this_turn, play, unit};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const GRANTED: Keyword = Keyword::Ganking;

pub const RUNNER: TargetSpec = a_unit("a unit to give Ganking this turn");

fn jam(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if grant_this_turn(ctx, unit, GRANTED) {
            ctx.narrate(format!("{{card {unit}}} has Ganking this turn"));
        }
    }
    done()
}

pub static CARD: Card = unit("Gem Jammer", &[], &[play(&[RUNNER], jam)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Filter, TargetKind, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, play as play_engine, priority};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const JAMMER: u32 = 90;

    fn jammer(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            ..fixtures::unit(JAMMER, zone, 0, "Gem Jammer", 2)
        }
    }

    fn workshop() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jammer(fixtures::HAND));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn land(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, JAMMER).unwrap();
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

    fn across(ctx: &Ctx, unit: u32) -> Result<(), Refusal> {
        march::legal_destination(
            ctx,
            unit,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_any_one_unit() {
        assert!(std::ptr::eq(script_of("Gem Jammer").unwrap(), &CARD));
        assert_eq!(CARD.name, "Gem Jammer");
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
        assert_eq!(ability.targets, &[RUNNER]);
        assert_eq!((RUNNER.min, RUNNER.max), (1, 1));
        assert_eq!(RUNNER.kind, TargetKind::Card);
        assert_eq!(RUNNER.filter, Filter::Unit);
        assert_eq!(GRANTED, Keyword::Ganking);
    }

    #[test]
    fn playing_him_offers_every_unit_and_the_pick_gains_ganking_until_the_turn_ends() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        land(&mut ctx);
        assert_eq!(ctx.location(JAMMER), Some(Location::Base(0)));
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {JAMMER}}}"),
            ],
            "friendly and enemy units alike, himself included"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::LEGEND_CARD]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a legend is not a unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        assert_eq!(
            across(&ctx, fixtures::VI),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == JAMMER
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(
            !ctx.has_keyword(fixtures::VI, Keyword::Ganking),
            "the grant waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(
            across(&ctx, fixtures::VI),
            Ok(()),
            "736 · battlefield to battlefield"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} has Ganking this turn", fixtures::VI)));
        assert!(!ctx.has_keyword(JAMMER, Keyword::Ganking), "only the pick");
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert!(
            !ctx.has_keyword(fixtures::VI, Keyword::Ganking),
            "the grant ends with the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_pick_that_left_the_board_before_resolution_gains_nothing() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        land(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        ctx.bounce(fixtures::THEIR_UNIT);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(fixtures::THEIR_UNIT));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("has Ganking this turn")));
    }
}
