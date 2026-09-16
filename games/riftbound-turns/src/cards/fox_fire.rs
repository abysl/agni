use super::prelude::{asking, card_targets, done, kill, play, spell, target, with_candidates};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const TOTAL_MIGHT: i32 = 4;
pub const ANY_NUMBER: u8 = u8::MAX;
pub const SUBSET: u8 = 1;
pub const QUESTION: &str = "which of the chosen units still fit under 4 total Might";

pub const UNITS_AT_ONE_BATTLEFIELD: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::AtBattlefield,
        Filter::TotalMightAtMost(4),
        Filter::SameLocationAsPicks,
    ]),
    0,
    ANY_NUMBER,
    TargetKind::Card,
    "any number of units at one battlefield with total Might 4 or less",
);

pub fn total_might(ctx: &Ctx, units: &[u32]) -> i32 {
    units.iter().map(|unit| ctx.current_might(*unit)).sum()
}

pub fn fits_total_might(ctx: &Ctx, chosen: &[u32], picked: &[u32]) -> Vec<u32> {
    let used = total_might(
        ctx,
        &picked
            .iter()
            .copied()
            .filter(|unit| chosen.contains(unit))
            .collect::<Vec<u32>>(),
    );
    chosen
        .iter()
        .copied()
        .filter(|unit| !picked.contains(unit))
        .filter(|unit| used + ctx.current_might(*unit) <= TOTAL_MIGHT)
        .collect()
}

