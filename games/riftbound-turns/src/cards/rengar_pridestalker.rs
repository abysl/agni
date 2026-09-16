use super::prelude::{a_unit, card_target, done, legend, might_this_turn, on_you_play_card, when};
use super::{Card, Flow, Item, Source, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};

pub const BONUS: i16 = 1;

pub fn a_unit_of_yours(_: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::Played { kind, .. } if kind == KIND_UNIT)
}

fn stalk(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, BONUS, None);
        ctx.narrate(format!("{{card {unit}}} gets +{BONUS} Might this turn"));
    }
    done()
}

pub static CARD: Card = legend(
    "Rengar - Pridestalker",
    &[],
    &[when(
        on_you_play_card(&[a_unit("a unit to give +1 Might this turn")], stalk),
        a_unit_of_yours,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, this_turn, Location, Token};
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const RENGAR: u32 = fixtures::LEGEND_CARD;

    fn hunt() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(RENGAR).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RENGAR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn his_trigger_is_queued(ctx: &Ctx) -> bool {
        ctx.blob.chain.iter().any(
            |item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == RENGAR),
        )
    }

    #[test]
    fn the_legend_watches_the_units_its_controller_plays_and_targets_a_unit() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlayCard);
        assert!(ability.condition.is_some());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].min, 1);
        assert_eq!(ability.targets[0].max, 1);
        assert_eq!(BONUS, 1);
        let mut fixture = hunt();
        let ctx = fixture.ctx();
        let me = Source {
            card: RENGAR,
            ability: 0,
        };
        let played = |kind: &str| Event::Played {
            card: fixtures::HAND_UNIT,
            controller: 0,
            kind: kind.into(),
            origin: Origin::Hand,
            paid_additional: false,
        };
        assert!(a_unit_of_yours(&ctx, &played("Unit"), me));
        assert!(!a_unit_of_yours(&ctx, &played("Gear"), me));
        assert!(!a_unit_of_yours(&ctx, &played("Spell"), me));
        assert!(!a_unit_of_yours(
            &ctx,
            &Event::PlayedSpell {
                item: 1,
                controller: 0,
                nth: 1
            },
            me
        ));
    }

    #[test]
    fn playing_a_unit_asks_for_a_unit_and_the_pick_reads_one_more_might_until_the_turn_ends() {
        let mut fixture = hunt();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {RENGAR}}}: choose a unit to give +1 Might this turn (0 of 1)")
        );
        let offered = fixtures::labels(&ctx);
        assert!(
            offered.contains(&format!("{{card {}}}", fixtures::HAND_UNIT)),
            "the unit just played is a unit: {offered:?}"
        );
        assert!(offered.contains(&format!("{{card {}}}", fixtures::VI)));
        assert!(
            offered.contains(&format!("{{card {}}}", fixtures::THEIR_UNIT)),
            "any unit, not only yours"
        );
        assert!(
            !offered.contains(&"cancel".to_string()),
            "a trigger has no cancel"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(his_trigger_is_queued(&ctx));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "nothing until it resolves"
        );
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +1 Might this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "gone with the turn");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_cannot_pick_for_him_and_a_spell_or_a_gear_is_not_a_unit() {
        let mut fixture = hunt();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_UNIT).unwrap();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        drop(ctx);
        let mut fixture = hunt();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        assert!(ctx.on_board(fixtures::HAND_GEAR));
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "a gear is not a unit");
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "nor is a spell");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_token_unit_you_play_is_a_unit_you_play() {
        let mut fixture = hunt();
        let mut ctx = fixture.ctx();
        let sprite = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), false).unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { spec: 0, .. })),
            "a Sprite played is a unit played"
        );
        assert!(fixtures::labels(&ctx).contains(&format!("{{card {sprite}}}")));
        fixtures::choose(&mut ctx, 0, &format!("{{card {sprite}}}")).unwrap();
        assert!(his_trigger_is_queued(&ctx));
        resolve_top(&mut ctx);
        assert_eq!(ctx.current_might(sprite), 4);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_unit_is_not_a_unit_you_play() {
        let mut theirs = hunt();
        let unit = theirs.table.card_mut(fixtures::HAND_UNIT).unwrap();
        unit.seat = 1;
        unit.owner = 1;
        unit.energy = Some(0);
        theirs.blob.core_mut().unwrap().advance();
        theirs.resolve();
        let mut ctx = theirs.ctx();
        assert_eq!(ctx.turn_player(), 1);
        fixtures::play_from_hand(&mut ctx, 1, fixtures::HAND_UNIT).unwrap();
        assert!(ctx.on_board(fixtures::HAND_UNIT));
        assert!(
            ctx.blob.chain.is_empty(),
            "the opponent's unit is not yours"
        );
        assert!(ctx.fault.is_none());
    }
}
