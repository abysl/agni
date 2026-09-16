use super::prelude::{activated, named, paying_with, unit, usable_if, with_statics};
use super::{Card, Cost, Flow, Grant, Item, Keyword, SelfCost, Source, Stage, Static, Timing};
use crate::engine::ctx::{Ctx, Event, COUNTER_EMPOWERED};
use agni_plugin_sdk::decide::Effect;
use agni_plugin_sdk::table::Target;

pub const EMPOWER: Cost = Cost {
    energy: 3,
    power: &[],
};
pub const TIMES: i32 = 3;
pub const MIGHT_PER_TIME: i16 = 2;
pub const DEFLECT: u8 = 3;
pub const EMPOWER_ABILITY: u8 = 0;

pub fn times_empowered(ctx: &Ctx, me: u32) -> i32 {
    ctx.table
        .counter(Target::Card(me), COUNTER_EMPOWERED)
        .unwrap_or(0)
        .max(0)
}

pub fn counter_room(ctx: &Ctx, me: u32) -> i32 {
    ctx.counter_room(me, COUNTER_EMPOWERED)
}

pub fn can_be_empowered_again(ctx: &Ctx, source: Source) -> bool {
    counter_room(ctx, source.card) > 0
}

fn empowered(ctx: &Ctx, me: u32) -> bool {
    times_empowered(ctx, me) >= 1
}

fn empowered_twice(ctx: &Ctx, me: u32, _: u32) -> bool {
    times_empowered(ctx, me) >= 2
}

fn empowered_thrice(ctx: &Ctx, me: u32, _: u32) -> bool {
    times_empowered(ctx, me) >= TIMES
}

fn empowered_three_times(ctx: &Ctx, me: u32) -> bool {
    empowered_thrice(ctx, me, me)
}

pub fn empower_once_more(ctx: &mut Ctx, me: u32) -> bool {
    let by = ctx.controller(me);
    empower_once_more_by(ctx, me, by)
}

pub fn empower_once_more_by(ctx: &mut Ctx, me: u32, by: u8) -> bool {
    if !ctx.on_board(me) || times_empowered(ctx, me) >= TIMES || counter_room(ctx, me) == 0 {
        return false;
    }
    if times_empowered(ctx, me) == 0 {
        return ctx.empower_by(me, by);
    }
    ctx.emit(Effect::Counter {
        target: Target::Card(me),
        counter: COUNTER_EMPOWERED,
        delta: 1,
    });
    ctx.raise(Event::Empowered { card: me, by });
    true
}

fn ascend(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if empower_once_more_by(ctx, me, item.controller) {
        let times = times_empowered(ctx, me);
        ctx.narrate(format!(
            "{{card {me}}} is Empowered {times} of {TIMES} times"
        ));
    }
    Flow::Done
}