fn still_fitting(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let chosen = card_targets(ctx, item);
    let picked: Vec<u32> = ctx
        .blob
        .prompt
        .as_ref()
        .map(|prompt| prompt.picked.clone())
        .unwrap_or_default();
    fits_total_might(ctx, &chosen, &picked)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn burn_them(ctx: &mut Ctx, item: &Item, units: &[u32]) {
    for unit in units {
        ctx.narrate(format!("{{card {unit}}} burns"));
        kill(ctx, item, *unit);
    }
}

fn fox_fire(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let chosen = card_targets(ctx, item);
    if stage.0 == SUBSET {
        let mut kept = Vec::new();
        for unit in ctx.picks().iter().copied() {
            if chosen.contains(&unit)
                && !kept.contains(&unit)
                && total_might(ctx, &kept) + ctx.current_might(unit) <= TOTAL_MIGHT
            {
                kept.push(unit);
            }
        }
        burn_them(ctx, item, &kept);
        return done();
    }
    if chosen.is_empty() {
        return done();
    }
    if total_might(ctx, &chosen) <= TOTAL_MIGHT {
        burn_them(ctx, item, &chosen);
        return done();
    }
    ctx.narrate(format!(
        "{{card {}}} · the chosen units now total more than {TOTAL_MIGHT} Might · a subset is chosen",
        item.kind.source()
    ));
    let count = u8::try_from(chosen.len()).unwrap_or(u8::MAX);
    Flow::Ask(ctx.ask_resume(item, SUBSET, 0, count))
}

pub static CARD: Card = spell(
    "Fox-Fire",
    &[Keyword::Hidden, Keyword::Action],
    &[asking(
        with_candidates(play(&[UNITS_AT_ONE_BATTLEFIELD], fox_fire), still_fitting),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{might_this_turn, Location};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{hide, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const FOX_FIRE: u32 = 90;
    const ONE: u32 = 91;
    const TWO: u32 = 92;
    const THREE: u32 = 93;
    const ELSEWHERE: u32 = fixtures::SPRITE;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut fox_fire = fixtures::spell(FOX_FIRE, fixtures::HAND, 0, "Fox-Fire", 0, 0);
        fox_fire.domain = vec!["Calm".into(), "Mind".into()];
        fixture.table.cards.push(fox_fire);
        fixture
            .table
            .cards
            .push(fixtures::unit(ONE, fixtures::BF1, 1, "Poro", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(TWO, fixtures::BF1, 1, "Yordle", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(THREE, fixtures::BF1, 0, "Recruit", 3));
        fixture.table.card_mut(fixtures::VI).unwrap().might = Some(5);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_hidden_action_over_small_units_at_one_battlefield() {
        assert!(std::ptr::eq(script_of("Fox-Fire").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Action));
        let spec = CARD.abilities[0].targets[0];
        assert_eq!((spec.min, spec.max), (0, ANY_NUMBER));
        assert_eq!(spec.filter, UNITS_AT_ONE_BATTLEFIELD.filter);
        assert_eq!(CARD.abilities[0].question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(total_might(&ctx, &[ONE, TWO, THREE]), 6);
        assert_eq!(
            fits_total_might(&ctx, &[ONE, TWO, THREE], &[]),
            [ONE, TWO, THREE]
        );
        assert_eq!(fits_total_might(&ctx, &[ONE, TWO, THREE], &[THREE]), [ONE]);
        assert_eq!(fits_total_might(&ctx, &[ONE, TWO, THREE], &[TWO]), [ONE]);
        assert!(fits_total_might(&ctx, &[ONE, TWO, THREE], &[ONE, THREE]).is_empty());
    }

    #[test]
    fn units_at_one_battlefield_are_chosen_together_and_all_die_within_four_might() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FOX_FIRE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {ELSEWHERE}}}"),
                format!("{{card {ONE}}}"),
                format!("{{card {TWO}}}"),
                format!("{{card {THREE}}}"),
                "done".to_string(),
                "skip".to_string(),
                "cancel".to_string()
            ],
            "units at battlefields with Might 4 or less; the 5-Might Vi in base is out"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ONE}}}")).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {TWO}}}"),
                format!("{{card {THREE}}}"),
                "done".to_string(),
                "cancel".to_string()
            ],
            "after the first pick only its battlefield remains: the Sprite at the other one is gone"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THREE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.prompt.is_none(), "1 + 3 fits: no subset question");
        assert_eq!(ctx.card(ONE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(THREE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(TWO));
        assert!(ctx.on_board(ELSEWHERE));
        assert_eq!(ctx.card(FOX_FIRE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_pump_in_response_forces_a_subset_that_still_fits() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FOX_FIRE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {ONE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TWO}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, TWO, 2, None);
        assert_eq!(ctx.current_might(TWO), 4);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: SUBSET
            }),
            "355.11.b · 1 + 4 no longer fits"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {ONE}}}"),
                format!("{{card {TWO}}}"),
                "done".to_string(),
                "skip".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {TWO}}}")).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing else fits beside 4 Might, the lone done answers itself: {:?}",
            fixtures::labels(&ctx)
        );
        assert_eq!(ctx.card(TWO).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(ONE));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn from_facedown_only_the_hiding_battlefield_is_in_reach_and_zero_picks_is_a_legal_play() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        hide::hide(&mut ctx, 0, FOX_FIRE, fixtures::BF1).unwrap();
        ctx.blob.card_state_mut(FOX_FIRE).hidden_since = 0;
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        hide::play_from_facedown(&mut ctx, 0, FOX_FIRE).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {ONE}}}"),
                format!("{{card {TWO}}}"),
                format!("{{card {THREE}}}"),
                "done".to_string(),
                "skip".to_string(),
                "cancel".to_string()
            ],
            "the Sprite at the other battlefield is out of reach"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Spell { card }) if card == FOX_FIRE
        ));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.on_board(ONE) && ctx.on_board(TWO) && ctx.on_board(THREE));
        assert_eq!(ctx.blob.chain.len(), 1, "Spark still waits below");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FOX_FIRE}}} resolves")));
        assert_eq!(
            ctx.location(ONE),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }

    #[test]
    fn the_opponent_cannot_choose_the_subset_and_the_spell_is_not_theirs_to_play() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FOX_FIRE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {ONE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TWO}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, ONE, 3, None);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert!(ctx.on_board(ONE) && ctx.on_board(TWO));
    }

    #[test]
    fn at_play_a_second_pick_that_breaks_four_total_might_is_not_offered() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FOX_FIRE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THREE}}}")).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {ONE}}}"),
                "done".to_string(),
                "cancel".to_string()
            ],
            "the 2-Might Yordle would make 5"
        );
    }
}
