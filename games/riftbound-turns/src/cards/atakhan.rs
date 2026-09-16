use super::prelude::{
    asking, done, friendly_units, on_attack, remember_card, remembered_cards, unit, with_candidates,
};
use super::{Card, Domain, Flow, Item, Keyword, Stage};
use crate::engine::cost::{Cost, Need};
use crate::engine::ctx::{Cause, Ctx, Killed, Location};
use crate::engine::kill;
use crate::state::TargetRef;

const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str = "one of your units here to kill";

pub fn kill_candidates(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut units = friendly_units(ctx, seat);
    units.sort_unstable();
    units
}

pub fn discount_for(ctx: &Ctx, killed: u32) -> Cost {
    let Some(face) = ctx.card(killed) else {
        return Cost::free();
    };
    Cost {
        energy: face.energy.unwrap_or(0),
        power: vec![Need::Domain(Domain::Order); usize::from(face.power.unwrap_or(0))],
        ..Cost::free()
    }
}

pub fn pay_kill(ctx: &mut Ctx, unit: u32) -> Killed {
    let killed = ctx.kill(unit, Cause::Cost);
    if killed != Killed::NotOnBoard {
        ctx.narrate(format!("{{card {unit}}} is killed as an additional cost"));
    }
    killed
}

pub fn defending_seat(ctx: &Ctx, me: u32) -> Option<u8> {
    let Some(Location::Battlefield(zone)) = ctx.location(me) else {
        return None;
    };
    let seat = ctx.controller(me);
    if let Some(showdown) = ctx.blob.showdown.as_ref() {
        if showdown.zone == zone && showdown.attacker == seat {
            return Some(showdown.defender);
        }
    }
    ctx.blob.holder(zone).filter(|holder| *holder != seat)
}

pub fn defenders_units_here(ctx: &Ctx, me: u32) -> Vec<u32> {
    let (Some(here), Some(defender)) = (ctx.location(me), defending_seat(ctx, me)) else {
        return Vec::new();
    };
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) == defender && !ctx.is_facedown(*unit))
        .collect()
}

