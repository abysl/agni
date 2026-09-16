use super::ivern_nurturer::revealing;
use super::prelude::{
    asking, banish_by, done, forget_revealing, play, remember_card, remembered_cards, spell,
    with_candidates, Location,
};
use super::rek_sai_void_burrower::{play_revealed_for, playable_for};
use super::{Card, Flow, Item, Stage, KIND_GEAR, KIND_UNIT};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{Origin, TargetRef, FLAG_REVEALING};
use agni_plugin_sdk::decide::{Effect, TOP};

pub const LOOK: usize = 5;
pub const DISCOUNT: u8 = 5;
pub const QUESTION: &str =
    "a unit or gear among the top five to banish and play for 5 less, where it is played, then whether to Empower it";
pub const PICK: u8 = 1;
pub const REVEALED: u8 = 2;
pub const LOCATE: u8 = 3;
pub const EMPOWER: u8 = 4;

pub fn reduced_cost(ctx: &Ctx, card: u32) -> cost::Cost {
    let printed = cost::printed_of(ctx, card, false);
    cost::Cost {
        energy: printed.energy.saturating_sub(DISCOUNT),
        ..printed
    }
}

pub fn may_be_unit_or_gear(ctx: &Ctx, card: u32) -> bool {
    ctx.kind_of(card)
        .is_none_or(|kind| kind == KIND_UNIT || kind == KIND_GEAR)
}

fn in_deck(ctx: &Ctx, seat: u8, card: u32) -> bool {
    ctx.zones
        .main_deck
        .is_some_and(|deck| ctx.table.held(deck, seat).any(|held| held.id == card))
}

pub fn looked(item: &Item) -> Vec<u32> {
    remembered_cards(item)
}

fn still_on_top(ctx: &Ctx, item: &Item) -> Vec<u32> {
    looked(item)
        .into_iter()
        .filter(|card| in_deck(ctx, item.controller, *card))
        .collect()
}

fn offered(ctx: &Ctx, item: &Item) -> Vec<u32> {
    still_on_top(ctx, item)
        .into_iter()
        .filter(|card| may_be_unit_or_gear(ctx, *card))
        .collect()
}

fn revealed(ctx: &Ctx, item: &Item) -> Option<u32> {
    revealing(ctx, item.controller).filter(|card| looked(item).contains(card))
}

pub fn played(ctx: &Ctx, item: &Item) -> Option<u32> {
    let seat = item.controller;
    looked(item)
        .into_iter()
        .find(|card| !in_deck(ctx, seat, *card) && !ctx.has_flag(*card, FLAG_REVEALING))
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
        PICK => offered(ctx, item)
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        LOCATE => location_options(ctx, item.controller),
        EMPOWER => played(ctx, item).map(TargetRef::Card).into_iter().collect(),
        _ => Vec::new(),
    }
}

fn recycle_all(ctx: &mut Ctx, seat: u8, cards: &[u32]) {
    for card in cards {
        ctx.recycle_to_bottom(*card);
    }
    if !cards.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", cards.len()));
    }
}

fn recycle_the_rest(ctx: &mut Ctx, item: &Item, except: Option<u32>) {
    let rest: Vec<u32> = still_on_top(ctx, item)
        .into_iter()
        .filter(|card| Some(*card) != except)
        .collect();
    recycle_all(ctx, item.controller, &rest);
}

fn look(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = ctx
        .zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default();
    if top.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no cards left to look at"));
        return done();
    }
    for card in &top {
        ctx.peek(*card, seat);
        remember_card(ctx, *card);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} looks at the top {} cards of their deck",
        top.len()
    ));
    Flow::Ask(ctx.ask_resume(item, PICK, 0, 1))
}

fn reveal_from_the_deck(ctx: &mut Ctx, seat: u8, card: u32) -> bool {
    let Some(chain) = ctx.zones.chain else {
        return false;
    };
    ctx.emit(Effect::Move {
        card,
        zone: chain,
        seat: 0,
        index: TOP,
    });
    ctx.set_flag(card, FLAG_REVEALING, true);
    ctx.reveal(card);
    ctx.narrate(format!("{{seat {seat}}} reveals {{card {card}}}"));
    true
}

fn pick(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let Some(card) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| offered(ctx, item).contains(card))
    else {
        ctx.narrate(format!("{{seat {seat}}} plays none of them"));
        recycle_the_rest(ctx, item, None);
        return done();
    };
    if !reveal_from_the_deck(ctx, seat, card) {
        recycle_the_rest(ctx, item, None);
        return done();
    }
    if ctx.kind_of(card).is_some() {
        return arrived(ctx, item);
    }
    Flow::Ask(ctx.await_faces(item, &[card], REVEALED))
}

