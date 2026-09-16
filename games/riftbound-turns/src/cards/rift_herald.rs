use super::ivern_nurturer::{
    draw_the_revealed_unit, look_at_the_top_three, on_top, reveal_the_pick,
};
use super::prelude::{
    asking, deathknell, done, forget_revealing, hand_card_may_be_a_priced_unit,
    hand_cards_that_may_be_a_priced_unit, on_move_to_battlefield, unit, unit_priced_in_hand,
    viable_if, with_candidates, LimitedPlay, Location, Price,
};
use super::rell_magnetic::origin_of_a_free_hand_play;
use super::{Ability, Card, Flow, Item, Keyword, Stage, KIND_UNIT};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, FLAG_REVEALING};

pub const PICK: u8 = 1;
pub const REVEALED: u8 = 2;
pub const SCOUT_QUESTION: &str = "a unit among the top three to reveal and draw";
pub const KNELL_QUESTION: &str = "a unit in your hand to play to your base for its Power cost";

fn picked_in_hand(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .find(|card| ctx.owner(*card) == seat && ctx.in_hand(*card))
}

pub fn play_from_hand_to_base_for_power_alone(ctx: &mut Ctx, item: &Item, unit: u32) -> bool {
    let seat = item.controller;
    if !ctx.in_hand(unit) || ctx.owner(unit) != seat {
        ctx.narrate(format!("{{card {unit}}} is no longer in hand"));
        return false;
    }
    if ctx.kind_of(unit) != Some(KIND_UNIT) {
        ctx.narrate(format!("{{card {unit}}} is not a unit · it stays in hand"));
        return false;
    }
    let price = cost::priced(ctx, unit, Price::PowerOnly, false);
    if !unit_priced_in_hand(ctx, seat, unit, Price::PowerOnly) {
        ctx.narrate(format!(
            "{{card {unit}}} stays in hand · {} can't be paid",
            price.label()
        ));
        return false;
    }
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {unit}}} from hand to their base, ignoring its Energy cost"
    ));
    ctx.play_limited(LimitedPlay {
        card: unit,
        by: seat,
        origin: origin_of_a_free_hand_play(),
        locations: vec![Location::Base(seat)],
        price: Price::PowerOnly,
    })
    .is_ok()
}

fn scout_candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        PICK => on_top(ctx, item),
        _ => Vec::new(),
    }
}

fn scout(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        PICK => reveal_the_pick(ctx, item, REVEALED),
        REVEALED => {
            draw_the_revealed_unit(ctx, item.controller);
            done()
        }
        _ => look_at_the_top_three(ctx, item, PICK),
    }
}

fn hand_candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        PICK => hand_cards_that_may_be_a_priced_unit(ctx, item.controller, Price::PowerOnly)
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        _ => Vec::new(),
    }
}

fn hand_viable(ctx: &Ctx, item: &Item, target: TargetRef) -> bool {
    match target {
        TargetRef::Card(card) => {
            hand_card_may_be_a_priced_unit(ctx, item.controller, card, Price::PowerOnly)
        }
        _ => true,
    }
}

fn knell(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        PICK => {
            let Some(card) = ctx
                .picks()
                .first()
                .copied()
                .filter(|card| ctx.hand_of(seat).contains(card))
            else {
                ctx.narrate(format!("{{seat {seat}}} plays nothing"));
                return done();
            };
            ctx.set_flag(card, FLAG_REVEALING, true);
            if ctx.table.is_revealed(card) {
                return knell(ctx, item, Stage(REVEALED));
            }
            ctx.narrate(format!("{{seat {seat}}} reveals {{card {card}}}"));
            Flow::Ask(ctx.await_faces(item, &[card], REVEALED))
        }
        REVEALED => {
            let Some(unit) = picked_in_hand(ctx, seat) else {
                return done();
            };
            forget_revealing(ctx, unit);
            play_from_hand_to_base_for_power_alone(ctx, item, unit);
            done()
        }
        _ => {
            if ctx.hand_of(seat).is_empty() {
                ctx.narrate(format!("{{seat {seat}}} has no card in hand"));
                return done();
            }
            if hand_cards_that_may_be_a_priced_unit(ctx, seat, Price::PowerOnly).is_empty() {
                ctx.narrate(format!(
                    "{{seat {seat}}} has no unit in hand whose Power cost can be paid"
                ));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, PICK, 0, 1))
        }
    }
}

