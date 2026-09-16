use super::prelude::{
    activated, done, is_empowered, might_this_turn, named, paying_with, triggered, unit, when,
    with_statics, ONE_ENERGY,
};
use super::{
    Card, Event, Flow, Grant, Item, Keyword, SelfCost, Source, Stage, Static, Timing, Trigger,
};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
pub const THRESHOLD: i32 = 10;
pub const WHEN_MY_MIGHT_BECOMES_TEN: Trigger = Trigger::Reflexive;
pub static EMPOWERED_TEXT: &[Grant] = &[
    Grant::Keyword(Keyword::Deflect(1)),
    Grant::Keyword(Keyword::Ganking),
];

pub fn might_reached_ten(before: i32, after: i32) -> bool {
    before < THRESHOLD && after >= THRESHOLD
}

pub fn my_might_became_ten(_: &Ctx, _: &Event, _: Source) -> bool {
    false
}

pub fn my_might_became_ten_until_a_might_event_exists(
    ctx: &Ctx,
    before: i32,
    source: Source,
) -> bool {
    ctx.on_board(source.card) && might_reached_ten(before, ctx.current_might(source.card))
}

fn surge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    might_this_turn(ctx, item, me, MIGHT, None);
    ctx.narrate(format!("{{card {me}}} gets +{MIGHT} might this turn"));
    done()
}