fn recycle_the_revealed(ctx: &mut Ctx, card: u32) {
    forget_revealing(ctx, card);
    ctx.recycle_to_bottom(card);
    ctx.narrate(format!(
        "{{card {card}}} can't be played · it is recycled with the rest"
    ));
}

fn arrived(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let Some(card) = revealed(ctx, item) else {
        recycle_the_rest(ctx, item, None);
        return done();
    };
    let playable =
        (ctx.is_unit(card) || ctx.is_gear(card)) && playable_for(ctx, seat, card, reduced_cost);
    if !playable {
        recycle_the_revealed(ctx, card);
        recycle_the_rest(ctx, item, Some(card));
        return done();
    }
    if ctx.is_unit(card) {
        let locations = ctx.play_locations(seat);
        if locations.len() > 1 {
            return Flow::Ask(ctx.ask_resume(item, LOCATE, 1, 1));
        }
        return banish_and_play(ctx, item, card, locations.first().copied());
    }
    banish_and_play(ctx, item, card, None)
}

fn locate(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let Some(card) = revealed(ctx, item) else {
        recycle_the_rest(ctx, item, None);
        return done();
    };
    let at = ctx
        .picks()
        .first()
        .and_then(|zone| u16::try_from(*zone).ok())
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .filter(|at| ctx.play_locations(seat).contains(at))
        .unwrap_or(Location::Base(seat));
    banish_and_play(ctx, item, card, Some(at))
}

fn banish_and_play(ctx: &mut Ctx, item: &Item, card: u32, at: Option<Location>) -> Flow {
    let seat = item.controller;
    forget_revealing(ctx, card);
    banish_by(ctx, card, seat);
    let played = play_revealed_for(ctx, seat, card, reduced_cost, Origin::Banishment, at);
    recycle_the_rest(ctx, item, Some(card));
    if !played {
        ctx.recycle_to_bottom(card);
        ctx.narrate(format!("{{card {card}}} is recycled instead"));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, EMPOWER, 0, 1))
}

pub fn empower_the_played_card(ctx: &mut Ctx, card: u32) -> bool {
    let by = ctx.controller(card);
    empower_the_played_card_by(ctx, card, by)
}

pub fn empower_the_played_card_by(ctx: &mut Ctx, card: u32, by: u8) -> bool {
    if ctx.empower_by(card, by) {
        ctx.narrate(format!("{{card {card}}} is Empowered"));
        return true;
    }
    ctx.narrate(format!("{{card {card}}} is already Empowered"));
    false
}

fn empower_it(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    match (ctx.picks().first().copied(), played(ctx, item)) {
        (Some(picked), Some(card)) if picked == card => {
            empower_the_played_card_by(ctx, card, item.controller);
        }
        (_, Some(card)) => ctx.narrate(format!("{{seat {seat}}} leaves {{card {card}}} as it is")),
        _ => {}
    }
    done()
}

fn claw(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        PICK => pick(ctx, item),
        REVEALED => arrived(ctx, item),
        LOCATE => locate(ctx, item),
        EMPOWER => empower_it(ctx, item),
        _ => look(ctx, item),
    }
}

