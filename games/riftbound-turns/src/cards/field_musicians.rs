use super::prelude::{a_unit, card_target, done, might_this_turn, play, unit};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 3;

pub const AUDIENCE: TargetSpec = a_unit("a unit to give +3 Might this turn");

fn serenade(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!(
            "{{card {}}} gives {{card {unit}}} +{MIGHT} Might this turn",
            item.kind.source()
        ));
    }
    done()
}

pub static CARD: Card = unit("Field Musicians", &[], &[play(&[AUDIENCE], serenade)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Filter, TargetKind, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MUSICIANS: u32 = 90;

    fn musicians(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Calm".into()],
            ..fixtures::unit(MUSICIANS, zone, 0, "Field Musicians", 3)
        }
    }

    fn stage() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(musicians(fixtures::HAND));
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

    #[test]
    fn the_script_is_a_plain_unit_with_one_targeted_play_trigger() {
        assert!(std::ptr::eq(script_of("Field Musicians").unwrap(), &CARD));
        assert_eq!(CARD.name, "Field Musicians");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets, &[AUDIENCE]);
        assert_eq!((AUDIENCE.min, AUDIENCE.max), (1, 1));
        assert_eq!(AUDIENCE.kind, TargetKind::Card);
        assert_eq!(AUDIENCE.filter, Filter::Unit);
        assert_eq!(MIGHT, 3);
    }

    #[test]
    fn playing_them_asks_for_any_unit_and_the_pick_gains_three_might_for_the_turn() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MUSICIANS).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {MUSICIANS}}}"),
            ],
            "any unit, the musicians included"
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
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MUSICIANS
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "the buff waits");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert_eq!(ctx.current_might(MUSICIANS), 3, "only the pick");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MUSICIANS}}} gives {{card {}}} +3 Might this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "it ends with the turn");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_pick_that_left_the_board_before_resolution_gets_nothing() {
        let mut fixture = stage();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MUSICIANS).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(ctx.bounce(fixtures::THEIR_UNIT));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(fixtures::THEIR_UNIT));
        assert!(ctx
            .state_of(fixtures::THEIR_UNIT)
            .is_none_or(|row| row.might.is_empty()));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("+3 Might this turn")));
    }
}
