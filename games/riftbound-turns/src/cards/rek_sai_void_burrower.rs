use super::prelude::{
    asking, done, exhausting_self, forget_revealing, legend, on_conquer, optional, remember_card,
    remembered_cards, with_candidates, Location,
};
use super::teemo_strategist::reveal_top_cards;
use super::{Card, Flow, Item, Paying, Stage, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;
use crate::engine::{cost, pay, play, targets};
use crate::state::{Origin, TargetRef, FLAG_REVEALING};

pub const REVEAL: usize = 2;
pub const QUESTION: &str = "a revealed card to play, then where it is played";
pub const REVEALED: u8 = 1;
pub const CHOOSE: u8 = 2;
pub const LOCATE: u8 = 3;

pub fn revealing(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let Some(chain) = ctx.zones.chain else {
        return Vec::new();
    };
    ctx.table
        .held(chain, 0)
        .map(|card| card.id)
        .filter(|card| ctx.has_flag(*card, FLAG_REVEALING) && ctx.owner(*card) == seat)
        .collect()
}

fn recycle_revealed(ctx: &mut Ctx, seat: u8, cards: &[u32]) {
    for card in cards {
        forget_revealing(ctx, *card);
        ctx.recycle_to_bottom(*card);
    }
    if !cards.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", cards.len()));
    }
}

pub type Price = fn(&Ctx, u32) -> cost::Cost;

pub fn price_of(ctx: &Ctx, card: u32) -> cost::Cost {
    cost::printed_of(ctx, card, false)
}

pub fn playable_for(ctx: &Ctx, seat: u8, card: u32, price: Price) -> bool {
    let play = cost::play_item(ctx, seat, card, Origin::Banishment);
    ctx.kind_of(card).is_some()
        && pay::affordable_for(ctx, seat, &price(ctx, card), Paying::Item(&play))
        && targets::first_spec_fillable(ctx, &play)
}

pub fn playable(ctx: &Ctx, seat: u8, card: u32) -> bool {
    playable_for(ctx, seat, card, price_of)
}

pub fn playable_revealed(ctx: &Ctx, seat: u8) -> Vec<u32> {
    revealing(ctx, seat)
        .into_iter()
        .filter(|card| playable(ctx, seat, *card))
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
    let seat = item.controller;
    match stage.0 {
        CHOOSE => playable_revealed(ctx, seat)
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        LOCATE => location_options(ctx, seat),
        _ => Vec::new(),
    }
}

pub fn play_revealed_for(
    ctx: &mut Ctx,
    seat: u8,
    card: u32,
    price: Price,
    origin: Origin,
    at: Option<Location>,
) -> bool {
    let total = price(ctx, card);
    let play = cost::play_item(ctx, seat, card, Origin::Banishment);
    let Ok(plan) = pay::plan_for(ctx, seat, &total, Paying::Item(&play)) else {
        ctx.narrate(format!(
            "{{card {card}}} stays revealed · its cost can't be paid"
        ));
        return false;
    };
    forget_revealing(ctx, card);
    pay::pay(ctx, seat, &plan);
    match at {
        Some(at) => ctx.narrate(format!(
            "{{seat {seat}}} plays the revealed {{card {card}}} to {} for {}",
            describe(at),
            total.label()
        )),
        None => ctx.narrate(format!(
            "{{seat {seat}}} plays the revealed {{card {card}}} for {}",
            total.label()
        )),
    }
    play::begin(ctx, seat, card, origin, at).is_ok()
}

pub fn play_revealed(ctx: &mut Ctx, seat: u8, card: u32, at: Option<Location>) -> bool {
    play_revealed_for(ctx, seat, card, price_of, Origin::Banishment, at)
}

fn play_then_recycle(ctx: &mut Ctx, seat: u8, card: u32, at: Option<Location>) -> Flow {
    let rest: Vec<u32> = revealing(ctx, seat)
        .into_iter()
        .filter(|held| *held != card)
        .collect();
    if !play_revealed(ctx, seat, card, at) {
        recycle_revealed(ctx, seat, &[card]);
    }
    recycle_revealed(ctx, seat, &rest);
    done()
}