pub static CARD: Card = spell(
    "Wild Claw",
    &[],
    &[asking(
        with_candidates(play(&[], claw), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{gear as gear_card, unit as unit_card};
    use crate::cards::{script_of, Keyword, Trigger, KIND_SPELL};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, legal, prompts, settle};
    use crate::state::{ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, BOTTOM};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::{CardInfo, Face};

    const CLAW: u32 = 90;
    const THEIR_CLAW: u32 = 91;
    const BODY_RUNE: u32 = 46;
    const FIRST_EXTRA_RUNE: u32 = 100;
    const EXTRA: [u32; 3] = [26, 27, 28];
    const MY_DECK: [u32; 7] = [20, 21, 22, 23, 26, 27, 28];
    const TOP_FIVE: [u32; 5] = [28, 27, 26, 23, 22];
    const CHOSEN: u32 = 26;

    static CRAB: Card = unit_card("Crab", &[], &[]);
    static DISC: Card = gear_card("Disc", &[], &[]);
    static PUMP: Card = spell("Pump", &[], &[]);

    fn claw_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Wild Claw", 2, 1);
        card.domain = vec!["Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(claw_card(CLAW, 0));
        fixture.table.cards.push(claw_card(THEIR_CLAW, 1));
        for id in EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::MAIN_DECK, 0));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        for rune in FIRST_EXTRA_RUNE..FIRST_EXTRA_RUNE + 2 {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
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

    fn crab_face(energy: u8) -> Face {
        Face::named("Crab")
            .with_kind(KIND_UNIT)
            .with_might(Some(4))
            .with_cost(Some(energy), Some(1))
            .with_domain(vec!["Fury".into()])
    }

    fn disc_face(energy: u8) -> Face {
        Face::named("Disc")
            .with_kind(KIND_GEAR)
            .with_cost(Some(energy), Some(1))
            .with_domain(vec!["Fury".into()])
    }

    fn pump_face() -> Face {
        Face::named("Pump")
            .with_kind(KIND_SPELL)
            .with_cost(Some(1), Some(1))
            .with_domain(vec!["Fury".into()])
    }

    fn cast(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CLAW).unwrap();
        assert!(ctx.blob.prompt.is_none(), "the look comes at resolution");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn answer(fixture: &mut Fixture, label: &str) {
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, label).unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face, script: &'static Card) {
        let action = Action::Reveal { card, face };
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
    }

    fn reveal_the_chosen(fixture: &mut Fixture, face: Face, script: &'static Card) {
        cast(fixture);
        {
            let ctx = fixture.ctx();
            assert_eq!(
                fixtures::labels(&ctx),
                [
                    "{card 28}",
                    "{card 27}",
                    "{card 26}",
                    "{card 23}",
                    "{card 22}",
                    "skip"
                ],
                "the top five, top first, and the may"
            );
        }
        answer(fixture, &format!("{{card {CHOSEN}}}"));
        {
            let ctx = fixture.ctx();
            let parked = &ctx.blob.chain[0];
            assert_eq!(parked.status, ItemStatus::Resolving);
            assert_eq!(parked.stage, REVEALED);
            assert_eq!(parked.awaiting, [CHOSEN]);
            assert_eq!(ctx.card(CHOSEN).unwrap().zone, Some(fixtures::CHAIN));
            assert!(ctx.has_flag(CHOSEN, FLAG_REVEALING));
            assert!(ctx.blob.prompt.is_none(), "the host's reveal is awaited");
            assert_eq!(
                deck_of(&ctx, 0),
                [20, 21, 22, 23, 27, 28],
                "the other four wait on top until the play is settled"
            );
        }
        arrives(fixture, CHOSEN, face, script);
    }

    #[test]
    fn the_script_is_a_targetless_sorcery_whose_stages_pick_reveal_locate_then_empower() {
        assert!(std::ptr::eq(script_of("Wild Claw").unwrap(), &CARD));
        assert_eq!(CARD.name, "Wild Claw");
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!((LOOK, DISCOUNT), (5, 5));
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(7);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(reduced_cost(&ctx, fixtures::HAND_UNIT).energy, 2);
        let spark = reduced_cost(&ctx, fixtures::HAND_SPELL);
        assert_eq!(
            spark.energy, 0,
            "a cost under five goes to zero, never below"
        );
        assert_eq!(
            spark.power,
            cost::printed_of(&ctx, fixtures::HAND_SPELL, false).power,
            "the power is untouched"
        );
        assert!(may_be_unit_or_gear(&ctx, 23), "a hidden card may be either");
        assert!(may_be_unit_or_gear(&ctx, fixtures::HAND_UNIT));
        assert!(may_be_unit_or_gear(&ctx, fixtures::HAND_GEAR));
        assert!(!may_be_unit_or_gear(&ctx, fixtures::HAND_SPELL));
    }

    #[test]
    fn the_look_peeks_five_the_pick_is_revealed_banished_and_played_for_five_less_and_the_rest_recycle(
    ) {
        let mut fixture = armed();
        assert_eq!(deck_of(&fixture.ctx(), 0), MY_DECK.to_vec());
        reveal_the_chosen(&mut fixture, crab_face(6), &CRAB);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: LOCATE
            }),
            "a unit with a held battlefield asks where"
        );
        assert_eq!(fixtures::labels(&ctx), ["{zone 8}", "{zone 9}"]);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 1,
            "6 energy less 5: one rune, which also pays the Fury power"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CHOSEN}}} is banished")));
        assert!(ctx.blob.log.contains(
            &format!("{{seat 0}} plays the revealed {{card {CHOSEN}}} to {{zone 9}} for 1 energy and 1 Fury power")
        ));
        assert!(!ctx.has_flag(CHOSEN, FLAG_REVEALING));
        assert_eq!(
            ctx.location(CHOSEN),
            Some(Location::Battlefield(fixtures::BF1)),
            "419.3 · the play is finalized as part of the resolution: {:?}",
            ctx.blob.log
        );
        assert!(ctx.card(CHOSEN).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Banishment, .. } if *card == CHOSEN
        )));
        assert!(
            ctx.banished_of(0).is_empty(),
            "banished, then played from banishment to the board"
        );
        assert_eq!(
            deck_of(&ctx, 0),
            [22, 23, 27, 28, 20, 21],
            "the other four go under in the listed order"
        );
        for card in [28, 27, 23, 22] {
            assert!(ctx.effects.contains(&Effect::Move {
                card,
                zone: fixtures::MAIN_DECK,
                seat: 0,
                index: BOTTOM
            }));
        }
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 4".to_string()));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: EMPOWER
            }),
            "then the may-Empower"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CHOSEN}}}"), "skip".to_string()]
        );
        assert_eq!(played(&ctx, &ctx.blob.chain[0]), Some(CHOSEN));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} leaves {{card {CHOSEN}}} as it is")));
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_empowered(CHOSEN));
        assert_eq!(ctx.card(CLAW).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_gear_plays_to_the_base_without_a_location_question() {
        let mut fixture = armed();
        reveal_the_chosen(&mut fixture, disc_face(7), &DISC);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: EMPOWER
            }),
            "a gear needs no location"
        );
        assert!(ctx.blob.log.contains(
            &"{seat 0} plays the revealed {card 26} for 2 energy and 1 Fury power".to_string()
        ));
        assert_eq!(deck_of(&ctx, 0), [22, 23, 27, 28, 20, 21]);
        assert_eq!(ctx.location(CHOSEN), Some(Location::Base(0)));
        assert!(ctx.is_gear(CHOSEN));
        assert!(!ctx.is_empowered(CHOSEN));
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHOSEN}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.is_empowered(CHOSEN),
            "the may-Empower lands on the gear"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CHOSEN}}} is Empowered")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_revealed_spell_or_an_unaffordable_unit_is_recycled_with_the_rest_and_nothing_is_banished()
    {
        let mut fixture = armed();
        reveal_the_chosen(&mut fixture, pump_face(), &PUMP);
        let ctx = fixture.ctx();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.has_flag(CHOSEN, FLAG_REVEALING));
        assert_eq!(
            deck_of(&ctx, 0),
            [22, 23, 27, 28, CHOSEN, 20, 21],
            "the spell goes under first, then the rest"
        );
        assert!(ctx.banished_of(0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {CHOSEN}}} can't be played · it is recycled with the rest"
        )));
        assert_eq!(ctx.card(CLAW).unwrap().zone, Some(fixtures::TRASH));
        drop(ctx);
        let mut fixture = armed();
        reveal_the_chosen(&mut fixture, crab_face(12), &CRAB);
        let ctx = fixture.ctx();
        assert!(
            ctx.blob.chain.is_empty(),
            "seven after the discount is beyond the pool"
        );
        assert_eq!(deck_of(&ctx, 0), [22, 23, 27, 28, CHOSEN, 20, 21]);
        assert!(ctx.banished_of(0).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_recycles_all_five_a_known_spell_is_not_offered_and_an_empty_deck_looks_at_nothing()
    {
        let mut fixture = armed();
        cast(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {CLAW}}}: choose {QUESTION} (0 of 1)")
        );
        for card in TOP_FIVE {
            assert!(
                ctx.effects.is_empty() || ctx.effects.contains(&Effect::Peek { card, seat: 0 }),
                "the peeks were applied in the earlier decide"
            );
        }
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx, 0), [22, 23, 26, 27, 28, 20, 21]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} plays none of them".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 5".to_string()));
        drop(ctx);
        let mut fixture = armed();
        fixture.table.card_mut(27).unwrap().kind = Some(KIND_SPELL.into());
        fixture.resolve();
        cast(&mut fixture);
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 28}", "{card 26}", "{card 23}", "{card 22}", "skip"],
            "only a card that may still be a unit or gear is offered"
        );
        drop(ctx);
        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.owner != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CLAW).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to look at".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seats_pick_its_turnless_play_and_a_pool_without_body_are_refused() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CLAW)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        cast(&mut fixture);
        let mut ctx = fixture.ctx();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 6 }),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 6,
                count: 6
            }))
        );
        ctx.picked = vec![21];
        let item = ctx.blob.chain[0].clone();
        assert_eq!(
            pick(&mut ctx, &item),
            Flow::Done,
            "a pick outside the top five plays nothing"
        );
        assert_eq!(deck_of(&ctx, 0), [22, 23, 26, 27, 28, 20, 21]);
        drop(ctx);
        let mut poor = armed();
        poor.table.card_mut(BODY_RUNE).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, CLAW)),
            Err(Refusal::NoPowerOf)
        );
    }

    #[test]
    fn choosing_to_empower_leaves_the_played_unit_empowered_and_a_second_empower_is_nothing() {
        let mut fixture = armed();
        reveal_the_chosen(&mut fixture, crab_face(6), &CRAB);
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(ctx.on_board(CHOSEN));
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHOSEN}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_empowered(CHOSEN));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CHOSEN}}} is Empowered")));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Empowered { card, .. } if *card == CHOSEN)));
        assert!(
            !empower_the_played_card(&mut ctx, CHOSEN),
            "441.1.c · an Empowered unit can't be Empowered again"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CHOSEN}}} is already Empowered")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }
}
