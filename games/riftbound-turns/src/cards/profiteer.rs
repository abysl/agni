use super::prelude::{
    asking, card_target, disempower, done, friendly_gear, friendly_units, is_empowered, optional,
    play, target, unit, with_candidates,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const QUESTION: &str = "something you control to disempower";
pub const PICK_SOURCE: u8 = 1;
pub const EMPOWER_TARGET: TargetSpec = target(
    Filter::Or(&[Filter::Legend, Filter::Unit, Filter::Gear]),
    0,
    1,
    TargetKind::Card,
    "a legend, unit or gear to empower",
);

pub fn legend_can_be_empowered(ctx: &Ctx, legend: u32) -> bool {
    ctx.is_legend(legend) && ctx.in_play(legend)
}

fn legends(ctx: &Ctx) -> Vec<u32> {
    let mut legends: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .map(|card| card.id)
        .filter(|card| legend_can_be_empowered(ctx, *card))
        .collect();
    legends.sort_unstable();
    legends
}

pub fn empowered_things_you_control(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut things: Vec<u32> = friendly_units(ctx, seat)
        .into_iter()
        .chain(friendly_gear(ctx, seat))
        .chain(
            legends(ctx)
                .into_iter()
                .filter(|legend| ctx.controller(*legend) == seat),
        )
        .filter(|thing| is_empowered(ctx, *thing))
        .collect();
    things.sort_unstable();
    things
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != PICK_SOURCE {
        return Vec::new();
    }
    empowered_things_you_control(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn profit(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    let Some(target) = card_target(ctx, item, 0) else {
        ctx.narrate(format!("{{card {me}}} · {{seat {seat}}} empowers nothing"));
        return done();
    };
    if stage.0 != PICK_SOURCE {
        if empowered_things_you_control(ctx, seat).is_empty() {
            ctx.narrate(format!(
                "{{card {me}}} · {{seat {seat}}} controls nothing Empowered · nothing is empowered"
            ));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, PICK_SOURCE, 1, 1));
    }
    let offered = empowered_things_you_control(ctx, seat);
    let Some(source) = ctx
        .picks()
        .first()
        .copied()
        .filter(|pick| offered.contains(pick))
    else {
        ctx.narrate(format!(
            "{{card {me}}} · {{seat {seat}}} disempowers nothing · nothing is empowered"
        ));
        return done();
    };
    if !disempower(ctx, source) {
        ctx.narrate(format!(
            "{{card {source}}} is no longer Empowered · nothing is empowered"
        ));
        return done();
    }
    ctx.narrate(format!("{{card {source}}} is disempowered"));
    if ctx.empower_by(target, item.controller) {
        ctx.narrate(format!("{{card {target}}} is empowered"));
    } else {
        ctx.narrate(format!("{{card {target}}} was Empowered already"));
    }
    done()
}

pub static CARD: Card = unit(
    "Profiteer",
    &[],
    &[asking(
        with_candidates(optional(play(&[EMPOWER_TARGET], profit)), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{Expiry, ItemKind, PromptWhy};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const PROFITEER: u32 = 90;
    const MY_GEAR: u32 = 91;

    fn profiteer() -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Body".into()],
            ..fixtures::unit(PROFITEER, fixtures::HAND, 0, "Profiteer", 4)
        }
    }

    fn empowered_in_the_table(fixture: &mut Fixture, card: u32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(card),
            counter: COUNTER_EMPOWERED,
            value: 1,
        });
        fixture.table.counters.sort();
    }

    fn market(with_empowered: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(profiteer());
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        if with_empowered {
            empowered_in_the_table(&mut fixture, fixtures::VI);
            empowered_in_the_table(&mut fixture, fixtures::SPRITE);
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PROFITEER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn play_him(ctx: &mut Ctx) -> u16 {
        fixtures::play_from_hand(ctx, 0, PROFITEER).unwrap();
        let item = ctx
            .blob
            .queue
            .iter()
            .find(|pending| {
                matches!(pending.item.kind, ItemKind::Trigger { source, index: 0 } if source == PROFITEER)
            })
            .map(|pending| pending.item.id)
            .expect("the play trigger waits on its target");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        item
    }

    #[test]
    fn the_script_is_one_optional_play_trigger_aimed_at_a_unit_or_gear_that_asks_the_source_at_resolution(
    ) {
        assert!(std::ptr::eq(script_of("Profiteer").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let trade = &CARD.abilities[0];
        assert_eq!(trade.trigger, Trigger::Play);
        assert!(
            trade.optional,
            "383.3.a · the you may is decided at finalization"
        );
        assert_eq!(trade.targets, [EMPOWER_TARGET]);
        assert_eq!((EMPOWER_TARGET.min, EMPOWER_TARGET.max), (0, 1));
        assert!(trade.candidates.is_some());
        assert_eq!(trade.question, Some(QUESTION));
    }

    #[test]
    fn the_thing_to_empower_is_chosen_on_the_chain_and_the_disempower_is_paid_as_it_resolves() {
        let mut fixture = market(true);
        let mut ctx = fixture.ctx();
        assert_eq!(empowered_things_you_control(&ctx, 0), [fixtures::VI]);
        assert_eq!(empowered_things_you_control(&ctx, 1), [fixtures::SPRITE]);
        let item = play_him(&mut ctx);
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        let mut expected: Vec<String> = [
            fixtures::LEGEND_CARD,
            fixtures::VI,
            fixtures::SPRITE,
            fixtures::THEIR_UNIT,
            PROFITEER,
            MY_GEAR,
        ]
        .iter()
        .map(|card| format!("{{card {card}}}"))
        .collect();
        expected.push("skip".to_string());
        expected.sort();
        assert_eq!(
            offered, expected,
            "every legend, unit and gear in play, yours or theirs, or nothing"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.is_empowered(fixtures::VI), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: PICK_SOURCE
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI)],
            "the enemy's Empowered Sprite is not yours to disempower · no skip once the may was taken"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_empowered(fixtures::VI));
        assert!(ctx.is_empowered(fixtures::THEIR_UNIT));
        assert!(ctx
            .events
            .contains(&Event::Disempowered { card: fixtures::VI }));
        assert!(ctx.events.contains(&Event::Empowered {
            card: fixtures::THEIR_UNIT,
            by: 0
        }));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_deflect_enemy_taxes_the_choice_on_the_chain() {
        let mut fixture = market(true);
        fixture
            .blob
            .card_state_mut(fixtures::THEIR_UNIT)
            .granted
            .push((crate::cards::Keyword::Deflect(5), Expiry::Permanent));
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1 || (40..44).contains(&card.id));
        for rune in 40..44 {
            fixture.table.card_mut(rune).unwrap().exhausted = false;
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(fixtures::THEIR_UNIT), 5);
        assert_eq!(ctx.runes_of(0).len(), 4);
        play_him(&mut ctx);
        assert!(
            !fixtures::labels(&ctx).contains(&format!("{{card {}}}", fixtures::THEIR_UNIT)),
            "four runes cannot pay a Deflect of five"
        );
        assert!(fixtures::labels(&ctx).contains(&format!("{{card {}}}", fixtures::SPRITE)));
    }

    #[test]
    fn skipping_the_target_empowers_nothing_and_nothing_empowered_pays_nothing() {
        let mut fixture = market(true);
        let mut ctx = fixture.ctx();
        play_him(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.is_empowered(fixtures::VI));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {PROFITEER}}} · {{seat 0}} empowers nothing"
        )));
        drop(ctx);

        let mut fixture = market(false);
        let mut ctx = fixture.ctx();
        play_him(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {PROFITEER}}} · {{seat 0}} controls nothing Empowered · nothing is empowered"
        )));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Empowered { .. })));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_source_disempowered_in_response_leaves_nothing_to_pay_with() {
        let mut fixture = market(true);
        let mut ctx = fixture.ctx();
        play_him(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.disempower(fixtures::VI));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.is_empowered(fixtures::SPRITE), "as it was");
        assert!(!ctx.events.iter().any(
            |event| matches!(event, Event::Empowered { card, .. } if *card == fixtures::SPRITE)
        ));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {PROFITEER}}} · {{seat 0}} controls nothing Empowered · nothing is empowered"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn your_legend_is_offered_to_empower_and_becomes_empowered() {
        let mut fixture = market(true);
        let mut ctx = fixture.ctx();
        assert!(legend_can_be_empowered(&ctx, fixtures::LEGEND_CARD));
        play_him(&mut ctx);
        assert!(fixtures::labels(&ctx).contains(&format!("{{card {}}}", fixtures::LEGEND_CARD)));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::LEGEND_CARD)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.is_empowered(fixtures::LEGEND_CARD));
    }

    #[test]
    #[ignore = "engine gap · non-resource costs at the pay stage (383.3.b): the disempower of something you control is the cost within instructions after the you may, so it is picked and paid as the trigger is finalized; today it is a Resume pick paid at resolution (profiteer::empowered_things_you_control is the candidate list)"]
    fn the_disempower_is_picked_and_paid_as_the_trigger_is_finalized() {
        let mut fixture = market(true);
        let mut ctx = fixture.ctx();
        play_him(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(!ctx.is_empowered(fixtures::VI), "paid before the chain");
        assert_eq!(ctx.blob.chain.len(), 1);
    }
}
