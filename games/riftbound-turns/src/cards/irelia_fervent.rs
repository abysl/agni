use super::prelude::{done, might_this_turn, on_chosen, on_readied, unit, when};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

const MIGHT: i16 = 1;

fn chosen_by_her_controller(ctx: &Ctx, event: &Event, source: Source) -> bool {
    matches!(event, Event::Chosen { by, .. } if *by == ctx.controller(source.card))
}

fn grow(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ctx.on_board(me) {
        might_this_turn(ctx, item, me, MIGHT, None);
        ctx.narrate(format!("{{card {me}}} gets +{MIGHT} might this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Irelia - Fervent",
    &[Keyword::Deflect(1)],
    &[
        when(on_chosen(&[], grow), chosen_by_her_controller),
        on_readied(&[], grow),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{an_enemy_unit, play, spell};
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::{EntryMove, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, phases, play as play_engine, priority, prompts, resume, settle};
    use crate::state::{Expiry, ItemKind, Origin, Phase, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const FERVENT: u32 = 90;
    const DISCIPLINE: u32 = 91;
    const POKE: u32 = 92;
    const LEGEND: u32 = fixtures::LEGEND_CARD;
    const PRINTED: u8 = 4;

    static POKE_CARD: Card = spell(
        "Poke",
        &[Keyword::Reaction],
        &[play(&[an_enemy_unit("an enemy unit")], |_, _, _| done())],
    );

    fn fervent(exhausted: bool) -> CardInfo {
        let mut card = fixtures::unit(FERVENT, fixtures::BASE, 0, "Irelia - Fervent", PRINTED);
        card.energy = Some(5);
        card.domain = vec!["Calm".into()];
        card.exhausted = exhausted;
        card
    }

    fn dojo(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(LEGEND).unwrap().name = "Irelia - Blade Dancer".into();
        fixture.table.cards.push(fervent(exhausted));
        let mut discipline = fixtures::spell(DISCIPLINE, fixtures::HAND, 0, "Discipline", 2, 0);
        discipline.domain = vec!["Calm".into()];
        fixture.table.cards.push(discipline);
        let mut poke = fixtures::spell(POKE, fixtures::HAND, 1, "Poke", 1, 1);
        poke.domain = vec!["Mind".into()];
        fixture.table.cards.push(poke);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(POKE, &POKE_CARD);
        fixture
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        let prompt = ctx.blob.prompt.as_ref().map(|p| p.id).unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)
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

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play_engine::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn resolve_top(ctx: &mut Ctx) {
        let first = priority::holder(ctx).unwrap();
        priority::pass(ctx, first).unwrap();
        priority::pass(ctx, 1 - first).unwrap();
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn deltas(ctx: &Ctx) -> Vec<i16> {
        ctx.state_of(FERVENT)
            .map(|row| row.might.iter().map(|held| held.delta).collect())
            .unwrap_or_default()
    }

    fn her_triggers(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, index } if source == FERVENT => Some(index),
                _ => None,
            })
            .collect()
    }

    fn recycled(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card,
                    zone,
                    index: BOTTOM,
                    ..
                } if Some(*zone) == ctx.zones.rune_deck => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn she_deflects_one_and_grows_on_her_own_choose_or_any_ready() {
        let fixture = dojo(false);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FERVENT).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Irelia - Fervent");
        assert_eq!(CARD.keywords, [Keyword::Deflect(1)]);
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Chosen);
        assert!(CARD.abilities[0].condition.is_some());
        assert_eq!(CARD.abilities[1].trigger, Trigger::Readied(Who::Me));
        assert!(CARD.abilities[1].condition.is_none());
        for ability in CARD.abilities {
            assert!(ability.targets.is_empty());
            assert!(ability.cost.is_none());
            assert!(!ability.optional);
        }
    }

    #[test]
    fn the_legend_readies_a_discipline_target_and_fervent_gains_one_twice_on_a_self_choose() {
        let mut fixture = dojo(true);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, DISCIPLINE).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert!(
            labels(&ctx).contains(&format!("{{card {FERVENT}}}")),
            "no Deflect against her own controller"
        );
        choose(&mut ctx, 0, &format!("{{card {FERVENT}}}")).unwrap();
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| [41, 42, 43]
                    .iter()
                    .any(|rune| **effect == Effect::exhaust(*rune)))
                .count(),
            2,
            "two energy and nothing more"
        );
        assert!(
            recycled(&ctx).is_empty(),
            "no Deflect against her own controller"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 0, .. } if *card == FERVENT
        )));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "her Chosen and the legend's ChosenFriendly form one batch"
        );
        choose(&mut ctx, 0, &format!("{{card {FERVENT}}} trigger")).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { cost: 1, .. })),
            "392.2 · the legend's may is the cost confirm"
        );
        choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(LEGEND).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 3, "the spell and both triggers");
        assert_eq!(her_triggers(&ctx), [0]);
        resolve_top(&mut ctx);
        assert!(
            !ctx.card(FERVENT).unwrap().exhausted,
            "the legend's trigger readies the chosen unit"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, .. } if *card == FERVENT
        )));
        assert_eq!(
            her_triggers(&ctx),
            [0, 1],
            "Readied queues her second trigger onto the chain"
        );
        assert_eq!(ctx.blob.chain.len(), 3);
        resolve_top(&mut ctx);
        assert_eq!(deltas(&ctx), [1], "+1 when readied resolves first");
        assert_eq!(ctx.current_might(FERVENT), 5);
        resolve_top(&mut ctx);
        assert_eq!(deltas(&ctx), [1, 1], "+1 when chosen");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deltas(&ctx), [1, 1, 2], "then +2 from Discipline");
        assert_eq!(ctx.current_might(FERVENT), 8);
        assert_eq!(might_counter(&ctx, FERVENT), 4);
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| *line == &format!("{{card {FERVENT}}} gets +1 might this turn"))
                .count(),
            2
        );
        let turn = ctx.turn();
        for held in &ctx.state_of(FERVENT).unwrap().might {
            assert_eq!(held.until, Expiry::EndOfTurn(turn));
        }
        ctx.expire(Expiry::EndOfTurn(turn));
        assert_eq!(ctx.current_might(FERVENT), i32::from(PRINTED));
        assert_eq!(might_counter(&ctx, FERVENT), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_awaken_step_readies_her_and_that_is_her_controller_readying_her() {
        let mut fixture = dojo(true);
        fixture.blob.core_mut().unwrap().turn = 2;
        fixture.blob.core_mut().unwrap().player = 0;
        let mut ctx = fixture.ctx();
        ctx.actor = 1;
        phases::start_turn(&mut ctx);
        assert!(ctx.effects.contains(&Effect::ready(FERVENT)));
        assert!(
            ctx.events.contains(&Event::Readied {
                card: FERVENT,
                by: 0
            }),
            "315.1.a / 402.3.a · the turn player readies her"
        );
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert_eq!(her_triggers(&ctx), [1], "queued at the Beginning phase");
        assert_eq!(ctx.current_might(FERVENT), i32::from(PRINTED));
        resolve_top(&mut ctx);
        assert_eq!(deltas(&ctx), [1]);
        assert_eq!(ctx.current_might(FERVENT), i32::from(PRINTED) + 1);
        assert_eq!(
            ctx.blob.phase(),
            Some(Phase::Action),
            "the turn then continues"
        );
        assert!(ctx.fault.is_none());
        let mut theirs = dojo(true);
        theirs.blob.core_mut().unwrap().turn = 2;
        theirs.blob.core_mut().unwrap().player = 1;
        let mut ctx = theirs.ctx();
        phases::start_turn(&mut ctx);
        assert!(
            !ctx.effects.contains(&Effect::ready(FERVENT)),
            "the opponent's Awaken readies only their own objects"
        );
        assert!(her_triggers(&ctx).is_empty());
        assert!(ctx.card(FERVENT).unwrap().exhausted);
    }

    #[test]
    fn a_ready_that_changes_nothing_raises_nothing_and_a_real_one_grows_her_once() {
        let mut fixture = dojo(false);
        let mut ctx = fixture.ctx();
        assert!(!ctx.ready(FERVENT));
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "already ready · no Readied");
        assert!(deltas(&ctx).is_empty());
        let mut fixture = dojo(true);
        let mut ctx = fixture.ctx();
        assert!(ctx.ready(FERVENT));
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "one free trigger asks nothing");
        assert_eq!(her_triggers(&ctx), [1]);
        assert_eq!(ctx.current_might(FERVENT), i32::from(PRINTED));
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deltas(&ctx), [1]);
        assert_eq!(ctx.current_might(FERVENT), i32::from(PRINTED) + 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_opponent_pays_a_rainbow_to_choose_her_and_she_does_not_grow_for_them() {
        let mut fixture = dojo(false);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, POKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {FERVENT}}}"),
                "cancel".into()
            ]
        );
        let recycled_before = recycled(&ctx).len();
        choose(&mut ctx, 1, &format!("{{card {FERVENT}}}")).unwrap();
        assert!(ctx.effects.contains(&Effect::exhaust(44)), "one energy");
        assert_eq!(
            recycled(&ctx).len(),
            recycled_before + 2,
            "the printed power and the Deflect rainbow"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 1, .. } if *card == FERVENT
        )));
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "no trigger for a choose by the enemy"
        );
        assert!(her_triggers(&ctx).is_empty());
        assert!(
            !ctx.card(LEGEND).unwrap().exhausted,
            "nor for the legend · seat 1 did not choose a unit it controls"
        );
        resolve_top(&mut ctx);
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(deltas(&ctx).is_empty());
        assert_eq!(ctx.current_might(FERVENT), i32::from(PRINTED));
    }

    #[test]
    fn an_opponent_short_of_the_deflect_rune_is_refused_her() {
        let mut fixture = dojo(false);
        fixture.table.cards.retain(|card| card.id != 45);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(POKE, &POKE_CARD);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, POKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "cancel".into()],
            "one rune pays the spell but not the Deflect"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 2, 0, &[FERVENT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        choose(&mut ctx, 1, "cancel").unwrap();
        assert_eq!(ctx.card(POKE).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(deltas(&ctx).is_empty());
    }
}
