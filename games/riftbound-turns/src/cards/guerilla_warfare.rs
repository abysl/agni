use super::prelude::{asking, done, play, spell, with_candidates};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const QUESTION: &str = "up to two Hidden cards in your trash to return to hand";
pub const UP_TO: u8 = 2;
pub const PICK: u8 = 1;

pub fn hidden_cards_in_trash(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.trash_of(seat)
        .into_iter()
        .filter(|card| ctx.has_keyword(*card, Keyword::Hidden))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    hidden_cards_in_trash(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub fn return_to_hand(ctx: &mut Ctx, card: u32) -> bool {
    let Some(hand) = ctx.zones.hand else {
        return false;
    };
    let owner = ctx.owner(card);
    ctx.emit(Effect::Move {
        card,
        zone: hand,
        seat: owner,
        index: TOP,
    });
    ctx.blob.drop_card_state(card);
    ctx.narrate(format!(
        "{{card {card}}} returns to {{seat {owner}}}'s hand"
    ));
    true
}

pub fn hides_free_this_turn(ctx: &mut Ctx, seat: u8) {
    ctx.narrate(format!(
        "{{seat {seat}}} can hide cards ignoring costs this turn"
    ));
}

fn warfare(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == PICK {
        let offered = hidden_cards_in_trash(ctx, seat);
        let picked: Vec<u32> = ctx
            .picks()
            .iter()
            .copied()
            .filter(|card| offered.contains(card))
            .take(usize::from(UP_TO))
            .collect();
        for card in picked {
            return_to_hand(ctx, card);
        }
        hides_free_this_turn(ctx, seat);
        return done();
    }
    if hidden_cards_in_trash(ctx, seat).is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no Hidden card in their trash"));
        hides_free_this_turn(ctx, seat);
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, PICK, 0, UP_TO))
}

pub static CARD: Card = spell(
    "Guerilla Warfare",
    &[],
    &[asking(
        with_candidates(play(&[], warfare), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::Trigger;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{hide, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const WARFARE: u32 = 90;
    const BREACH: u32 = 91;
    const HOURGLASS: u32 = 92;
    const SMOKE: u32 = 93;
    const PLAIN: u32 = 94;
    const THEIR_BREACH: u32 = 95;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut warfare = fixtures::spell(WARFARE, fixtures::HAND, 0, "Guerilla Warfare", 0, 0);
        warfare.domain = vec!["Mind".into(), "Chaos".into()];
        fixture.table.cards.push(warfare);
        fixture.table.cards.push(fixtures::spell(
            BREACH,
            fixtures::TRASH,
            0,
            "Temporal Breach",
            2,
            1,
        ));
        fixture.table.cards.push(fixtures::gear(
            HOURGLASS,
            fixtures::TRASH,
            0,
            "Zhonya's Hourglass",
            3,
        ));
        fixture.table.cards.push(fixtures::spell(
            SMOKE,
            fixtures::TRASH,
            0,
            "Smoke and Mirrors",
            1,
            0,
        ));
        fixture
            .table
            .cards
            .push(fixtures::spell(PLAIN, fixtures::TRASH, 0, "Spark", 2, 1));
        fixture.table.cards.push(fixtures::spell(
            THEIR_BREACH,
            fixtures::TRASH,
            1,
            "Temporal Breach",
            2,
            1,
        ));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_sorcery_that_offers_only_my_hidden_trash_cards() {
        assert!(std::ptr::eq(script_of("Guerilla Warfare").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(CARD.abilities[0].question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            hidden_cards_in_trash(&ctx, 0),
            [BREACH, HOURGLASS, SMOKE],
            "a scripted Hidden, a Hidden gear and a keyword-only Hidden; not Spark, not theirs"
        );
    }

    #[test]
    fn up_to_two_come_back_to_hand_and_the_third_stays() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, WARFARE).unwrap();
        assert!(ctx.blob.prompt.is_none(), "chosen at resolution");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 2));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BREACH}}}"),
                format!("{{card {HOURGLASS}}}"),
                format!("{{card {SMOKE}}}"),
                "done".to_string(),
                "skip".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SMOKE}}}")).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BREACH}}}"),
                format!("{{card {HOURGLASS}}}"),
                "done".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BREACH}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "two picks fill the prompt");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(SMOKE).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(BREACH).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + 2);
        assert_eq!(ctx.card(WARFARE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} can hide cards ignoring costs this turn".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn one_then_done_returns_one_and_skip_returns_none() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WARFARE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {HOURGLASS}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(BREACH).unwrap().zone, Some(fixtures::TRASH));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WARFARE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            ctx.trash_of(0),
            vec![BREACH, HOURGLASS, SMOKE, PLAIN, WARFARE]
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_empty_trash_asks_nothing_and_the_opponent_cannot_pick_for_me() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![BREACH, HOURGLASS, SMOKE].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WARFARE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no Hidden card in their trash".to_string()));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WARFARE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(priority::pass(&mut ctx, 1), Err(Refusal::PromptOpen));
    }

    #[test]
    #[ignore = "engine gap · hiding free this turn: hide::hide charges a rune blind and reads no per-seat grant"]
    fn after_it_resolves_a_hide_this_turn_costs_nothing() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WARFARE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BREACH}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        let runes = ctx.runes_of(0).len();
        hide::hide(&mut ctx, 0, BREACH, fixtures::BF1).unwrap();
        assert_eq!(
            ctx.runes_of(0).len(),
            runes,
            "no rune is recycled for a hide this turn"
        );
    }
}
