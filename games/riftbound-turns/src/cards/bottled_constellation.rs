use super::prelude::{
    asking, done, friendly_gear, friendly_units, gear, optional, score_point, triggered,
    with_candidates,
};
use super::{Card, Flow, Item, Stage, Trigger};
use crate::engine::ctx::{Cause, Ctx, Killed};
use crate::engine::statics;
use crate::state::TargetRef;

pub const KILLS: usize = 3;
pub const WISH: u8 = 0;
pub const KILL: u8 = 1;
pub const QUESTION: &str = "three other friendly units and/or gear to kill for a point";

pub fn wishes_at_the_start_of_the_main_phase_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut bottles: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .map(|held| held.id)
        .filter(|bottle| statics::in_play(ctx, *bottle) && ctx.controller(*bottle) == seat)
        .collect();
    bottles.sort_unstable();
    bottles
}

pub fn kill_candidates(ctx: &Ctx, seat: u8, bottle: u32) -> Vec<u32> {
    let mut candidates: Vec<u32> = friendly_units(ctx, seat)
        .into_iter()
        .chain(friendly_gear(ctx, seat))
        .filter(|card| *card != bottle)
        .collect();
    candidates.sort_unstable();
    candidates.dedup();
    candidates
}

pub fn kill_cost_payable(ctx: &Ctx, seat: u8, bottle: u32) -> bool {
    kill_candidates(ctx, seat, bottle).len() >= KILLS
}

pub fn pay_kills(ctx: &mut Ctx, picked: &[u32]) -> usize {
    let mut killed = 0;
    for card in picked {
        if ctx.kill(*card, Cause::Cost) == Killed::NotOnBoard {
            continue;
        }
        ctx.narrate(format!("{{card {card}}} is killed as a cost"));
        killed += 1;
    }
    killed
}

fn sacrifices(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    kill_candidates(ctx, item.controller, item.kind.source())
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn wish(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let bottle = item.kind.source();
    let seat = item.controller;
    if stage.0 != KILL {
        if !kill_cost_payable(ctx, seat, bottle) {
            ctx.narrate(format!(
                "{{card {bottle}}} · fewer than {KILLS} other friendly units and gear to kill"
            ));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, KILL, 0, KILLS as u8));
    }
    let offered = kill_candidates(ctx, seat, bottle);
    let picked: Vec<u32> = ctx
        .picks()
        .iter()
        .copied()
        .filter(|card| offered.contains(card))
        .collect();
    if picked.len() < KILLS {
        ctx.narrate(format!(
            "{{seat {seat}}} keeps their units and gear · no point from {{card {bottle}}}"
        ));
        return done();
    }
    if pay_kills(ctx, &picked) == KILLS {
        score_point(ctx, seat);
    }
    done()
}