pub static CARD: Card = with_statics(
    unit(
        "Kayle, Justified",
        &[Keyword::Empower(EMPOWER)],
        &[usable_if(
            named(
                paying_with(
                    activated(Timing::Sorcery, EMPOWER, &[], ascend),
                    SelfCost::Free,
                ),
                "empower",
            ),
            can_be_empowered_again,
        )],
    ),
    &[
        Static::CounterCap {
            counter: COUNTER_EMPOWERED,
            max: Some(TIMES),
        },
        Static::While(
            empowered,
            &[
                Grant::Might(MIGHT_PER_TIME),
                Grant::MightIf(empowered_twice, MIGHT_PER_TIME),
                Grant::MightIf(empowered_thrice, MIGHT_PER_TIME),
            ],
        ),
        Static::While(
            empowered_three_times,
            &[
                Grant::Keyword(Keyword::Deflect(DEFLECT)),
                Grant::Keyword(Keyword::Ganking),
            ],
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const KAYLE: u32 = 90;
    const EXTRA_RUNES: [u32; 6] = [46, 47, 48, 49, 50, 51];

    fn kayle() -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Order".into()],
            ..fixtures::unit(KAYLE, fixtures::BF1, 0, "Kayle, Justified", 3)
        }
    }

    fn cathedral(cap: Option<i32>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(kayle());
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        if let Some(bounds) = fixture
            .table
            .counter_table
            .iter_mut()
            .find(|bounds| bounds.id == COUNTER_EMPOWERED)
        {
            bounds.max = cap;
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(KAYLE).unwrap(), &CARD));
        fixture
    }

    fn her_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == KAYLE)
            .collect()
    }

    fn empower_her(ctx: &mut Ctx) {
        activate::activate(ctx, 0, KAYLE, EMPOWER_ABILITY).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn keywords(ctx: &Ctx) -> Vec<Keyword> {
        statics::grants_on(ctx, KAYLE)
            .into_iter()
            .filter_map(|grant| match grant {
                Grant::Keyword(keyword) => Some(keyword),
                _ => None,
            })
            .collect()
    }

    fn empowered_events(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Empowered { card, .. } if *card == KAYLE))
            .count()
    }

    #[test]
    fn the_champion_prints_empower_without_the_once_clause_and_ladders_her_might_and_keywords() {
        assert!(std::ptr::eq(script_of("Kayle, Justified").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 1);
        let empower = &CARD.abilities[usize::from(EMPOWER_ABILITY)];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        assert!(empower.targets.is_empty());
        assert!(
            empower.usable.is_some(),
            "the gate is the count, not the flag"
        );
        assert!(matches!(
            CARD.statics,
            [
                Static::CounterCap {
                    counter: COUNTER_EMPOWERED,
                    max: Some(TIMES)
                },
                Static::While(
                    _,
                    [
                        Grant::Might(MIGHT_PER_TIME),
                        Grant::MightIf(_, MIGHT_PER_TIME),
                        Grant::MightIf(_, MIGHT_PER_TIME)
                    ]
                ),
                Static::While(
                    _,
                    [
                        Grant::Keyword(Keyword::Deflect(DEFLECT)),
                        Grant::Keyword(Keyword::Ganking)
                    ]
                )
            ]
        ));
        assert_eq!((TIMES, MIGHT_PER_TIME, DEFLECT), (3, 2, 3));
    }

    #[test]
    fn the_first_empower_pays_three_and_reads_plus_two_and_no_keywords() {
        let mut fixture = cathedral(Some(1));
        let mut ctx = fixture.ctx();
        assert_eq!(times_empowered(&ctx, KAYLE), 0);
        assert_eq!(ctx.current_might(KAYLE), 3);
        let offers = her_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {KAYLE}}}: empower (3 energy)")
        );
        assert!(offers[0].enabled);
        let runes = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, KAYLE, EMPOWER_ABILITY).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.ready_runes_of(0).len(), runes - 3);
        assert!(
            !ctx.card(KAYLE).unwrap().exhausted,
            "827.1 · Empower never exhausts her"
        );
        assert_eq!(times_empowered(&ctx, KAYLE), 0, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(KAYLE));
        assert_eq!(times_empowered(&ctx, KAYLE), 1);
        assert_eq!(empowered_events(&ctx), 1);
        assert_eq!(ctx.current_might(KAYLE), 3 + i32::from(MIGHT_PER_TIME));
        assert!(keywords(&ctx).is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| *line == format!("{{card {KAYLE}}} is Empowered 1 of {TIMES} times")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn under_the_manifest_cap_of_one_a_second_empower_is_not_offered_and_is_refused() {
        let mut fixture = cathedral(Some(1));
        let mut ctx = fixture.ctx();
        empower_her(&mut ctx);
        assert_eq!(counter_room(&ctx, KAYLE), 0);
        assert!(!can_be_empowered_again(
            &ctx,
            Source {
                card: KAYLE,
                ability: EMPOWER_ABILITY
            }
        ));
        assert!(her_offers(&ctx).is_empty(), "377.2.b · the offer is gone");
        assert_eq!(
            activate::activate(&mut ctx, 0, KAYLE, EMPOWER_ABILITY),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(
            !empower_once_more(&mut ctx, KAYLE),
            "no room on the counter"
        );
        assert_eq!(times_empowered(&ctx, KAYLE), 1);
        assert_eq!(ctx.current_might(KAYLE), 5);
    }

    #[test]
    fn two_ready_runes_grey_the_offer_and_refuse_the_activation() {
        let mut fixture = cathedral(Some(3));
        for card in fixture.table.cards.iter_mut() {
            if card.is_kind("Rune") && card.owner == 0 && ![41, 42].contains(&card.id) {
                card.exhausted = true;
            }
        }
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 2);
        let offers = her_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].enabled);
        assert_eq!(
            activate::activate(&mut ctx, 0, KAYLE, EMPOWER_ABILITY),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 2
            })
        );
        assert_eq!(times_empowered(&ctx, KAYLE), 0);
    }

    #[test]
    fn with_room_on_the_counter_three_empowers_read_nine_might_deflect_three_and_ganking_then_stop()
    {
        let mut fixture = cathedral(Some(3));
        let mut ctx = fixture.ctx();
        empower_her(&mut ctx);
        assert_eq!(times_empowered(&ctx, KAYLE), 1);
        assert_eq!(ctx.current_might(KAYLE), 5);
        assert!(keywords(&ctx).is_empty());
        assert_eq!(her_offers(&ctx).len(), 1, "she may be Empowered again");
        empower_her(&mut ctx);
        assert_eq!(times_empowered(&ctx, KAYLE), 2);
        assert_eq!(
            empowered_events(&ctx),
            2,
            "441.2.a · each Empower is an event"
        );
        assert_eq!(ctx.current_might(KAYLE), 7);
        assert!(keywords(&ctx).is_empty());
        empower_her(&mut ctx);
        assert_eq!(times_empowered(&ctx, KAYLE), 3);
        assert_eq!(ctx.current_might(KAYLE), 9);
        assert_eq!(
            keywords(&ctx),
            [Keyword::Deflect(DEFLECT), Keyword::Ganking]
        );
        assert!(ctx.has_keyword(KAYLE, Keyword::Ganking));
        assert!(her_offers(&ctx).is_empty(), "three is the limit");
        assert_eq!(
            activate::activate(&mut ctx, 0, KAYLE, EMPOWER_ABILITY),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(!empower_once_more(&mut ctx, KAYLE));
        assert_eq!(times_empowered(&ctx, KAYLE), 3);
        assert!(ctx.disempower(KAYLE), "a disempower clears every level");
        assert_eq!(times_empowered(&ctx, KAYLE), 0);
        assert_eq!(ctx.current_might(KAYLE), 3);
        assert!(keywords(&ctx).is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn on_the_manifest_table_she_can_be_empowered_three_times() {
        let mut fixture = cathedral(None);
        let mut ctx = fixture.ctx();
        assert!(counter_room(&ctx, KAYLE) >= TIMES);
        for expected in 1..=TIMES {
            assert_eq!(her_offers(&ctx).len(), 1);
            empower_her(&mut ctx);
            assert_eq!(times_empowered(&ctx, KAYLE), expected);
        }
        assert_eq!(ctx.current_might(KAYLE), 9);
        assert!(ctx.has_keyword(KAYLE, Keyword::Ganking));
        assert!(her_offers(&ctx).is_empty());
    }
}
