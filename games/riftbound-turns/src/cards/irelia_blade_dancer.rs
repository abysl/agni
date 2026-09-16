use super::prelude::{
    chosen_unit, done, exhausting_self, legend, on_conquer, on_friendly_unit_chosen, optional,
    ready, when, with_cost, ONE_ENERGY, RAINBOW,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

fn chosen_unit_is_exhausted(ctx: &Ctx, event: &Event, _: Source) -> bool {
    matches!(
        event,
        Event::Chosen { card, .. } if ctx.card(*card).is_some_and(|held| held.exhausted)
    )
}

fn she_is_exhausted(ctx: &Ctx, _: &Event, source: Source) -> bool {
    ctx.card(source.card).is_some_and(|held| held.exhausted)
}

fn ready_the_chosen_unit(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = chosen_unit(item) {
        if ctx.on_board(unit) && ready(ctx, unit) {
            ctx.narrate(format!(
                "{{card {}}} readies {{card {unit}}}",
                item.kind.source()
            ));
        }
    }
    done()
}

fn ready_herself(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    done()
}

pub static CARD: Card = legend(
    "Irelia - Blade Dancer",
    &[],
    &[
        when(
            optional(exhausting_self(with_cost(
                on_friendly_unit_chosen(&[], ready_the_chosen_unit),
                RAINBOW,
            ))),
            chosen_unit_is_exhausted,
        ),
        when(
            optional(with_cost(on_conquer(&[], ready_herself), ONE_ENERGY)),
            she_is_exhausted,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{prelude, Filter, SelfCost, Trigger, Who};
    use crate::engine::ctx::{EntryMove, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, legal, play as play_engine, priority, prompts, resume, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::Target;

    const DISCIPLINE: u32 = 90;
    const LEGEND: u32 = fixtures::LEGEND_CARD;
    const TINKER: u32 = 91;
    const OWN_GEAR: u32 = 92;

    static TINKERER: Card = prelude::spell(
        "Tinkerer",
        &[],
        &[prelude::play(
            &[prelude::a_card(Filter::Gear, "a gear")],
            |_, _, _| Flow::Done,
        )],
    );

    fn dojo() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(LEGEND).unwrap().name = "Irelia - Blade Dancer".into();
        let mut discipline = fixtures::spell(DISCIPLINE, fixtures::HAND, 0, "Discipline", 2, 0);
        discipline.domain = vec!["Calm".into()];
        fixture.table.cards.push(discipline);
        fixture.resolve();
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

    fn play_from_hand(ctx: &mut Ctx, card: u32) {
        let entry = EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        legal::classify(ctx, 0, &entry).unwrap();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), 0)
            .unwrap();
        play_engine::begin(ctx, 0, card, Origin::Hand, None).unwrap();
        settle(ctx).unwrap();
    }

    fn play_discipline(ctx: &mut Ctx) {
        play_from_hand(ctx, DISCIPLINE)
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
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

    fn readied(ctx: &Ctx, card: u32) -> bool {
        ctx.events
            .iter()
            .any(|event| matches!(event, Event::Readied { card: held, .. } if *held == card))
    }

    #[test]
    fn the_legend_has_two_paid_may_triggers_and_only_the_first_exhausts_her() {
        let fixture = dojo();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LEGEND).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Irelia - Blade Dancer");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let chosen = &CARD.abilities[0];
        assert_eq!(chosen.trigger, Trigger::ChosenFriendly(Who::Friendly));
        assert!(chosen.optional);
        assert_eq!(chosen.cost, Some(RAINBOW));
        assert_eq!(chosen.self_cost, SelfCost::Exhaust);
        assert!(chosen.condition.is_some());
        assert!(chosen.targets.is_empty());
        let conquer = &CARD.abilities[1];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::You));
        assert!(conquer.optional);
        assert_eq!(conquer.cost, Some(ONE_ENERGY));
        assert_eq!(conquer.self_cost, SelfCost::Auto);
        assert!(conquer.condition.is_some());
        assert!(conquer.targets.is_empty());
    }

    #[test]
    fn choosing_an_exhausted_friendly_unit_asks_for_the_rune_and_exhausts_her_to_ready_it() {
        let mut fixture = dojo();
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        play_discipline(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 0, .. } if *card == fixtures::VI
        )));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 2, cost: 1 }),
            "one trigger needs no ordering · 392.2 asks for the cost"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "pay 1 any power for the {{card {LEGEND}}} trigger · {{card {}}}?",
                fixtures::VI
            ),
            "the confirm names the unit she would ready"
        );
        assert_eq!(labels(&ctx), ["yes", "no"]);
        assert!(!ctx.card(LEGEND).unwrap().exhausted);
        let recycled_before = recycled(&ctx).len();
        choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(LEGEND).unwrap().exhausted,
            "exhausting her is part of the cost"
        );
        assert_eq!(
            recycled(&ctx).len(),
            recycled_before + 1,
            "the rainbow recycles one rune"
        );
        assert_eq!(ctx.blob.chain.len(), 2, "the spell and her trigger");
        assert!(matches!(
            ctx.blob.chain[1].kind,
            ItemKind::Trigger { source, index: 0 } if source == LEGEND
        ));
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "nothing until it resolves"
        );
        resolve_top(&mut ctx);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(readied(&ctx, fixtures::VI));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {LEGEND}}} readies {{card {}}}",
            fixtures::VI
        )));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "Vi has no Readied trigger of her own"
        );
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, fixtures::VI), 2);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_rune_removes_the_trigger_and_leaves_both_of_them_as_they_were() {
        let mut fixture = dojo();
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        play_discipline(&mut ctx);
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        let recycled_before = recycled(&ctx).len();
        choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1, "only the spell");
        assert!(!ctx.card(LEGEND).unwrap().exhausted);
        assert_eq!(recycled(&ctx).len(), recycled_before);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {LEGEND}}} trigger is removed · its cost is declined"
        )));
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "Discipline does not ready"
        );
        assert!(!readied(&ctx, fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
    }

    #[test]
    fn conquering_lets_her_controller_pay_one_energy_to_ready_her() {
        let mut fixture = dojo();
        fixture.table.card_mut(LEGEND).unwrap().exhausted = true;
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        assert_eq!(ctx.points(0), 1);
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "pay 1 energy for the {{card {LEGEND}}} trigger · {{zone {}}}?",
                fixtures::BF1
            )
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: 1,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the may is her controller's · seat 1 cannot answer it"
        );
        choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.effects.contains(&Effect::exhaust(41)),
            "one ready rune pays the energy: {:?}",
            ctx.effects
        );
        assert!(recycled(&ctx).is_empty(), "no power is asked");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == LEGEND
        ));
        assert!(
            ctx.card(LEGEND).unwrap().exhausted,
            "nothing until it resolves"
        );
        resolve_top(&mut ctx);
        assert!(!ctx.card(LEGEND).unwrap().exhausted);
        assert!(readied(&ctx, LEGEND));
        assert!(ctx.blob.log.contains(&format!("{{card {LEGEND}}} readies")));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_legend_has_no_conquer_trigger_and_no_ready_rune_removes_it_before_asking() {
        let mut fixture = dojo();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "she is ready · nothing to pay for"
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        let mut broke = dojo();
        broke.table.card_mut(LEGEND).unwrap().exhausted = true;
        broke.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        broke.blob.set_contested(fixtures::BF1, Some(0));
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = broke.ctx();
        cleanup::establish(&mut ctx, fixtures::BF1);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "392.2 · a cost that cannot be paid is not offered"
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "no rune to exhaust · the trigger is removed"
        );
        assert!(ctx.card(LEGEND).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {LEGEND}}} trigger is removed · its cost can't be paid"
        )));
    }

    #[test]
    fn choosing_an_exhausted_friendly_gear_raises_no_trigger() {
        let mut fixture = dojo();
        let mut tinker = fixtures::spell(TINKER, fixtures::HAND, 0, "Tinkerer", 1, 0);
        tinker.domain = vec!["Calm".into()];
        fixture.table.cards.push(tinker);
        let mut gear = fixtures::gear(OWN_GEAR, fixtures::BASE, 0, "Sturdy Boots", 1);
        gear.exhausted = true;
        fixture.table.cards.push(gear);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(TINKER, &TINKERER);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, TINKER);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        choose(&mut ctx, 0, &format!("{{card {OWN_GEAR}}}")).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 0, .. } if *card == OWN_GEAR
        )));
        assert!(
            ctx.blob.prompt.is_none(),
            "a friendly gear is not a friendly unit: {:?}",
            ctx.blob.why
        );
        assert_eq!(ctx.blob.chain.len(), 1, "only the spell");
        assert!(ctx.blob.queue.is_empty());
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("trigger is removed")));
    }

    #[test]
    fn an_exhausted_legend_or_a_ready_target_gives_no_choose_trigger_at_all() {
        let mut fixture = dojo();
        fixture.table.card_mut(LEGEND).unwrap().exhausted = true;
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        play_discipline(&mut ctx);
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no cost confirm for a cost she cannot pay"
        );
        assert_eq!(ctx.blob.chain.len(), 1, "only the spell");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {LEGEND}}} trigger is removed · its source is exhausted"
        )));
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        play_discipline(&mut ctx);
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.prompt.is_none(), "Vi is ready · nothing to ready");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.card(LEGEND).unwrap().exhausted);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("trigger is removed")));
        let mut theirs = dojo();
        theirs
            .table
            .card_mut(fixtures::THEIR_UNIT)
            .unwrap()
            .exhausted = true;
        let mut ctx = theirs.ctx();
        play_discipline(&mut ctx);
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "an enemy unit is not a friendly one"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
    }
}