pub static CARD: Card = gear(
    "Bottled Constellation",
    &[],
    &[asking(
        with_candidates(
            optional(triggered(Trigger::Reflexive, &[], wish)),
            sacrifices,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::queue_trigger;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{phases, prompts};
    use crate::state::{Phase, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const BOTTLE: u32 = 90;
    const ALLY: u32 = 91;
    const TRINKET: u32 = 92;
    const THEIR_TRINKET: u32 = 93;

    fn bottle(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            power: Some(2),
            domain: vec!["Mind".into()],
            ..fixtures::gear(BOTTLE, zone, seat, "Bottled Constellation", 10)
        }
    }

    fn observatory(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bottle(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Wailer", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture.table.cards.push(fixtures::gear(
            THEIR_TRINKET,
            fixtures::BASE,
            1,
            "Their Trinket",
            1,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BOTTLE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn wish_now(ctx: &mut Ctx) {
        queue_trigger(ctx, BOTTLE, WISH, TargetRef::Card(BOTTLE));
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_a_keywordless_gear_with_one_optional_trigger_that_asks_for_three_kills() {
        assert!(std::ptr::eq(
            script_of("Bottled Constellation").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let wish = &CARD.abilities[usize::from(WISH)];
        assert_eq!(
            wish.trigger,
            Trigger::Reflexive,
            "seam · no Trigger for the start of a Main Phase; the engine queues it"
        );
        assert!(wish.optional, "you may");
        assert!(wish.candidates.is_some());
        assert_eq!(wish.question, Some(QUESTION));
        assert!(wish.targets.is_empty());
        assert!(wish.cost.is_none());
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(KILLS, 3);
    }

    #[test]
    fn the_candidates_are_the_controllers_other_units_and_gear_and_the_bottle_lists_itself_in_play()
    {
        let mut fixture = observatory(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            kill_candidates(&ctx, 0, BOTTLE),
            [fixtures::VI, ALLY, TRINKET],
            "units and gear, never the bottle, never the enemy's"
        );
        assert!(kill_cost_payable(&ctx, 0, BOTTLE));
        assert_eq!(
            kill_candidates(&ctx, 1, BOTTLE),
            [fixtures::SPRITE, fixtures::THEIR_UNIT, THEIR_TRINKET]
        );
        assert_eq!(wishes_at_the_start_of_the_main_phase_of(&ctx, 0), [BOTTLE]);
        assert!(wishes_at_the_start_of_the_main_phase_of(&ctx, 1).is_empty());
        assert!(ctx.set_controller(BOTTLE, 1, fixtures::SPRITE));
        assert_eq!(wishes_at_the_start_of_the_main_phase_of(&ctx, 1), [BOTTLE]);
        assert!(wishes_at_the_start_of_the_main_phase_of(&ctx, 0).is_empty());
        drop(ctx);
        let mut fixture = observatory(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(wishes_at_the_start_of_the_main_phase_of(&ctx, 0).is_empty());
    }

    #[test]
    fn killing_three_others_scores_a_point() {
        let mut fixture = observatory(fixtures::BASE);
        let mut ctx = fixture.ctx();
        wish_now(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: KILL
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {ALLY}}}"),
                format!("{{card {TRINKET}}}"),
                "done".to_string(),
                "skip".to_string()
            ]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {BOTTLE}}}: choose {QUESTION} (0 of 3)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TRINKET}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_trash(fixtures::VI));
        assert!(ctx.in_trash(ALLY));
        assert!(ctx.in_trash(TRINKET));
        assert_eq!(ctx.location(BOTTLE), Some(Location::Base(0)));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(ctx.points(1), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} scores 1 point".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TRINKET}}} is killed as a cost")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_or_stopping_short_of_three_kills_nothing_and_scores_nothing() {
        let mut fixture = observatory(fixtures::BASE);
        let mut ctx = fixture.ctx();
        wish_now(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 0);
        assert!(ctx.on_board(fixtures::VI) && ctx.on_board(ALLY) && ctx.on_board(TRINKET));
        wish_now(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        assert!(fixtures::labels(&ctx).contains(&"done".to_string()));
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 0);
        assert!(
            ctx.on_board(ALLY),
            "one pick is not the cost: nothing is killed"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} keeps their units and gear · no point from {{card {BOTTLE}}}"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_fewer_than_three_others_the_wish_resolves_without_asking() {
        let mut fixture = observatory(fixtures::BASE);
        fixture.table.cards.retain(|card| card.id != TRINKET);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!kill_cost_payable(&ctx, 0, BOTTLE));
        wish_now(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 0);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BOTTLE}}} · fewer than 3 other friendly units and gear to kill"
        )));
    }

    #[test]
    fn today_the_main_phase_opens_without_the_wish() {
        let mut fixture = observatory(fixtures::BASE);
        fixture.blob.set_phase(Phase::Beginning);
        let mut ctx = fixture.ctx();
        assert_eq!(wishes_at_the_start_of_the_main_phase_of(&ctx, 0), [BOTTLE]);
        phases::continue_beginning(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(
            ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty(),
            "seam · no When::MainPhaseOf and no Main Phase event"
        );
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    #[ignore = "engine gap · delayed triggers and the pay stage: no When::MainPhaseOf(seat) for phases::continue_beginning to queue as the Action phase opens (316.4, the Iascylla row), so wishes_at_the_start_of_the_main_phase_of is consulted by nothing; and the three kills are a non-resource cost the M10 pay-stage row owes (the Commander Ledros row) — paid at resolution through a Resume pick until then"]
    fn the_wish_is_queued_as_your_main_phase_opens_and_the_kills_are_paid_before_it_resolves() {
        let mut fixture = observatory(fixtures::BASE);
        fixture.blob.set_phase(Phase::Beginning);
        let mut ctx = fixture.ctx();
        phases::continue_beginning(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        for card in [fixtures::VI, ALLY, TRINKET] {
            fixtures::choose(&mut ctx, 0, &format!("{{card {card}}}")).unwrap();
        }
        assert_eq!(ctx.points(0), 1);
    }
}
