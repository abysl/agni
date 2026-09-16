use super::prelude::{
    asking, done, forget_revealing, play, remember_card, remembered_cards, spell, with_candidates,
    Location,
};
use super::rek_sai_void_burrower::{play_revealed_for, playable_for};
use super::{Card, Flow, Item, Stage};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{Origin, RevealedFrom, TargetRef};
use agni_plugin_sdk::decide::{Effect, TOP};

pub const LOOK: usize = 2;
pub const DISCOUNT: u8 = 2;
pub const QUESTION: &str = "a revealed card to play for 2 less, then where it is played";
pub const REVEALED: u8 = 1;
pub const CHOOSE: u8 = 2;
pub const LOCATE: u8 = 3;

pub fn reduced_cost(ctx: &Ctx, card: u32) -> cost::Cost {
    let printed = cost::printed_of(ctx, card, false);
    cost::Cost {
        energy: printed.energy.saturating_sub(DISCOUNT),
        ..printed
    }
}

pub fn playable(ctx: &Ctx, seat: u8, cards: &[u32]) -> Vec<u32> {
    cards
        .iter()
        .copied()
        .filter(|card| playable_for(ctx, seat, *card, reduced_cost))
        .collect()
}

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        CHOOSE => playable(ctx, item.controller, &remembered_cards(item))
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        LOCATE => location_options(ctx, item.controller),
        _ => Vec::new(),
    }
}

pub fn play_for_two_less(ctx: &mut Ctx, seat: u8, card: u32, at: Option<Location>) -> bool {
    play_revealed_for(
        ctx,
        seat,
        card,
        reduced_cost,
        Origin::Revealed {
            from: RevealedFrom::Deck,
        },
        at,
    )
}

fn draw_the_rest(ctx: &mut Ctx, seat: u8, cards: &[u32], played: Option<u32>) {
    let (Some(deck), Some(chain)) = (ctx.zones.main_deck, ctx.zones.chain) else {
        return;
    };
    let rest: Vec<u32> = cards
        .iter()
        .copied()
        .filter(|card| Some(*card) != played)
        .filter(|card| ctx.card(*card).is_some_and(|held| held.zone == Some(chain)))
        .collect();
    for card in &rest {
        forget_revealing(ctx, *card);
        ctx.emit(Effect::Move {
            card: *card,
            zone: deck,
            seat,
            index: TOP,
        });
    }
    let drawn = ctx.draw(seat, rest.len());
    ctx.narrate(format!("{{seat {seat}}} draws the {drawn} not played"));
}

fn chosen(ctx: &Ctx, item: &Item) -> Option<u32> {
    let offered = playable(ctx, item.controller, &remembered_cards(item));
    ctx.picks()
        .first()
        .copied()
        .filter(|card| offered.contains(card))
}

fn offer(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let revealed = remembered_cards(item);
    if playable(ctx, seat, &revealed).is_empty() {
        ctx.narrate(format!(
            "{{seat {seat}}} can play none of the revealed cards"
        ));
        draw_the_rest(ctx, seat, &revealed, None);
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, CHOOSE, 0, 1))
}

fn rush(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        REVEALED => offer(ctx, item),
        CHOOSE => {
            let revealed = remembered_cards(item);
            let Some(card) = chosen(ctx, item) else {
                draw_the_rest(ctx, seat, &revealed, None);
                return done();
            };
            if ctx.is_unit(card) && ctx.play_locations(seat).len() > 1 {
                remember_card(ctx, card);
                return Flow::Ask(ctx.ask_resume(item, LOCATE, 1, 1));
            }
            let played = play_for_two_less(ctx, seat, card, None).then_some(card);
            draw_the_rest(ctx, seat, &revealed, played);
            done()
        }
        LOCATE => {
            let mut remembered = remembered_cards(item);
            let Some(card) = remembered.pop() else {
                return done();
            };
            let at = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
                .filter(|at| ctx.play_locations(seat).contains(at));
            let played = play_for_two_less(ctx, seat, card, at).then_some(card);
            draw_the_rest(ctx, seat, &remembered, played);
            done()
        }
        _ => {
            let mut revealed = Vec::new();
            for _ in 0..LOOK {
                if let Some(card) = ctx.reveal_top(seat) {
                    remember_card(ctx, card);
                    revealed.push(card);
                }
            }
            if revealed.is_empty() {
                ctx.narrate(format!("{{seat {seat}}} has no card left to reveal"));
                return done();
            }
            let unseen: Vec<u32> = revealed
                .iter()
                .copied()
                .filter(|card| !ctx.table.is_revealed(*card))
                .collect();
            if unseen.is_empty() {
                return offer(ctx, item);
            }
            Flow::Ask(ctx.await_faces(item, &unseen, REVEALED))
        }
    }
}

