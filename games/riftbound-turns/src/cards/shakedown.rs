use super::prelude::{
    an_enemy_unit, asking, card_target, deal, done, draw, play, spell, with_candidates,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const DAMAGE: u8 = 6;
pub const DRAWS: usize = 2;
const STAGE_ANSWERED: u8 = 1;

fn take_it_or_let_them_draw(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != STAGE_ANSWERED {
        return Vec::new();
    }
    card_target(ctx, item, 0)
        .map(TargetRef::Card)
        .into_iter()
        .collect()
}

fn shake(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let caster = item.controller;
    if stage.0 != STAGE_ANSWERED {
        let payer = ctx.controller(unit);
        ctx.narrate(format!(
            "{{seat {payer}}} may have {{seat {caster}}} draw {DRAWS} to spare {{card {unit}}}"
        ));
        return Flow::Ask(ctx.ask_seat_resume(item, payer, STAGE_ANSWERED, 0, 1));
    }
    if ctx.picks().first() == Some(&unit) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
        return done();
    }
    ctx.narrate(format!("{{seat {caster}}} draws {DRAWS} instead"));
    draw(ctx, caster, DRAWS);
    done()
}

pub static CARD: Card = spell(
    "Shakedown",
    &[Keyword::Reaction],
    &[asking(
        with_candidates(
            play(&[an_enemy_unit("an enemy unit")], shake),
            take_it_or_let_them_draw,
        ),
        "the unit to take 6 · skip to let the caster draw 2",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const SHAKEDOWN: u32 = 90;
    const BRUTE: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            SHAKEDOWN,
            fixtures::HAND,
            0,
            "Shakedown",
            2,
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture.resolve();
        fixture
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, SHAKEDOWN).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn shakedown_is_a_reaction_that_chooses_an_enemy_unit_and_asks_its_controller() {
        assert!(std::ptr::eq(script_of("Shakedown").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].candidates.is_some());
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHAKEDOWN).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 91}", "cancel"],
            "enemy units anywhere, never Vi"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_ANSWERED
            })
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap();
        assert_eq!(
            (prompt.seat, prompt.min, prompt.max),
            (1, 0, 1),
            "the unit's controller answers"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BRUTE}}}"), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Resume {
                    item: 1,
                    stage: STAGE_ANSWERED
                }
            ),
            "{seat 1}: choose the unit to take 6 · skip to let the caster draw 2 (0 of 1)"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 1}} may have {{seat 0}} draw 2 to spare {{card {BRUTE}}}"
        )));
        let id = prompt.id;
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt: id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 })),
            "the caster cannot answer for them"
        );
    }

    #[test]
    fn taking_it_deals_six_and_the_caster_draws_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, BRUTE);
        fixtures::choose(&mut ctx, 1, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(BRUTE), "six kills the 5-Might Brute");
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "no draw");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(SHAKEDOWN).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn letting_the_caster_draw_two_spares_the_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, BRUTE);
        fixtures::choose(&mut ctx, 1, "skip").unwrap();
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(ctx.on_board(BRUTE));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + 1,
            "the spell left, two came in"
        );
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
                .count(),
            2
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws 2 instead".to_string()));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_left_the_board_is_neither_asked_about_nor_hit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SHAKEDOWN).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        ctx.bounce(fixtures::SPRITE);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing to decide about a gone unit"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
    }

    #[test]
    fn a_unit_whose_id_equals_the_caster_seat_is_spared_when_its_controller_lets_them_draw() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            SHAKEDOWN,
            fixtures::HAND,
            0,
            "Shakedown",
            2,
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(0, fixtures::BF1, 1, "Brute", 5));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast_at(&mut ctx, 0);
        assert_eq!(fixtures::labels(&ctx), ["{card 0}", "skip"]);
        fixtures::choose(&mut ctx, 1, "skip").unwrap();
        assert!(
            ctx.on_board(0),
            "card 0 and seat 0 share a u32 · the skip cannot be mistaken for the unit"
        );
        assert_eq!(ctx.damage_on(0), 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.fault.is_none());
    }
}
