use super::ornns_forge::non_token_gear_played_this_turn;
use super::prelude::{
    card_target, done, on_you_play_card, once_each_turn, optional, play, ready, target, unit, when,
};
use super::{Card, Filter, Flow, Item, Source, Stage, TargetKind, TargetSpec, KIND_GEAR};
use crate::engine::ctx::{Ctx, Event};

pub const SOMETHING_ELSE_EXHAUSTED: Filter = Filter::And(&[
    Filter::Or(&[Filter::Unit, Filter::Gear, Filter::Rune, Filter::Legend]),
    Filter::Exhausted,
    Filter::NotSelf,
]);

pub const SPARK: TargetSpec = target(
    SOMETHING_ELSE_EXHAUSTED,
    0,
    1,
    TargetKind::Card,
    "something else that's exhausted to ready",
);

pub fn first_non_token_gear(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Played {
        card,
        controller,
        kind,
        ..
    } = event
    else {
        return false;
    };
    kind == KIND_GEAR
        && *controller == ctx.controller(source.card)
        && !ctx.is_token(*card)
        && non_token_gear_played_this_turn(ctx, *controller) == 1
}

fn invent(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(card) = card_target(ctx, item, 0) else {
        return done();
    };
    if ready(ctx, card) {
        ctx.narrate(format!(
            "{{card {}}} readies {{card {card}}}",
            item.kind.source()
        ));
    }
    done()
}

pub static CARD: Card = unit(
    "Jayce, Brilliant Inventor",
    &[],
    &[
        optional(play(&[SPARK], invent)),
        optional(once_each_turn(when(
            on_you_play_card(&[SPARK], invent),
            first_non_token_gear,
        ))),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Once, Trigger, KIND_UNIT};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, prompts};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef, FLAG_ONCE_USED};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const JAYCE: u32 = 90;
    const SPENT_UNIT: u32 = 91;
    const SPENT_GEAR: u32 = 92;
    const SECOND_GEAR: u32 = 93;
    const THEIR_SPENT: u32 = 94;

    fn jayce(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Mind".into()],
            ..fixtures::unit(JAYCE, zone, 0, "Jayce, Brilliant Inventor", 6)
        }
    }

    fn lab(jayce_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jayce(jayce_zone));
        let mut spent = fixtures::unit(SPENT_UNIT, fixtures::BASE, 0, "Ally", 2);
        spent.exhausted = true;
        fixture.table.cards.push(spent);
        let mut gear = fixtures::gear(SPENT_GEAR, fixtures::BASE, 0, "Turret", 1);
        gear.exhausted = true;
        fixture.table.cards.push(gear);
        let mut theirs = fixtures::unit(THEIR_SPENT, fixtures::BASE, 1, "Brute", 4);
        theirs.exhausted = true;
        fixture.table.cards.push(theirs);
        fixture
            .table
            .cards
            .push(fixtures::gear(SECOND_GEAR, fixtures::HAND, 0, "Trinket", 0));
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().energy = Some(0);
        fixture.resolve();
        fixture
    }

    fn pending(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the trigger is pending")
    }

    #[test]
    fn the_script_asks_the_same_ready_question_on_his_play_and_on_the_first_gear_each_turn() {
        assert!(std::ptr::eq(
            script_of("Jayce, Brilliant Inventor").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Jayce, Brilliant Inventor");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let played = &CARD.abilities[0];
        assert_eq!(played.trigger, Trigger::Play);
        assert!(played.condition.is_none());
        assert_eq!(played.once, Once::Never);
        let gear = &CARD.abilities[1];
        assert_eq!(gear.trigger, Trigger::YouPlayCard);
        assert!(gear.condition.is_some());
        assert_eq!(gear.once, Once::PerTurn, "the first time each turn");
        for ability in CARD.abilities {
            assert!(ability.optional, "you may");
            assert_eq!(ability.targets, &[SPARK]);
        }
        assert_eq!((SPARK.min, SPARK.max), (0, 1));
        assert_eq!(SPARK.kind, TargetKind::Card);
        assert_eq!(SPARK.filter, SOMETHING_ELSE_EXHAUSTED);
    }

    #[test]
    fn the_gear_condition_reads_your_first_non_token_gear_of_the_turn() {
        let mut fixture = lab(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let source = Source {
            card: JAYCE,
            ability: 1,
        };
        let played = |card: u32, controller: u8, kind: &str| Event::Played {
            card,
            controller,
            kind: kind.to_string(),
            origin: Origin::Hand,
            paid_additional: false,
        };
        assert!(
            !first_non_token_gear(&ctx, &played(fixtures::HAND_GEAR, 0, KIND_GEAR), source),
            "no gear entered this turn yet · the count reads the board"
        );
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert_eq!(non_token_gear_played_this_turn(&ctx, 0), 1);
        assert!(first_non_token_gear(
            &ctx,
            &played(fixtures::HAND_GEAR, 0, KIND_GEAR),
            source
        ));
        assert!(!first_non_token_gear(
            &ctx,
            &played(fixtures::HAND_GEAR, 1, KIND_GEAR),
            source
        ));
        assert!(!first_non_token_gear(
            &ctx,
            &played(fixtures::HAND_UNIT, 0, KIND_UNIT),
            source
        ));
    }

    #[test]
    fn played_he_offers_every_exhausted_thing_but_himself_and_readies_the_pick() {
        let mut fixture = lab(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JAYCE).unwrap();
        assert_eq!(ctx.location(JAYCE), Some(Location::Base(0)));
        assert!(ctx.card(JAYCE).unwrap().exhausted);
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::RUNE_A),
                format!("{{card {SPENT_UNIT}}}"),
                format!("{{card {SPENT_GEAR}}}"),
                format!("{{card {THEIR_SPENT}}}"),
                "skip".to_string()
            ],
            "the exhausted rune, unit, gear and enemy unit · not Jayce, not the ready ones"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {JAYCE}}}: choose something else that's exhausted to ready (0 of 1)")
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[JAYCE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "something besides me"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Vi is ready"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SPENT_GEAR}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == JAYCE
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(SPENT_GEAR)]);
        assert!(ctx.card(SPENT_GEAR).unwrap().exhausted, "the ready waits");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(SPENT_GEAR).unwrap().exhausted);
        assert!(ctx.card(SPENT_UNIT).unwrap().exhausted, "only the pick");
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { card, .. } if *card == SPENT_GEAR)));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {JAYCE}}} readies {{card {SPENT_GEAR}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_first_gear_of_the_turn_asks_again_and_the_second_does_not() {
        let mut fixture = lab(fixtures::BASE);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert!(ctx.has_flag(JAYCE, FLAG_ONCE_USED));
        fixtures::choose(&mut ctx, 0, &format!("{{card {SPENT_UNIT}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == JAYCE
        ));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(SPENT_UNIT).unwrap().exhausted);
        fixtures::play_from_hand(&mut ctx, 0, SECOND_GEAR).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the second gear is not the first"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.card(SPENT_GEAR).unwrap().exhausted);
    }

    #[test]
    fn a_gear_played_before_him_this_turn_was_the_first_so_the_next_one_is_not() {
        let mut fixture = lab(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert!(ctx.blob.chain.is_empty(), "he is still in hand");
        fixtures::play_from_hand(&mut ctx, 0, JAYCE).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(SPENT_UNIT).unwrap().exhausted, "skipped");
        fixtures::play_from_hand(&mut ctx, 0, SECOND_GEAR).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.has_flag(JAYCE, FLAG_ONCE_USED));
        assert_eq!(non_token_gear_played_this_turn(&ctx, 0), 2);
    }
}