pub static CARD: Card = spell(
    "Void Rush",
    &[],
    &[asking(
        with_candidates(play(&[], rush), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, card_target, might_this_turn, unit as unit_card};
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Filter, Keyword, Trigger, KIND_SPELL, KIND_UNIT};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, legal, prompts, settle};
    use crate::state::{ItemStatus, PromptWhy, FLAG_REVEALING};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::{CardInfo, Face};

    const RUSH: u32 = 90;
    const THEIR_RUSH: u32 = 91;
    const FURY_RUNE: u32 = 100;
    const ORDER_RUNE: u32 = 101;
    const FIRST_EXTRA_RUNE: u32 = 102;
    const TOP: u32 = 23;
    const SECOND: u32 = 22;

    static ORPHAN: Card = spell(
        "Orphan",
        &[],
        &[play(
            &[crate::cards::prelude::a_card(
                Filter::Named("Nobody"),
                "nobody",
            )],
            |_, _, _| done(),
        )],
    );

    static PUMP: Card = spell(
        "Pump",
        &[],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = card_target(ctx, item, 0) {
                might_this_turn(ctx, item, unit, 2, None);
            }
            Flow::Done
        })],
    );

    static CRAB: Card = unit_card("Crab", &[], &[]);

    fn rush(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Void Rush", 2, 2);
        card.domain = vec!["Fury".into(), "Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rush(RUSH, 0));
        fixture.table.cards.push(rush(THEIR_RUSH, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(FURY_RUNE, 0, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        for rune in FIRST_EXTRA_RUNE..FIRST_EXTRA_RUNE + 3 {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing is targeted");
        fixtures::pass_until_open(&mut ctx);
        for card in [TOP, SECOND] {
            if ctx
                .card(card)
                .is_some_and(|held| held.zone == Some(fixtures::CHAIN))
            {
                assert!(ctx.effects.contains(&Effect::Reveal { card }));
            }
        }
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face, script: &'static Card) -> bool {
        let action = Action::Reveal { card, face };
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
        let mut ctx = fixture.ctx_for(0, &action);
        let arrived = chain::face_arrived(&mut ctx, card).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
        arrived
    }

    fn crab_face(energy: u8) -> Face {
        Face::named("Crab")
            .with_kind(KIND_UNIT)
            .with_might(Some(2))
            .with_cost(Some(energy), Some(1))
            .with_domain(vec!["Fury".into()])
    }

    fn pump_face(energy: u8) -> Face {
        Face::named("Pump")
            .with_kind(crate::cards::KIND_SPELL)
            .with_cost(Some(energy), Some(1))
            .with_domain(vec!["Fury".into()])
    }

    fn reveal_both(fixture: &mut Fixture, top: Face, second: Face) {
        cast(fixture);
        {
            let ctx = fixture.ctx();
            let parked = &ctx.blob.chain[0];
            assert_eq!(parked.status, ItemStatus::Resolving);
            assert_eq!(parked.stage, REVEALED);
            assert_eq!(parked.awaiting, [TOP, SECOND]);
            assert_eq!(deck_of(&ctx, 0), [20, 21], "two left in the deck");
            for card in [TOP, SECOND] {
                assert_eq!(ctx.card(card).unwrap().zone, Some(fixtures::CHAIN));
                assert!(ctx.has_flag(card, FLAG_REVEALING));
            }
        }
        let crab = if top.name == "Crab" { &CRAB } else { &PUMP };
        assert!(arrives(fixture, TOP, top, crab));
        let second_script = if second.name == "Crab" { &CRAB } else { &PUMP };
        assert!(arrives(fixture, SECOND, second, second_script));
    }

    #[test]
    fn the_script_is_a_targetless_sorcery_whose_stages_reveal_choose_then_locate() {
        assert!(std::ptr::eq(script_of("Void Rush").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(CARD.abilities[0].question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(1);
        fixture.resolve();
        let ctx = fixture.ctx();
        let reduced = reduced_cost(&ctx, fixtures::HAND_UNIT);
        assert_eq!(reduced.energy, 0, "a cost of one goes to zero, never below");
        let spark = reduced_cost(&ctx, fixtures::HAND_SPELL);
        assert_eq!(spark.energy, 0);
        assert_eq!(
            spark.power,
            cost::printed_of(&ctx, fixtures::HAND_SPELL, false).power,
            "the power is untouched"
        );
    }

    #[test]
    fn the_top_two_are_revealed_one_is_played_for_two_less_and_the_other_is_drawn() {
        let mut fixture = armed();
        reveal_both(&mut fixture, crab_face(6), pump_face(3));
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: CHOOSE
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}", "{card 22}", "skip"],
            "both are affordable at two less: the Crab for 4, Pump for 1"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card 90}}: choose {QUESTION} (0 of 1)")
        );
        let hand = ctx.hand_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "{card 22}").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 2, spec: 0 }),
            "Pump is on the chain asking its target · {:?}",
            ctx.blob.log
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 1,
            "3 energy less 2: one rune, which also pays the Fury power"
        );
        assert!(ctx.blob.log.contains(
            &"{seat 0} plays the revealed {card 22} for 1 energy and 1 Fury power".to_string()
        ));
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "the Crab was drawn");
        assert_eq!(ctx.card(TOP).unwrap().zone, Some(fixtures::HAND));
        assert!(!ctx.has_flag(TOP, FLAG_REVEALING));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, nth: 1 })));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws the 1 not played".to_string()));
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Revealed {
                from: RevealedFrom::Deck
            }
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4);
        assert_eq!(ctx.card(SECOND).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(RUSH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_draws_both_and_an_unaffordable_card_is_not_offered() {
        let mut fixture = armed();
        reveal_both(&mut fixture, crab_face(9), pump_face(3));
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 22}", "skip"],
            "seven energy after the discount is beyond the six ready runes"
        );
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + 2);
        for card in [TOP, SECOND] {
            assert_eq!(ctx.card(card).unwrap().zone, Some(fixtures::HAND));
            assert_eq!(ctx.card(card).unwrap().seat, 0);
            assert!(!ctx.has_flag(card, FLAG_REVEALING));
        }
        assert_eq!(ctx.blob.seat(0).draws, 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws the 2 not played".to_string()));
        assert_eq!(ctx.card(RUSH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_revealed_spell_without_a_legal_target_is_not_offered_and_is_drawn_instead() {
        let mut fixture = armed();
        cast(&mut fixture);
        assert!(arrives(
            &mut fixture,
            TOP,
            Face::named("Orphan")
                .with_kind(KIND_SPELL)
                .with_cost(Some(3), None)
                .with_domain(vec!["Fury".into()]),
            &ORPHAN
        ));
        assert!(arrives(&mut fixture, SECOND, crab_face(9), &CRAB));
        let mut ctx = fixture.ctx();
        let runes = ctx.ready_runes_of(0).len();
        assert!(
            playable(&ctx, 0, &[TOP, SECOND]).is_empty(),
            "nobody to target"
        );
        assert!(ctx.blob.prompt.is_none(), "{:?}", fixtures::labels(&ctx));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.ready_runes_of(0).len(), runes, "nothing was paid");
        for card in [TOP, SECOND] {
            assert_eq!(ctx.card(card).unwrap().zone, Some(fixtures::HAND));
            assert!(!ctx.has_flag(card, FLAG_REVEALING));
        }
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} can play none of the revealed cards".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_revealed_unit_asks_where_it_is_played_when_a_battlefield_is_held() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        reveal_both(&mut fixture, crab_face(6), pump_face(3));
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: LOCATE
            })
        );
        assert_eq!(fixtures::labels(&ctx), ["{zone 8}", "{zone 9}"]);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        pass_until_parked(&mut ctx);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 4,
            "6 energy less 2 · {:?}",
            ctx.blob.log
        );
        assert_eq!(
            ctx.location(TOP),
            Some(Location::Battlefield(fixtures::BF1)),
            "{:?}",
            ctx.blob.log
        );
        assert!(ctx.card(TOP).unwrap().exhausted);
        assert_eq!(ctx.card(SECOND).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_deck_of_one_reveals_one_and_an_empty_deck_reveals_nothing() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![20, 21, 22].contains(&card.id));
        fixture.resolve();
        cast(&mut fixture);
        {
            let ctx = fixture.ctx();
            assert_eq!(ctx.blob.chain[0].awaiting, [TOP]);
        }
        assert!(arrives(&mut fixture, TOP, pump_face(3), &PUMP));
        let mut ctx = fixture.ctx();
        assert_eq!(fixtures::labels(&ctx), ["{card 23}", "skip"]);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.card(TOP).unwrap().zone, Some(fixtures::HAND));
        drop(ctx);

        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RUSH).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card left to reveal".to_string()));
        assert_eq!(ctx.card(RUSH).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn the_rush_is_the_turn_players_and_needs_fury_and_order() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_RUSH)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut poor = armed();
        poor.table.card_mut(ORDER_RUNE).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, RUSH)),
            Err(Refusal::NoPowerOf),
            "one Fury and one Order"
        );
    }

    #[test]
    fn the_played_card_reports_a_revealed_origin() {
        let mut fixture = armed();
        reveal_both(&mut fixture, crab_face(6), pump_face(3));
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "{card 22}").unwrap();
        let queued = ctx
            .blob
            .pending(2)
            .expect("Pump waits for its target")
            .item
            .clone();
        assert_ne!(queued.origin, Origin::Banishment);
    }
}