fn recycle_all(ctx: &mut Ctx, seat: u8) -> Flow {
    let all = revealing(ctx, seat);
    recycle_revealed(ctx, seat, &all);
    done()
}

fn burrow(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        REVEALED => {
            if playable_revealed(ctx, seat).is_empty() {
                ctx.narrate(format!(
                    "{{seat {seat}}} can play none of the revealed cards"
                ));
                return recycle_all(ctx, seat);
            }
            Flow::Ask(ctx.ask_resume(item, CHOOSE, 0, 1))
        }
        CHOOSE => {
            let Some(card) = ctx
                .picks()
                .first()
                .copied()
                .filter(|card| playable_revealed(ctx, seat).contains(card))
            else {
                ctx.narrate(format!("{{seat {seat}}} plays none of the revealed cards"));
                return recycle_all(ctx, seat);
            };
            if ctx.kind_of(card) == Some(KIND_UNIT) {
                let locations = ctx.play_locations(seat);
                if locations.len() > 1 {
                    remember_card(ctx, card);
                    return Flow::Ask(ctx.ask_resume(item, LOCATE, 1, 1));
                }
                return play_then_recycle(ctx, seat, card, locations.first().copied());
            }
            play_then_recycle(ctx, seat, card, None)
        }
        LOCATE => {
            let Some(card) = remembered_cards(item).last().copied() else {
                return recycle_all(ctx, seat);
            };
            let at = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
                .filter(|at| ctx.play_locations(seat).contains(at))
                .unwrap_or(Location::Base(seat));
            play_then_recycle(ctx, seat, card, Some(at))
        }
        _ => {
            let top = reveal_top_cards(ctx, seat, REVEAL);
            if top.is_empty() {
                ctx.narrate(format!("{{seat {seat}}} has no cards left to reveal"));
                return done();
            }
            let unseen: Vec<u32> = top
                .iter()
                .copied()
                .filter(|card| ctx.kind_of(*card).is_none())
                .collect();
            if unseen.is_empty() {
                return burrow(ctx, item, Stage(REVEALED));
            }
            Flow::Ask(ctx.await_faces(item, &unseen, REVEALED))
        }
    }
}