fn their_units_here(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    defenders_units_here(ctx, item.kind.source())
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn slaughter(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    if stage.0 == STAGE_PICKED {
        let offered = defenders_units_here(ctx, me);
        let Some(unit) = ctx
            .picks()
            .first()
            .copied()
            .filter(|unit| offered.contains(unit))
        else {
            return done();
        };
        let Some(defender) = defending_seat(ctx, me) else {
            return done();
        };
        remember_card(ctx, unit);
        ctx.narrate(format!("{{seat {defender}}} chooses {{card {unit}}}"));
        kill::batch(ctx, &[unit], Cause::Item(item.id));
        return done();
    }
    if !remembered_cards(item).is_empty() {
        return done();
    }
    let Some(defender) = defending_seat(ctx, me) else {
        ctx.narrate(format!("{{card {me}}}: nobody defends here"));
        return done();
    };
    let theirs = defenders_units_here(ctx, me);
    match theirs.as_slice() {
        [] => {
            ctx.narrate(format!("{{seat {defender}}} has no unit here to kill"));
            done()
        }
        [only] => {
            let only = *only;
            ctx.narrate(format!("{{seat {defender}}} must kill {{card {only}}}"));
            kill::batch(ctx, &[only], Cause::Item(item.id));
            done()
        }
        _ => Flow::Ask(ctx.ask_seat_resume(item, defender, STAGE_PICKED, 1, 1)),
    }
}

pub static CARD: Card = unit(
    "Atakhan",
    &[Keyword::Ganking],
    &[asking(
        with_candidates(on_attack(&[], slaughter), their_units_here),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::cost;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, prompts, triggers};
    use crate::state::{ItemKind, PromptWhy, Showdown};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const ATAKHAN: u32 = 90;
    const GUARD: u32 = 91;
    const SENTRY: u32 = 92;
    const MY_ESCORT: u32 = 93;
    const ENERGY: u8 = 10;
    const MIGHT: u8 = 7;

    fn atakhan(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(3),
            domain: vec!["Order".into()],
            ..fixtures::unit(ATAKHAN, zone, seat, "Atakhan", MIGHT)
        }
    }

    fn pit(defenders: &[u32]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(atakhan(fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(MY_ESCORT, fixtures::BF1, 0, "Escort", 2));
        for (id, name) in [(GUARD, "Guard"), (SENTRY, "Sentry")] {
            if defenders.contains(&id) {
                fixture
                    .table
                    .cards
                    .push(fixtures::unit(id, fixtures::BF1, 1, name, 3));
            }
        }
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ATAKHAN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: ATAKHAN });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == ATAKHAN
        ));
    }

    #[test]
    fn the_script_prints_ganking_and_one_untargeted_attack_trigger_the_defender_answers() {
        assert!(std::ptr::eq(script_of("Atakhan").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Ganking]);
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is a rune cost; his is a kill with a discount read off the victim"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
    }

    #[test]
    fn the_kill_cost_seams_read_the_victims_energy_and_power_and_kill_it_as_a_cost() {
        let mut fixture = pit(&[GUARD]);
        fixture.table.cards.push(CardInfo {
            energy: Some(4),
            power: Some(2),
            ..fixtures::unit(94, fixtures::BASE, 0, "Offering", 3)
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            kill_candidates(&ctx, 0),
            [fixtures::VI, ATAKHAN, MY_ESCORT, 94]
        );
        assert_eq!(
            kill_candidates(&ctx, 1),
            [fixtures::SPRITE, fixtures::THEIR_UNIT, GUARD]
        );
        let discount = discount_for(&ctx, 94);
        assert_eq!(discount.energy, 4);
        assert_eq!(discount.power, vec![Need::Domain(Domain::Order); 2]);
        let printed = cost::printed_of(&ctx, ATAKHAN, false);
        assert_eq!(printed.energy, 10);
        assert_eq!(printed.power, vec![Need::Domain(Domain::Order); 3]);
        let reduced = printed.less(&discount);
        assert_eq!(reduced.energy, 6);
        assert_eq!(reduced.power, vec![Need::Domain(Domain::Order); 1]);
        assert_eq!(
            discount_for(&ctx, fixtures::SPRITE).energy,
            0,
            "a token costs nothing"
        );
        assert_eq!(pay_kill(&mut ctx, 94), Killed::Yes);
        assert_eq!(ctx.card(94).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(pay_kill(&mut ctx, 94), Killed::NotOnBoard);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 94} is killed as an additional cost".to_string()));
    }

    #[test]
    fn attacking_makes_the_defender_pick_one_of_their_units_here_and_it_dies() {
        let mut fixture = pit(&[GUARD, SENTRY]);
        let mut ctx = fixture.ctx();
        assert_eq!(defending_seat(&ctx, ATAKHAN), Some(1));
        assert_eq!(defenders_units_here(&ctx, ATAKHAN), [GUARD, SENTRY]);
        attacks(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: STAGE_PICKED
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (1, 1, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {GUARD}}}"), format!("{{card {SENTRY}}}")],
            "the defender's units here · not the Escort, not Jinx in base"
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Resume {
                    item,
                    stage: STAGE_PICKED
                }
            ),
            format!("{{seat 1}}: choose {QUESTION} (0 of 1)")
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 })),
            "the attacker does not choose for the defender"
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {SENTRY}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.on_board(SENTRY));
        assert!(ctx.on_board(GUARD));
        assert!(ctx.on_board(MY_ESCORT));
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, controller: 1, unit: true, .. } if *card == SENTRY)
        ));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 1}} chooses {{card {SENTRY}}}")));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_lone_defender_dies_unasked_and_nobody_here_means_nothing_to_kill() {
        let mut fixture = pit(&[GUARD]);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "one unit: nothing to choose");
        assert!(!ctx.on_board(GUARD));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 1}} must kill {{card {GUARD}}}")));
        assert!(ctx.on_board(MY_ESCORT));
        drop(ctx);
        let mut empty = pit(&[]);
        let mut ctx = empty.ctx();
        assert!(defenders_units_here(&ctx, ATAKHAN).is_empty());
        attacks(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no unit here to kill".to_string()));
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "Jinx in her base is not here"
        );
        assert!(ctx.on_board(MY_ESCORT));
    }

    #[test]
    fn the_defender_is_read_off_the_showdown_and_off_the_holder_without_one() {
        let mut fixture = pit(&[GUARD]);
        let ctx = fixture.ctx();
        assert_eq!(defending_seat(&ctx, ATAKHAN), Some(1), "the holder defends");
        drop(ctx);
        let mut fought = pit(&[GUARD]);
        fought.blob.set_holder(fixtures::BF1, None);
        fought.blob.showdown = Some(Showdown::open(fixtures::BF1, 0, 1));
        let ctx = fought.ctx();
        assert_eq!(
            defending_seat(&ctx, ATAKHAN),
            Some(1),
            "the open showdown names the defender"
        );
        drop(ctx);
        let mut elsewhere = pit(&[GUARD]);
        elsewhere.blob.set_holder(fixtures::BF1, None);
        elsewhere.blob.showdown = Some(Showdown::open(fixtures::BF2, 1, 0));
        let ctx = elsewhere.ctx();
        assert_eq!(
            defending_seat(&ctx, ATAKHAN),
            None,
            "a showdown elsewhere is not his"
        );
        drop(ctx);
        let mut mine = pit(&[GUARD]);
        mine.blob.set_holder(fixtures::BF1, Some(0));
        let ctx = mine.ctx();
        assert_eq!(
            defending_seat(&ctx, ATAKHAN),
            None,
            "holding it himself, nobody defends against him"
        );
        drop(ctx);
        let mut home = pit(&[GUARD]);
        home.table.card_mut(ATAKHAN).unwrap().zone = Some(fixtures::BASE);
        home.resolve();
        let ctx = home.ctx();
        assert_eq!(
            defending_seat(&ctx, ATAKHAN),
            None,
            "in base he attacks nowhere"
        );
    }

    #[test]
    #[ignore = "engine gap · an optional kill-a-friendly-unit additional cost at the pay stage with a discount read off the victim's printed costs: Card.additional is a fixed optional rune cost and cannot hold a kill (the Cruel Patron and Commander Ledros row)"]
    fn playing_him_may_kill_a_friendly_unit_and_its_costs_come_off_his() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(atakhan(fixtures::HAND, 0));
        fixture.table.cards.push(CardInfo {
            energy: Some(6),
            power: Some(2),
            ..fixtures::unit(94, fixtures::BASE, 0, "Offering", 3)
        });
        for id in [46, 47] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Order", false));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 5);
        fixtures::play_from_hand(&mut ctx, 0, ATAKHAN).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "which friendly unit dies, if any: {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, "{card 94}").unwrap();
        assert_eq!(ctx.card(94).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.location(ATAKHAN), Some(Location::Base(0)));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "four energy and one Order after six and two came off"
        );
    }
}