pub fn empower_me(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    if ctx.empower_by(me, item.controller) {
        ctx.narrate(format!("{{card {me}}} is empowered"));
    }
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Renekton, Brute",
        &[],
        &[
            named(
                paying_with(
                    activated(Timing::Sorcery, ONE_ENERGY, &[], surge),
                    SelfCost::Free,
                ),
                "+1 Might this turn",
            ),
            when(
                triggered(WHEN_MY_MIGHT_BECOMES_TEN, &[], empower_me),
                my_might_became_ten,
            ),
        ],
    ),
    &[Static::While(is_empowered, EMPOWERED_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const RENEKTON: u32 = 90;
    const SPARE_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn renekton() -> CardInfo {
        CardInfo {
            energy: Some(5),
            domain: vec!["Body".into()],
            ..fixtures::unit(RENEKTON, fixtures::BASE, 0, "Renekton, Brute", 4)
        }
    }

    fn pit() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(renekton());
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RENEKTON).unwrap(),
            &CARD
        ));
        fixture
    }

    fn trigger_item() -> ChainItem {
        ChainItem::new(
            7,
            ItemKind::Trigger {
                source: RENEKTON,
                index: 1,
            },
            0,
            Origin::Board,
        )
    }

    fn surge_once(ctx: &mut Ctx) {
        activate::activate(ctx, 0, RENEKTON, 0).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_owns_no_keyword_pays_one_for_might_and_grants_deflect_and_ganking_while_empowered(
    ) {
        assert!(std::ptr::eq(script_of("Renekton, Brute").unwrap(), &CARD));
        assert!(
            CARD.keywords.is_empty(),
            "a keyword an Empowered line grants is not the card's own"
        );
        assert!(!CARD.has_keyword(Keyword::Deflect(1)));
        assert!(!CARD.has_keyword(Keyword::Ganking));
        assert_eq!(CARD.abilities.len(), 2);
        let surge = &CARD.abilities[0];
        assert_eq!(surge.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(surge.cost, Some(ONE_ENERGY));
        assert_eq!(surge.self_cost, SelfCost::Free, "no exhaust in the cost");
        assert!(surge.targets.is_empty());
        let threshold = &CARD.abilities[1];
        assert_eq!(threshold.trigger, WHEN_MY_MIGHT_BECOMES_TEN);
        assert!(threshold.condition.is_some());
        assert!(matches!(
            CARD.statics,
            [Static::While(
                _,
                [
                    Grant::Keyword(Keyword::Deflect(1)),
                    Grant::Keyword(Keyword::Ganking)
                ]
            )]
        ));
        assert!(might_reached_ten(9, 10));
        assert!(might_reached_ten(4, 12));
        assert!(!might_reached_ten(10, 11), "already at ten");
        assert!(!might_reached_ten(9, 9));
        assert!(
            !might_reached_ten(12, 9),
            "losing Might is not reaching ten"
        );
    }

    #[test]
    fn each_activation_pays_one_energy_for_one_might_this_turn_without_exhausting_him() {
        let mut fixture = pit();
        let mut ctx = fixture.ctx();
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == RENEKTON)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {RENEKTON}}}: +1 Might this turn (1 energy)")
        );
        let ready = ctx.ready_runes_of(0).len();
        surge_once(&mut ctx);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
        assert_eq!(ctx.current_might(RENEKTON), 5);
        assert!(
            !ctx.card(RENEKTON).unwrap().exhausted,
            "no exhaust in the cost"
        );
        surge_once(&mut ctx);
        assert_eq!(ctx.current_might(RENEKTON), 6, "no once-per-turn");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_other_seat_and_an_empty_rune_pool_are_refused() {
        let mut fixture = pit();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, RENEKTON, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        for rune in ctx
            .runes_of(0)
            .iter()
            .map(|rune| rune.id)
            .collect::<Vec<u32>>()
        {
            ctx.exhaust(rune);
        }
        assert!(activate::activate(&mut ctx, 0, RENEKTON, 0).is_err());
        assert_eq!(ctx.current_might(RENEKTON), 4);
    }

    #[test]
    fn the_threshold_run_empowers_him_and_the_empowered_line_then_reads_deflect_and_ganking() {
        let mut fixture = pit();
        let mut ctx = fixture.ctx();
        assert!(!ctx.has_keyword(RENEKTON, Keyword::Deflect(1)));
        assert!(!ctx.has_keyword(RENEKTON, Keyword::Ganking));
        assert_eq!(empower_me(&mut ctx, &trigger_item(), Stage(0)), Flow::Done);
        assert!(ctx.is_empowered(RENEKTON));
        assert!(ctx.has_keyword(RENEKTON, Keyword::Deflect(1)));
        assert!(ctx.has_keyword(RENEKTON, Keyword::Ganking));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {RENEKTON}}} is empowered")));
        assert_eq!(
            empower_me(&mut ctx, &trigger_item(), Stage(0)),
            Flow::Done,
            "empowering twice is nothing"
        );
        assert!(ctx.disempower(RENEKTON));
        assert!(!ctx.has_keyword(RENEKTON, Keyword::Ganking));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_threshold_reader_fires_only_as_his_might_crosses_ten_while_he_stands() {
        let mut fixture = pit();
        let mut ctx = fixture.ctx();
        let source = Source {
            card: RENEKTON,
            ability: 1,
        };
        for _ in 0..5 {
            surge_once(&mut ctx);
        }
        assert_eq!(ctx.current_might(RENEKTON), 9);
        assert!(!my_might_became_ten_until_a_might_event_exists(
            &ctx, 8, source
        ));
        surge_once(&mut ctx);
        assert_eq!(ctx.current_might(RENEKTON), 10);
        assert!(my_might_became_ten_until_a_might_event_exists(
            &ctx, 9, source
        ));
        assert!(!my_might_became_ten_until_a_might_event_exists(
            &ctx, 10, source
        ));
        assert!(!my_might_became_ten(
            &ctx,
            &Event::Empowered {
                card: RENEKTON,
                by: 0
            },
            source
        ));
    }

    #[test]
    #[ignore = "engine gap · missing trigger subjects: no Might writer raises an event as current_might crosses a threshold and Trigger has no MightReached(Who::Me, 10) (the Fiora - Worthy BecameMighty row at ten); the sixth activation must queue renekton_brute::empower_me so he ends the turn Empowered with Deflect and Ganking"]
    fn six_activations_take_him_to_ten_and_the_trigger_empowers_him() {
        let mut fixture = pit();
        let mut ctx = fixture.ctx();
        for _ in 0..6 {
            surge_once(&mut ctx);
        }
        assert_eq!(ctx.current_might(RENEKTON), 10);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_empowered(RENEKTON));
        assert!(ctx.has_keyword(RENEKTON, Keyword::Ganking));
    }
}