pub static CARD: Card = legend(
    "Rek'sai - Void Burrower",
    &[],
    &[asking(
        with_candidates(
            optional(exhausting_self(on_conquer(&[], burrow))),
            candidates,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, an_enemy_unit, might_this_turn, spell, unit};
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, SelfCost, Trigger, Who, KIND_SPELL, KIND_UNIT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, cleanup, prompts, settle};
    use crate::state::{ItemStatus, PromptWhy};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::table::Face;

    const REKSAI: u32 = fixtures::LEGEND_CARD;
    const TOP: u32 = 23;
    const SECOND: u32 = 22;

    static PUMP: Card = spell(
        "Pump",
        &[],
        &[crate::cards::prelude::play(
            &[a_unit("a unit")],
            |ctx, item, _| {
                if let Some(unit) = crate::cards::prelude::card_target(ctx, item, 0) {
                    might_this_turn(ctx, item, unit, 2, None);
                }
                Flow::Done
            },
        )],
    );

    static CRAB: Card = unit("Crab", &[], &[]);

    static ZAP: Card = spell(
        "Zap",
        &[],
        &[crate::cards::prelude::play(
            &[an_enemy_unit("an enemy unit")],
            |_, _, _| Flow::Done,
        )],
    );

    fn tunnels() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(REKSAI).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(REKSAI).unwrap(),
            &CARD
        ));
        fixture
    }

    fn conquer(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 }),
            "392.2 · the may is the cost confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "exhaust {{card {REKSAI}}} for the {{card {REKSAI}}} trigger · {{zone {}}}?",
                fixtures::BF1
            )
        );
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn dig(fixture: &mut Fixture) {
        conquer(fixture);
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(REKSAI).unwrap().exhausted,
            "exhausting her is the cost"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        pass_until_parked(&mut ctx);
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [TOP, SECOND]);
        for card in [TOP, SECOND] {
            assert_eq!(ctx.card(card).unwrap().zone, Some(fixtures::CHAIN));
            assert!(ctx.has_flag(card, FLAG_REVEALING));
        }
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals the top 2 cards of their deck".to_string()));
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn rescript(fixture: &mut Fixture, scripts: &[(u32, &'static Card)]) {
        for (card, script) in scripts {
            fixture.scripts = fixture.scripts.clone().with_script(*card, script);
        }
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face, scripts: &[(u32, &'static Card)]) {
        let action = Action::Reveal { card, face };
        rescript(fixture, scripts);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        rescript(fixture, scripts);
    }

    fn crab_face(energy: u8, power: u8) -> Face {
        Face::named("Crab")
            .with_kind(KIND_UNIT)
            .with_might(Some(2))
            .with_cost(Some(energy), Some(power))
            .with_domain(vec!["Fury".into()])
    }

    fn pump_face(energy: u8, power: u8) -> Face {
        Face::named("Pump")
            .with_kind(KIND_SPELL)
            .with_cost(Some(energy), Some(power))
            .with_domain(vec!["Fury".into()])
    }

    fn deck_of(ctx: &Ctx) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, 0)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_legend_has_one_may_conquer_trigger_that_exhausts_her_with_staged_candidates() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(REVEAL, 2);
    }

    #[test]
    fn conquering_asks_to_exhaust_her_and_the_reveal_parks_on_the_two_faces() {
        let mut fixture = tunnels();
        dig(&mut fixture);
        let ctx = fixture.ctx();
        assert_eq!(deck_of(&ctx), [20, 21], "two left in the deck");
        assert_eq!(revealing(&ctx, 0), [TOP, SECOND]);
    }

    #[test]
    fn a_revealed_unit_is_played_for_its_printed_cost_where_the_seat_picks_and_the_rest_is_recycled(
    ) {
        let mut fixture = tunnels();
        dig(&mut fixture);
        let scripts = [(TOP, &CRAB), (SECOND, &PUMP)];
        arrives(&mut fixture, TOP, crab_face(2, 0), &scripts);
        arrives(&mut fixture, SECOND, pump_face(6, 0), &scripts);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: CHOOSE
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {REKSAI}}}: choose {QUESTION} (0 of 1)")
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {TOP}}}"), "skip".to_string()],
            "the six-energy Pump is out of reach, the Crab is offered"
        );
        assert_eq!(playable_revealed(&ctx, 0), [TOP]);
        fixtures::choose(&mut ctx, 0, &format!("{{card {TOP}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: LOCATE
            }),
            "a unit with two locations asks where"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ],
            "the base and the battlefield just conquered · the Sprite holds the other"
        );
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 2,
            "the Crab's two energy are paid"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(TOP),
            Some(Location::Battlefield(fixtures::BF1)),
            "{:?}",
            ctx.blob.log
        );
        assert!(
            ctx.card(TOP).unwrap().exhausted,
            "369.3 · it enters exhausted"
        );
        assert!(!ctx.has_flag(TOP, FLAG_REVEALING));
        assert_eq!(deck_of(&ctx), [SECOND, 20, 21], "the Pump went under");
        assert!(!ctx.has_flag(SECOND, FLAG_REVEALING));
        assert!(ctx.state_of(SECOND).is_none(), "no state row lingers");
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 1".to_string()));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_revealed_spell_is_played_at_full_cost_asks_its_targets_and_lands_in_the_trash() {
        let mut fixture = tunnels();
        dig(&mut fixture);
        let scripts = [(TOP, &PUMP), (SECOND, &CRAB)];
        arrives(&mut fixture, TOP, pump_face(1, 1), &scripts);
        arrives(&mut fixture, SECOND, crab_face(9, 0), &scripts);
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {TOP}}}"), "skip".to_string()]
        );
        let runes = ctx.runes_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TOP}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 2, spec: 0 }),
            "the Pump asks for its unit: {:?}",
            ctx.blob.log
        );
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "its Fury power recycled a rune"
        );
        assert_eq!(
            deck_of(&ctx).first(),
            Some(&SECOND),
            "the Crab went under first"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].origin, Origin::Banishment);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(
            ctx.card(TOP).unwrap().zone,
            Some(fixtures::TRASH),
            "the resolved spell is trashed"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_recycles_both_and_two_unplayable_cards_are_recycled_without_asking() {
        let mut fixture = tunnels();
        dig(&mut fixture);
        let crabs = [(TOP, &CRAB), (SECOND, &CRAB)];
        arrives(&mut fixture, TOP, crab_face(2, 0), &crabs);
        arrives(&mut fixture, SECOND, crab_face(1, 0), &crabs);
        let mut ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {TOP}}}"),
                format!("{{card {SECOND}}}"),
                "skip".to_string()
            ]
        );
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "nothing is paid");
        assert_eq!(
            deck_of(&ctx),
            [SECOND, TOP, 20, 21],
            "recycled in reveal order"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} plays none of the revealed cards".to_string()));
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut broke = tunnels();
        dig(&mut broke);
        let scripts = [(TOP, &CRAB), (SECOND, &PUMP)];
        arrives(&mut broke, TOP, crab_face(9, 0), &scripts);
        arrives(&mut broke, SECOND, pump_face(9, 0), &scripts);
        let ctx = broke.ctx();
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing affordable, nothing to ask"
        );
        assert_eq!(deck_of(&ctx), [SECOND, TOP, 20, 21]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} can play none of the revealed cards".to_string()));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn a_spell_without_a_legal_target_is_not_offered() {
        let mut fixture = tunnels();
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id));
        fixture.table.tokens.clear();
        fixture.resolve();
        dig(&mut fixture);
        let scripts = [(TOP, &ZAP), (SECOND, &CRAB)];
        arrives(
            &mut fixture,
            TOP,
            Face::named("Zap")
                .with_kind(KIND_SPELL)
                .with_cost(Some(0), Some(0))
                .with_domain(vec!["Fury".into()]),
            &scripts,
        );
        arrives(&mut fixture, SECOND, crab_face(1, 0), &scripts);
        let ctx = fixture.ctx();
        assert!(!playable(&ctx, 0, TOP), "no enemy unit anywhere to zap");
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {SECOND}}}"), "skip".to_string()]
        );
    }

    #[test]
    fn declining_the_exhaust_an_exhausted_reksai_and_the_opponents_conquer_reveal_nothing() {
        let mut fixture = tunnels();
        conquer(&mut fixture);
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(REKSAI).unwrap().exhausted);
        assert_eq!(deck_of(&ctx), [20, 21, 22, 23]);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {REKSAI}}} trigger is removed · its cost is declined"
        )));
        drop(ctx);
        let mut spent = tunnels();
        spent.table.card_mut(REKSAI).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        cleanup::establish(&mut ctx, fixtures::BF1);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no question for a cost she cannot pay"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {REKSAI}}} trigger is removed · its source is exhausted"
        )));
        drop(ctx);
        let mut theirs = Fixture::enforced();
        theirs.table.card_mut(REKSAI).unwrap().name = CARD.name.into();
        theirs.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        theirs.blob.set_contested(fixtures::BF1, Some(1));
        theirs.resolve();
        let mut ctx = theirs.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(1)
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "seat 1 conquering is not hers");
    }

    #[test]
    fn an_empty_deck_reveals_nothing_and_a_single_card_is_revealed_alone() {
        let mut fixture = tunnels();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        fixture.resolve();
        conquer(&mut fixture);
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to reveal".to_string()));
        drop(ctx);
        let mut one = tunnels();
        one.table.cards.retain(|card| {
            card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0 || card.id == TOP
        });
        one.resolve();
        conquer(&mut one);
        let mut ctx = one.ctx();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        pass_until_parked(&mut ctx);
        assert_eq!(ctx.blob.chain[0].awaiting, [TOP]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals the top 1 cards of their deck".to_string()));
    }
}
