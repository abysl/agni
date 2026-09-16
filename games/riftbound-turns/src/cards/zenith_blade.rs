use super::prelude::{
    a_card, asking, card_target, done, move_unit, play, spell, stun, target, with_candidates,
    Location, ENEMY_UNIT, MOVABLE_FRIENDLY_UNIT,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const ENEMY: usize = 0;
const FRIENDLY: usize = 1;
const MOVE: u8 = 1;

pub const ENEMY_UNIT_AT_A_BATTLEFIELD: Filter = Filter::And(&[ENEMY_UNIT, Filter::AtBattlefield]);
pub const ENEMY_TARGET: TargetSpec = a_card(
    ENEMY_UNIT_AT_A_BATTLEFIELD,
    "an enemy unit at a battlefield",
);
pub const FRIENDLY_TARGET: TargetSpec = target(
    MOVABLE_FRIENDLY_UNIT,
    0,
    1,
    TargetKind::Card,
    "a friendly unit that may move there",
);
pub const QUESTION: &str = "the friendly unit to move to that battlefield";

fn destination(ctx: &Ctx, item: &Item) -> Option<Location> {
    let enemy = card_target(ctx, item, ENEMY)?;
    let at = ctx.location(enemy)?;
    at.battlefield().map(|_| at)
}

fn mover(ctx: &Ctx, item: &Item) -> Option<u32> {
    let unit = card_target(ctx, item, FRIENDLY)?;
    let to = destination(ctx, item)?;
    (ctx.location(unit) != Some(to)).then_some(unit)
}

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    mover(ctx, item).into_iter().map(TargetRef::Card).collect()
}

fn resolve(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == MOVE {
        let (Some(unit), Some(to)) = (mover(ctx, item), destination(ctx, item)) else {
            return done();
        };
        if ctx.picks().first() == Some(&unit) {
            move_unit(ctx, item, unit, to);
        }
        return done();
    }
    if let Some(enemy) = card_target(ctx, item, ENEMY) {
        stun(ctx, enemy);
    }
    if mover(ctx, item).is_none() {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, MOVE, 0, 1))
}

pub static CARD: Card = spell(
    "Zenith Blade",
    &[Keyword::Action],
    &[asking(
        with_candidates(play(&[ENEMY_TARGET, FRIENDLY_TARGET], resolve), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, prompts};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BLADE: u32 = 90;
    const ALLY: u32 = 91;
    const FOE: u32 = 92;
    const ORDER_RUNE: u32 = 46;

    fn blade() -> CardInfo {
        let mut card = fixtures::spell(BLADE, fixtures::HAND, 0, "Zenith Blade", 3, 2);
        card.domain = vec!["Calm".into(), "Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(blade());
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture.table.cards.push(fixtures::unit(
            ALLY,
            fixtures::BF1,
            0,
            "Legion Rearguard",
            1,
        ));
        fixture.table.cards.push(fixtures::unit(
            FOE,
            fixtures::BF2,
            1,
            "Vanguard Sergeant",
            2,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_an_action_choosing_an_enemy_at_a_battlefield_and_maybe_a_friendly_unit() {
        assert!(std::ptr::eq(script_of("Zenith Blade").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, [ENEMY_TARGET, FRIENDLY_TARGET]);
        assert_eq!((FRIENDLY_TARGET.min, FRIENDLY_TARGET.max), (0, 1));
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
    }

    #[test]
    fn the_enemy_is_stunned_and_the_chosen_friendly_unit_moves_there_on_a_yes_at_resolution() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "enemy units at battlefields, not Jinx in her base"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 91}", "skip", "cancel"],
            "355.12 · the friendly unit is chosen now, the move is decided later"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(FOE), TargetRef::Card(fixtures::VI)]
        );
        assert_eq!(
            [42, ORDER_RUNE]
                .iter()
                .filter(|rune| ctx.card(**rune).unwrap().zone == Some(fixtures::RUNE_DECK))
                .count(),
            2,
            "one Calm and one Order power"
        );
        assert!(!ctx.is_stunned(FOE), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_stunned(FOE));
        assert_eq!(ctx.combat_might(FOE), 0);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume { item: 1, stage: 1 }),
            "the may is asked as the spell resolves"
        );
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "skip"]);
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item: 1, stage: 1 }),
            "{card 90}: choose the friendly unit to move to that battlefield (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2)),
            "Vi arrives where the stunned enemy stands"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, cause: MoveCause::Effect, .. } if *card == fixtures::VI
        )));
        assert!(ctx.blob.log.contains(&"{card 92} is stunned".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} moves to {zone 10}".to_string()));
        assert_eq!(ctx.card(BLADE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_the_move_at_resolution_leaves_the_friendly_unit_where_it_was() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 1 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 91}", "skip"]);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(FOE));
        assert_eq!(
            ctx.location(ALLY),
            Some(Location::Battlefield(fixtures::BF1)),
            "the may was declined"
        );
        assert!(!ctx.blob.log.iter().any(|line| line.contains("moves to")));
        assert_eq!(ctx.card(BLADE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn without_a_friendly_unit_chosen_only_the_stun_happens_and_nothing_is_asked() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(fixtures::SPRITE));
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn an_enemy_that_went_home_is_neither_stunned_nor_a_destination() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(FOE, fixtures::BASE, 1), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.blob.prompt.is_none(),
            "no battlefield to move to, no question"
        );
        assert!(
            !ctx.is_stunned(FOE),
            "355.9.b · an enemy in its base is no longer at a battlefield"
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(ALLY, fixtures::BF2, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_stunned(FOE));
        assert!(
            ctx.blob.prompt.is_none(),
            "a friendly unit already there has nowhere to move"
        );
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn a_friendly_unit_an_enemy_in_a_base_and_a_wrong_second_pick_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[ALLY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is not an enemy"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy in its base is not at a battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy is not a friendly unit to move"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::VI, ALLY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "at most one friendly unit"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(BLADE).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(
            ctx.card(ORDER_RUNE).unwrap().zone,
            Some(fixtures::RUNE_POOL)
        );
        let mut quiet = armed();
        quiet
            .table
            .cards
            .retain(|card| card.id != FOE && card.id != fixtures::SPRITE);
        quiet.resolve();
        let mut ctx = quiet.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "no enemy unit at any battlefield"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
    }
}
