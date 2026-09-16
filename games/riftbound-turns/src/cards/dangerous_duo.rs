use super::prelude::{a_unit, card_target, done, legion, might_this_turn, play, unit, when};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const MIGHT: i16 = 2;

pub fn legion_on_entry(ctx: &Ctx, _: &Event, source: Source) -> bool {
    legion(ctx, ctx.controller(source.card))
}

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} Might this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Dangerous Duo",
    &[Keyword::Legion],
    &[when(
        play(&[a_unit("a unit to give +2 Might this turn")], rally),
        legion_on_entry,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{Filter, TargetKind, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as plays, prompts};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::Target;

    const DUO: u32 = 90;

    fn garage(legion_on: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut duo = fixtures::unit(DUO, fixtures::HAND, 0, "Dangerous Duo", 3);
        duo.domain = vec!["Fury".into()];
        duo.energy = Some(3);
        fixture.table.cards.push(duo);
        fixture.blob.seat_mut(0).played_main = legion_on;
        fixture.resolve();
        fixture
    }

    fn card_label(card: u32) -> String {
        format!("{{card {card}}}")
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn pending_item(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the trigger asks for a unit, not {other:?}"),
        }
    }

    #[test]
    fn the_script_is_a_legion_unit_whose_gated_play_trigger_chooses_any_unit() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Dangerous Duo").unwrap(),
            &CARD
        ));
        let fixture = garage(true);
        assert!(std::ptr::eq(fixture.scripts.of_card(DUO).unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Legion]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.condition.is_some(), "812.1.b.1 · the Legion gate");
        assert!(!ability.optional);
        assert_eq!(ability.targets.len(), 1);
        let spec = ability.targets[0];
        assert_eq!(
            spec.filter,
            Filter::Unit,
            "any unit, theirs or mine, even me"
        );
        assert_eq!((spec.min, spec.max), (1, 1));
        assert_eq!(spec.kind, TargetKind::Card);
        assert_eq!(MIGHT, 2);
    }

    #[test]
    fn with_legion_on_the_duo_asks_for_a_unit_and_gives_it_two_might_for_the_turn() {
        let mut fixture = garage(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DUO).unwrap();
        let item = pending_item(&ctx);
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {DUO}}}: choose a unit to give +2 Might this turn (0 of 1)")
        );
        let offered = fixtures::labels(&ctx);
        assert!(offered.contains(&card_label(fixtures::VI)));
        assert!(offered.contains(&card_label(fixtures::THEIR_UNIT)));
        assert!(offered.contains(&card_label(fixtures::SPRITE)));
        assert!(
            offered.contains(&card_label(DUO)),
            "a unit includes the duo itself"
        );
        assert!(
            !offered.contains(&"skip".to_string()),
            "one unit is required"
        );
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::VI)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DUO
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the bonus waits for the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        assert_eq!(ctx.current_might(DUO), 3, "only the chosen unit grows");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +2 Might this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the bonus is only for the turn"
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_unit_or_the_duo_itself_may_be_the_pick() {
        let mut fixture = garage(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DUO).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::THEIR_UNIT)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4);
        drop(ctx);

        let mut fixture = garage(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DUO).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(DUO)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(DUO), 5);
    }

    #[test]
    fn as_the_first_card_of_the_turn_the_duo_asks_nothing_and_buffs_nobody() {
        let mut fixture = garage(false);
        let mut ctx = fixture.ctx();
        assert!(!legion(&ctx, 0));
        fixtures::play_from_hand(&mut ctx, 0, DUO).unwrap();
        assert!(ctx.on_board(DUO));
        assert!(ctx.blob.prompt.is_none(), "no Legion, no target to ask for");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_cannot_answer_and_a_gear_is_not_a_unit() {
        let mut fixture = garage(true);
        fixture.table.cards.push(fixtures::gear(
            91,
            fixtures::BASE,
            0,
            "Boots of Swiftness",
            2,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DUO).unwrap();
        let item = pending_item(&ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            plays::choose_targets(&mut ctx, item, 0, &[91]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt is still open");
    }

    #[test]
    fn a_pick_that_left_the_board_before_the_trigger_resolves_gets_nothing() {
        let mut fixture = garage(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DUO).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(fixtures::VI)).unwrap();
        assert!(ctx.bounce(fixtures::VI));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("gets +2 Might this turn")));
    }
}