const ON_MOVE: Ability = asking(
    with_candidates(on_move_to_battlefield(&[], scout), scout_candidates),
    SCOUT_QUESTION,
);
const KNELL: Ability = viable_if(
    asking(
        with_candidates(deathknell(&[], knell), hand_candidates),
        KNELL_QUESTION,
    ),
    hand_viable,
);

pub static CARD: Card = unit("Rift Herald", &[Keyword::Deathknell], &[ON_MOVE, KNELL]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, Where, Who, KIND_SPELL};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, march, prompts, settle};
    use crate::present::present;
    use crate::state::{ItemKind, ItemStatus, Origin, PromptWhy};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::{CardInfo, Face, Snapshot};
    use agni_plugin_sdk::view::{PluginView, Request};

    const HERALD: u32 = 90;

    fn herald(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(8),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(HERALD, zone, seat, "Rift Herald", 7)
        }
    }

    fn pit(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(herald(zone, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn march_to(fixture: &mut Fixture, from: Location, to: Location) {
        let zone = to.battlefield().unwrap_or(fixtures::BASE);
        let action = fixtures::move_action(HERALD, zone, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, HERALD, from, to);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn pick(fixture: &mut Fixture, label: &str) {
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, label).unwrap();
        pass_until_parked(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face) {
        let action = Action::Reveal { card, face };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn dies(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        ctx.kill(HERALD, Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(HERALD).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == HERALD
        ));
        fixtures::pass_until_open(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    #[test]
    fn the_script_prints_deathknell_a_move_to_battlefield_look_and_a_hand_play_knell() {
        assert!(std::ptr::eq(script_of("Rift Herald").unwrap(), &CARD));
        assert_eq!(CARD.name, "Rift Herald");
        assert_eq!(CARD.keywords, [Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let moved = &CARD.abilities[0];
        assert_eq!(
            moved.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert_eq!(moved.question, Some(SCOUT_QUESTION));
        let knell = &CARD.abilities[1];
        assert_eq!(knell.trigger, Trigger::Death);
        assert_eq!(knell.question, Some(KNELL_QUESTION));
        assert!(knell.viable.is_some());
        assert!(moved.viable.is_none());
        for ability in CARD.abilities {
            assert!(ability.targets.is_empty());
            assert!(!ability.optional);
            assert!(ability.candidates.is_some());
            assert!(ability.cost.is_none() && ability.condition.is_none());
        }
        let questions = prompts::resume_questions();
        assert!(questions.contains(&SCOUT_QUESTION));
        assert!(questions.contains(&KNELL_QUESTION));
    }

    #[test]
    fn a_march_to_a_battlefield_looks_at_three_reveals_the_pick_and_draws_a_unit() {
        let mut fixture = pit(fixtures::BASE);
        let hand = fixture.ctx().hand_of(0).len();
        march_to(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}", "{card 22}", "{card 21}", "skip"]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {HERALD}}}: choose {SCOUT_QUESTION} (0 of 1)")
        );
        drop(ctx);
        pick(&mut fixture, "{card 21}");
        let ctx = fixture.ctx();
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [21]);
        assert_eq!(
            deck_of(&ctx, 0),
            [22, 23, 20],
            "the rest recycled under the deck, top first"
        );
        drop(ctx);
        arrives(&mut fixture, 21, Face::named("Jinx").with_kind(KIND_UNIT));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.card(21).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert!(!ctx.has_flag(21, FLAG_REVEALING));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws {card 21}".to_string()));
    }

    #[test]
    fn a_revealed_spell_is_recycled_and_the_walk_home_looks_at_nothing() {
        let mut fixture = pit(fixtures::BASE);
        let hand = fixture.ctx().hand_of(0).len();
        march_to(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        pick(&mut fixture, "{card 23}");
        arrives(&mut fixture, 23, Face::named("Spark").with_kind(KIND_SPELL));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(deck_of(&ctx, 0), [23, 21, 22, 20]);
        drop(ctx);

        let mut fixture = pit(fixtures::BF1);
        let action = fixtures::move_action(HERALD, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(
            &mut ctx,
            0,
            HERALD,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(HERALD), Some(Location::Base(0)));
        assert!(ctx.blob.chain.is_empty(), "to a battlefield, not home");
    }

    #[test]
    fn his_death_offers_the_hand_reveals_the_pick_and_plays_a_unit_to_the_base_for_its_power_alone()
    {
        let mut fixture = pit(fixtures::BF1);
        dies(&mut fixture);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        let mut hand: Vec<String> = ctx
            .hand_of(0)
            .into_iter()
            .map(|card| format!("{{card {card}}}"))
            .collect();
        hand.push("skip".to_string());
        hand.sort();
        assert_eq!(
            offered, hand,
            "every hand card · decide is blind to hand faces"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {HERALD}}}: choose {KNELL_QUESTION} (0 of 1)")
        );
        assert_eq!(
            ctx.card(fixtures::RUNE_A).unwrap().zone,
            Some(fixtures::RUNE_POOL)
        );
        drop(ctx);
        pick(&mut fixture, &format!("{{card {}}}", fixtures::HAND_UNIT));
        let ctx = fixture.ctx();
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [fixtures::HAND_UNIT]);
        drop(ctx);
        arrives(
            &mut fixture,
            fixtures::HAND_UNIT,
            Face::named("Shadow Order Disciple")
                .with_kind(KIND_UNIT)
                .with_might(Some(2))
                .with_cost(Some(6), Some(1))
                .with_domain(vec!["Fury".into()]),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::HAND_UNIT),
            Some(Location::Base(0)),
            "played to the base"
        );
        assert!(
            ctx.events.iter().any(|event| matches!(
                event,
                Event::Played { card, .. } if *card == fixtures::HAND_UNIT
            )) || ctx.on_board(fixtures::HAND_UNIT)
        );
        assert_eq!(
            ctx.card(fixtures::RUNE_A).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the exhausted Fury rune is recycled for the Power cost · no rune for the 6 energy"
        );
        assert_eq!(
            ctx.runes_of(0)
                .iter()
                .filter(|rune| !rune.exhausted)
                .count(),
            3
        );
        assert!(ctx.blob.log.iter().any(|line| line
            == &format!(
                "{{seat 0}} plays {{card {}}} from hand to their base, ignoring its Energy cost",
                fixtures::HAND_UNIT
            )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_revealed_spell_stays_in_hand_and_skipping_plays_nothing() {
        let mut fixture = pit(fixtures::BF1);
        dies(&mut fixture);
        pick(&mut fixture, &format!("{{card {}}}", fixtures::HAND_SPELL));
        arrives(
            &mut fixture,
            fixtures::HAND_SPELL,
            Face::named("Spark").with_kind(KIND_SPELL),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(fixtures::HAND_SPELL));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} is not a unit · it stays in hand",
            fixtures::HAND_SPELL
        )));
        drop(ctx);

        let mut fixture = pit(fixtures::BF1);
        dies(&mut fixture);
        pick(&mut fixture, "skip");
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.log.contains(&"{seat 0} plays nothing".to_string()));
    }

    #[test]
    fn with_an_empty_hand_the_knell_just_finishes() {
        let mut fixture = pit(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.seat != 0);
        fixture.resolve();
        dies(&mut fixture);
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card in hand".to_string()));
    }

    fn view_of(fixture: &Fixture, table: Snapshot) -> PluginView {
        present(&Request {
            plugin_state: fixture.blob.encode(),
            players: 2,
            seat: 0,
            zones: table.zones.clone(),
            table,
        })
    }

    fn enabled_cards(view: &PluginView) -> Vec<(u32, bool)> {
        view.affordances
            .iter()
            .filter_map(|offer| Some((offer.card?, offer.enabled)))
            .collect()
    }

    #[test]
    fn the_seats_own_view_greys_the_hand_cards_it_sees_are_no_payable_unit_and_the_play_reports_the_hand(
    ) {
        let mut fixture = pit(fixtures::BF1);
        dies(&mut fixture);
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::HAND_UNIT),
                format!("{{card {}}}", fixtures::HAND_SPELL),
                format!("{{card {}}}", fixtures::HAND_GEAR),
                format!("{{card {}}}", fixtures::HAND_HIDDEN),
                "skip".to_string()
            ],
            "the fold offers every hand card · no hand face is public in the log"
        );
        drop(ctx);
        let mine = view_of(&fixture, fixture.table.clone());
        assert_eq!(
            enabled_cards(&mine),
            [
                (fixtures::HAND_UNIT, true),
                (fixtures::HAND_SPELL, false),
                (fixtures::HAND_GEAR, false),
                (fixtures::HAND_HIDDEN, true)
            ],
            "the seat's view knows its own faces: the spell and the gear are greyed, the unknown card stays live"
        );
        let mut blind = fixture.table.clone();
        for card in [
            fixtures::HAND_UNIT,
            fixtures::HAND_SPELL,
            fixtures::HAND_GEAR,
        ] {
            *blind.card_mut(card).unwrap() = fixtures::hidden(card, fixtures::HAND, 0);
        }
        let folded = view_of(&fixture, blind);
        assert_eq!(
            enabled_cards(&folded),
            [
                (fixtures::HAND_UNIT, true),
                (fixtures::HAND_SPELL, true),
                (fixtures::HAND_GEAR, true),
                (fixtures::HAND_HIDDEN, true)
            ],
            "the same list in the same order without the faces · a pick by index lands on the same card"
        );
        let mut costly = fixture.table.clone();
        {
            let unit = costly.card_mut(fixtures::HAND_UNIT).unwrap();
            unit.power = Some(2);
            unit.domain = vec!["Calm".into()];
        }
        let dear = view_of(&fixture, costly);
        assert_eq!(
            enabled_cards(&dear)[0],
            (fixtures::HAND_UNIT, false),
            "a unit whose Power cost the runes cannot pay is greyed too"
        );
        pick(&mut fixture, &format!("{{card {}}}", fixtures::HAND_UNIT));
        let action = Action::Reveal {
            card: fixtures::HAND_UNIT,
            face: Face::named("Shadow Order Disciple")
                .with_kind(KIND_UNIT)
                .with_might(Some(2))
                .with_cost(Some(6), Some(1))
                .with_domain(vec!["Fury".into()]),
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, fixtures::HAND_UNIT).unwrap());
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, origin: Origin::Hand, .. } if *card == fixtures::HAND_UNIT
        )));
        assert_eq!(ctx.location(fixtures::HAND_UNIT), Some(Location::Base(0)));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_knell_withholds_only_a_public_face_that_is_no_payable_unit_and_reports_a_hand_play() {
        let mut fixture = pit(fixtures::BF1);
        let disciple = Face::named("Shadow Order Disciple")
            .with_kind(KIND_UNIT)
            .with_might(Some(2))
            .with_cost(Some(6), Some(1))
            .with_domain(vec!["Fury".into()]);
        let spark = Face::named("Spark").with_kind(KIND_SPELL);
        for (card, face) in [
            (fixtures::HAND_UNIT, disciple),
            (fixtures::HAND_SPELL, spark),
        ] {
            fixture
                .table
                .apply_entry(&Action::Reveal { card, face }, 0)
                .unwrap();
        }
        fixture.resolve();
        dies(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::HAND_UNIT),
                format!("{{card {}}}", fixtures::HAND_GEAR),
                format!("{{card {}}}", fixtures::HAND_HIDDEN),
                "skip".to_string()
            ],
            "the revealed spell is not offered · the cards the log has no face for still are"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_UNIT)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, origin: Origin::Hand, .. } if *card == fixtures::HAND_UNIT
        )));
        assert_eq!(
            ctx.location(fixtures::HAND_UNIT),
            Some(Location::Base(0)),
            "a public face skips the wait"
        );
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = pit(fixtures::BF1);
        fixture.table.cards.retain(|card| {
            card.zone != Some(fixtures::HAND) || card.seat != 0 || card.id == fixtures::HAND_SPELL
        });
        fixture
            .table
            .apply_entry(
                &Action::Reveal {
                    card: fixtures::HAND_SPELL,
                    face: Face::named("Spark").with_kind(KIND_SPELL),
                },
                0,
            )
            .unwrap();
        fixture.resolve();
        dies(&mut fixture);
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no unit in hand whose Power cost can be paid".to_string()));
        assert!(ctx.in_hand(fixtures::HAND_SPELL));
    }
}
