use super::prelude::{
    at_end_of_turn, card_target, disempower, done, on_play_from_facedown, optional, target,
    triggered, unit,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec, Trigger};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const DISEMPOWER_AT_END_OF_TURN: u8 = 1;
pub const SOMETHING_HERE: Filter =
    Filter::And(&[Filter::Or(&[Filter::Unit, Filter::Gear]), Filter::Here]);
pub const GUST: TargetSpec = target(
    SOMETHING_HERE,
    0,
    1,
    TargetKind::Card,
    "something here to empower until the end of the turn",
);

fn gust(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(thing) = card_target(ctx, item, 0) else {
        return done();
    };
    if ctx.empower_by(thing, item.controller) {
        ctx.narrate(format!("{{card {thing}}} is empowered"));
    } else {
        ctx.narrate(format!("{{card {thing}}} was Empowered already"));
    }
    at_end_of_turn(ctx, item, DISEMPOWER_AT_END_OF_TURN, vec![thing]);
    ctx.narrate(format!(
        "{{card {thing}}} is disempowered at the end of the turn"
    ));
    done()
}

fn calm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(TargetRef::Card(thing)) = item.targets.first() {
        if disempower(ctx, *thing) {
            ctx.narrate(format!("{{card {thing}}} is disempowered"));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Tornado Warrior",
    &[Keyword::Hidden],
    &[
        optional(on_play_from_facedown(&[GUST], gust)),
        triggered(Trigger::Reflexive, &[], calm),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{phases, play as play_engine, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, When};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const WARRIOR: u32 = 90;

    fn warrior() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(WARRIOR, fixtures::BF1, 0, "Tornado Warrior", 3)
        }
    }

    fn windswept() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(warrior());
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.card_state_mut(WARRIOR).hidden_at = Some(fixtures::BF1);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(WARRIOR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn play_facedown(ctx: &mut Ctx) -> u16 {
        play_engine::begin(
            ctx,
            0,
            WARRIOR,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            Some(Location::Battlefield(fixtures::BF1)),
        )
        .unwrap();
        settle(ctx).unwrap();
        fixtures::pass_until_open(ctx);
        let item = ctx
            .blob
            .queue
            .iter()
            .find(|pending| {
                matches!(pending.item.kind, ItemKind::Trigger { source, index: 0 } if source == WARRIOR)
            })
            .map(|pending| pending.item.id)
            .expect("the played-from-face-down trigger is pending");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        item
    }

    #[test]
    fn the_script_is_hidden_with_an_optional_from_face_down_trigger_and_a_delayed_disempower() {
        assert!(std::ptr::eq(script_of("Tornado Warrior").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let entrance = &CARD.abilities[0];
        assert_eq!(entrance.trigger, Trigger::PlayFromFacedown);
        assert!(entrance.optional);
        assert_eq!(entrance.targets, [GUST]);
        assert_eq!((GUST.min, GUST.max), (0, 1), "the may is a 0-of-1 target");
        assert_eq!(GUST.filter, SOMETHING_HERE);
        assert_eq!(
            CARD.abilities[usize::from(DISEMPOWER_AT_END_OF_TURN)].trigger,
            Trigger::Reflexive
        );
    }

    #[test]
    fn played_from_face_down_it_offers_what_is_here_and_the_pick_stays_empowered_until_the_end_of_the_turn(
    ) {
        let mut fixture = windswept();
        let mut ctx = fixture.ctx();
        let item = play_facedown(&mut ctx);
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        let mut expected = [
            format!("{{card {}}}", fixtures::VI),
            format!("{{card {WARRIOR}}}"),
            "skip".to_string(),
        ];
        expected.sort();
        assert_eq!(offered, expected, "the units here, itself included");
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy in its base is not here"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit at the other battlefield is not here"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(
            !ctx.is_empowered(fixtures::VI),
            "the empower waits for the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_empowered(fixtures::VI));
        assert!(ctx.events.contains(&Event::Empowered {
            card: fixtures::VI,
            by: 0
        }));
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].when, When::EndOfTurn(ctx.turn()));
        assert_eq!(ctx.blob.delayed[0].source, WARRIOR);
        assert_eq!(ctx.blob.delayed[0].ability, DISEMPOWER_AT_END_OF_TURN);
        assert_eq!(ctx.blob.delayed[0].args, [fixtures::VI]);
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the disempower is a trigger on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger {
                source: WARRIOR,
                index: DISEMPOWER_AT_END_OF_TURN
            }
        ));
        assert!(ctx.is_empowered(fixtures::VI), "not until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_empowered(fixtures::VI));
        assert!(ctx
            .events
            .contains(&Event::Disempowered { card: fixtures::VI }));
        assert!(ctx.blob.delayed.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_the_may_empowers_nothing_and_schedules_nothing() {
        let mut fixture = windswept();
        let mut ctx = fixture.ctx();
        play_facedown(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.delayed.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Empowered { .. })));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_thing_empowered_already_is_still_disempowered_at_the_end_of_the_turn() {
        let mut fixture = windswept();
        let mut ctx = fixture.ctx();
        assert!(ctx.empower(fixtures::VI));
        play_facedown(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} was Empowered already", fixtures::VI)));
        assert_eq!(ctx.blob.delayed.len(), 1);
        phases::end_turn(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.is_empowered(fixtures::VI));
        assert!(ctx.fault.is_none());
    }
}
