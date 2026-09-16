use super::fox_fire::total_might;
use super::prelude::{asking, bounce, card_targets, done, play, spell, target, with_candidates};
use super::{Card, Domain, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const TOTAL_MIGHT: i32 = 5;
pub const ANY_NUMBER: u8 = u8::MAX;
pub const SUBSET: u8 = 1;
pub const QUESTION: &str = "which of the chosen Order units still fit under 5 total Might";

pub const ENEMY_ORDER_UNITS_UNDER_FIVE_MIGHT: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::Enemy,
        Filter::Domain(Domain::Order),
        Filter::TotalMightAtMost(5),
    ]),
    0,
    ANY_NUMBER,
    TargetKind::Card,
    "any number of enemy Order units with total Might 5 or less",
);

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

fn send_home(ctx: &mut Ctx, units: &[u32]) {
    for unit in units {
        if bounce(ctx, *unit) {
            ctx.narrate(format!("{{card {unit}}} returns to its owner's hand"));
        }
    }
}

fn discord(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
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
        send_home(ctx, &kept);
        return done();
    }
    if chosen.is_empty() {
        return done();
    }
    if total_might(ctx, &chosen) <= TOTAL_MIGHT {
        send_home(ctx, &chosen);
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
    "Decree of Discord",
    &[],
    &[asking(
        with_candidates(
            play(&[ENEMY_ORDER_UNITS_UNDER_FIVE_MIGHT], discord),
            still_fitting,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::might_this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const DECREE: u32 = 90;
    const THEIR_DECREE: u32 = 91;
    const KNIGHT: u32 = 92;
    const SQUIRE: u32 = 93;
    const PALADIN: u32 = 94;
    const MY_KNIGHT: u32 = 95;
    const CHAOS_RUNE: u32 = 100;

    fn decree_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Decree of Discord", 1, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn order(card: CardInfo) -> CardInfo {
        CardInfo {
            domain: vec!["Order".into()],
            ..card
        }
    }

    fn court() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(decree_card(DECREE, 0));
        fixture.table.cards.push(decree_card(THEIR_DECREE, 1));
        fixture
            .table
            .cards
            .push(order(fixtures::unit(KNIGHT, fixtures::BF1, 1, "Knight", 3)));
        fixture.table.cards.push(order(fixtures::unit(
            SQUIRE,
            fixtures::BASE,
            1,
            "Squire",
            2,
        )));
        fixture.table.cards.push(order(fixtures::unit(
            PALADIN,
            fixtures::BF1,
            1,
            "Paladin",
            6,
        )));
        fixture.table.cards.push(order(fixtures::unit(
            MY_KNIGHT,
            fixtures::BASE,
            0,
            "Knight",
            3,
        )));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_plain_sorcery_over_any_number_of_small_enemy_order_units() {
        assert!(std::ptr::eq(script_of("Decree of Discord").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        let spec = ability.targets[0];
        assert_eq!((spec.min, spec.max), (0, ANY_NUMBER));
        assert_eq!(spec.filter, ENEMY_ORDER_UNITS_UNDER_FIVE_MIGHT.filter);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(TOTAL_MIGHT, 5);
        let mut fixture = court();
        let ctx = fixture.ctx();
        assert_eq!(total_might(&ctx, &[KNIGHT, SQUIRE]), 5);
        assert_eq!(
            fits_total_might(&ctx, &[KNIGHT, SQUIRE], &[]),
            [KNIGHT, SQUIRE]
        );
        assert_eq!(
            fits_total_might(&ctx, &[KNIGHT, SQUIRE], &[KNIGHT]),
            [SQUIRE]
        );
        assert!(fits_total_might(&ctx, &[KNIGHT, PALADIN], &[KNIGHT]).is_empty());
    }

    #[test]
    fn enemy_order_units_within_five_total_might_all_go_home() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        let their_hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {KNIGHT}}}"),
                format!("{{card {SQUIRE}}}"),
                "done".to_string(),
                "skip".to_string(),
                "cancel".to_string()
            ],
            "enemy Order units of 5 or less; Jinx and the Sprite are Fury, the Paladin is 6, my Knight is mine"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {KNIGHT}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SQUIRE}}}")).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["done", "cancel"],
            "nothing else fits the filter; the group is closed by hand"
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.prompt.is_none(), "3 + 2 fits: no subset question");
        assert_eq!(ctx.card(KNIGHT).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(SQUIRE).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(1).len(), their_hand + 2);
        assert!(ctx.on_board(PALADIN));
        assert!(ctx.on_board(MY_KNIGHT));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {KNIGHT}}} returns to its owner's hand")));
        assert_eq!(ctx.card(DECREE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_pump_in_response_forces_a_subset_that_still_fits() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {KNIGHT}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SQUIRE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, KNIGHT, 1, None);
        assert_eq!(ctx.current_might(KNIGHT), 4);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: SUBSET
            }),
            "355.11.b · 4 + 2 no longer fits"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {KNIGHT}}}"),
                format!("{{card {SQUIRE}}}"),
                "done".to_string(),
                "skip".to_string()
            ]
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the opponent does not pick the subset"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {KNIGHT}}}")).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing else fits beside 4 Might: {:?}",
            fixtures::labels(&ctx)
        );
        assert_eq!(ctx.card(KNIGHT).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.on_board(SQUIRE));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn zero_picks_is_a_legal_play_and_the_wrong_units_are_refused() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DECREE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        for wrong in [
            PALADIN,
            MY_KNIGHT,
            fixtures::THEIR_UNIT,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is too big, mine, not Order or not on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain[0].targets.is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(KNIGHT) && ctx.on_board(SQUIRE));
        assert_eq!(ctx.card(DECREE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn at_play_a_second_pick_that_breaks_five_total_might_is_not_offered() {
        let mut fixture = court();
        fixture.table.card_mut(SQUIRE).unwrap().might = Some(3);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {KNIGHT}}}")).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["done".to_string(), "cancel".to_string()],
            "the 3-Might Squire would make 6"
        );
    }
}
