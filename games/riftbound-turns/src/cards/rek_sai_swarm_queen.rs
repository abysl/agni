use super::prelude::{
    asking, done, forget_revealing, on_attack, optional, target, unit, with_candidates,
    zone_target, LimitedPlay, Location, Price,
};
use super::teemo_strategist::reveal_top_cards;
use super::{Card, Filter, Flow, Item, Paying, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::{cost, pay};
use crate::state::{
    ChainItem, Limited, Origin, Price as StatePrice, RevealedFrom, TargetRef, FLAG_REVEALING,
};

pub const REVEAL: usize = 2;
pub const QUESTION: &str = "your Main Deck to reveal the top two of, then one to play";
pub const CONFIRM: u8 = 1;
pub const REVEALED: u8 = 2;
pub const PICK: u8 = 3;
pub const LOCATE: u8 = 4;
pub const HERE: TargetSpec = target(Filter::Here, 1, 1, TargetKind::Zone, "here");

pub fn origin_of_a_revealed_play() -> Origin {
    Origin::Revealed {
        from: RevealedFrom::Deck,
    }
}

fn here(ctx: &Ctx, item: &Item) -> Option<Location> {
    let zone = zone_target(item, 0)?;
    ctx.zones
        .is_battlefield(zone)
        .then_some(Location::Battlefield(zone))
}

pub fn play_locations_with_here(ctx: &Ctx, seat: u8, here: Option<Location>) -> Vec<Location> {
    let mut locations = ctx.play_locations(seat);
    if let Some(here) = here {
        if !locations.contains(&here) {
            locations.push(here);
        }
    }
    locations
}

pub fn revealed(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let Some(chain) = ctx.zones.chain else {
        return Vec::new();
    };
    ctx.table
        .held(chain, 0)
        .map(|card| card.id)
        .filter(|card| ctx.has_flag(*card, FLAG_REVEALING) && ctx.owner(*card) == seat)
        .collect()
}

fn recycle_rest(ctx: &mut Ctx, seat: u8, cards: &[u32]) {
    for card in cards {
        forget_revealing(ctx, *card);
        ctx.recycle_to_bottom(*card);
    }
    if !cards.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", cards.len()));
    }
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        CONFIRM => ctx
            .zones
            .main_deck
            .map(|deck| vec![TargetRef::Zone(deck)])
            .unwrap_or_default(),
        PICK => playable_revealed(ctx, item.controller)
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        LOCATE => play_locations_with_here(ctx, item.controller, here(ctx, item))
            .into_iter()
            .filter_map(|at| ctx.zone_of(at))
            .map(|(zone, _)| TargetRef::Zone(zone))
            .collect(),
        _ => Vec::new(),
    }
}

pub fn playable_revealed(ctx: &Ctx, seat: u8) -> Vec<u32> {
    revealed(ctx, seat)
        .into_iter()
        .filter(|card| {
            let item = priced_revealed_item(ctx, seat, *card, &ctx.play_locations(seat));
            pay::affordable_for(
                ctx,
                seat,
                &cost::of_item(ctx, &item, None),
                Paying::Item(&item),
            ) && crate::engine::targets::first_spec_fillable(ctx, &item)
        })
        .collect()
}

fn priced_revealed_item(ctx: &Ctx, seat: u8, card: u32, locations: &[Location]) -> ChainItem {
    let mut item = cost::play_item(ctx, seat, card, origin_of_a_revealed_play());
    item.limited = Some(Limited {
        zones: locations
            .iter()
            .filter_map(|at| ctx.zone_of(*at).map(|(zone, _)| zone))
            .collect(),
        price: StatePrice::Printed,
    });
    item
}

fn play_revealed_at(ctx: &mut Ctx, item: &Item, card: u32, locations: &[Location]) -> bool {
    let seat = item.controller;
    if ctx.is_unit(card) && locations.is_empty() {
        ctx.narrate(format!(
            "{{card {card}}} stays revealed · nowhere it can be played"
        ));
        return false;
    }
    let quoted = priced_revealed_item(ctx, seat, card, locations);
    let price = cost::of_item(ctx, &quoted, None);
    if !pay::affordable_for(ctx, seat, &price, Paying::Item(&quoted)) {
        ctx.narrate(format!(
            "{{card {card}}} stays revealed · its cost can't be paid"
        ));
        return false;
    }
    forget_revealing(ctx, card);
    ctx.narrate(format!(
        "{{seat {seat}}} plays the revealed {{card {card}}} for {}",
        price.label()
    ));
    ctx.play_limited(LimitedPlay {
        card,
        by: seat,
        origin: origin_of_a_revealed_play(),
        locations: locations.to_vec(),
        price: Price::Printed,
    })
    .is_ok()
}

pub fn play_revealed_here(ctx: &mut Ctx, item: &Item, card: u32) -> bool {
    let seat = item.controller;
    let locations = ctx.limited_play_locations(
        seat,
        card,
        &play_locations_with_here(ctx, seat, here(ctx, item)),
    );
    play_revealed_at(ctx, item, card, &locations)
}

fn play_and_recycle(ctx: &mut Ctx, item: &Item, card: u32) -> Flow {
    let seat = item.controller;
    let rest: Vec<u32> = revealed(ctx, seat)
        .into_iter()
        .filter(|held| *held != card)
        .collect();
    if !play_revealed_here(ctx, item, card) {
        recycle_rest(ctx, seat, &[card]);
    }
    recycle_rest(ctx, seat, &rest);
    done()
}

fn burrow(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    match stage.0 {
        LOCATE => {
            let Some(card) = item.targets.iter().find_map(|target| match target {
                TargetRef::Card(card) => Some(*card),
                TargetRef::Zone(_) | TargetRef::Seat(_) | TargetRef::Item(_) => None,
            }) else {
                return done();
            };
            let Some(at) = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            else {
                return done();
            };
            let rest: Vec<u32> = revealed(ctx, seat)
                .into_iter()
                .filter(|held| *held != card)
                .collect();
            let locations = ctx.limited_play_locations(seat, card, &[at]);
            if locations.is_empty() || !play_revealed_at(ctx, item, card, &locations) {
                recycle_rest(ctx, seat, &[card]);
            }
            recycle_rest(ctx, seat, &rest);
            done()
        }
        CONFIRM => {
            let deck = ctx.zones.main_deck.map(u32::from);
            if ctx.picks().first().copied() != deck {
                ctx.narrate(format!("{{card {me}}} reveals nothing"));
                return done();
            }
            let top = reveal_top_cards(ctx, seat, REVEAL);
            if top.is_empty() {
                ctx.narrate(format!("{{seat {seat}}} has no cards left to reveal"));
                return done();
            }
            let unseen: Vec<u32> = top
                .iter()
                .copied()
                .filter(|card| !ctx.table.is_revealed(*card))
                .collect();
            if unseen.is_empty() {
                return burrow(ctx, item, Stage(REVEALED));
            }
            Flow::Ask(ctx.await_faces(item, &unseen, REVEALED))
        }
        REVEALED => {
            let top = revealed(ctx, seat);
            if top.is_empty() {
                return done();
            }
            if playable_revealed(ctx, seat).is_empty() {
                ctx.narrate(format!(
                    "{{seat {seat}}} can play none of the revealed cards"
                ));
                recycle_rest(ctx, seat, &top);
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, PICK, 0, 1))
        }
        PICK => {
            let top = revealed(ctx, seat);
            let Some(card) = ctx
                .picks()
                .first()
                .copied()
                .filter(|card| playable_revealed(ctx, seat).contains(card))
            else {
                ctx.narrate(format!("{{seat {seat}}} plays nothing"));
                recycle_rest(ctx, seat, &top);
                return done();
            };
            play_and_recycle(ctx, item, card)
        }
        _ => {
            if ctx
                .zones
                .main_deck
                .is_none_or(|deck| ctx.top_of(deck, seat, 1).is_empty())
            {
                ctx.narrate(format!("{{seat {seat}}} has no cards left to reveal"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, CONFIRM, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Rek'Sai - Swarm Queen",
    &[],
    &[asking(
        with_candidates(optional(on_attack(&[HERE], burrow)), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::rek_sai_void_burrower::playable;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who, KIND_SPELL, KIND_UNIT};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, cleanup, prompts, settle};
    use crate::state::{GameBlob, ItemKind, ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::cbor::{Item as CborItem, Reader, Writer};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::Face;

    const REK_SAI: u32 = 90;
    const TOP: u32 = 23;
    const SECOND: u32 = 22;

    static ORPHAN: Card = crate::cards::prelude::spell(
        "Orphan",
        &[],
        &[crate::cards::prelude::play(
            &[crate::cards::prelude::a_card(
                Filter::Named("Nobody"),
                "nobody",
            )],
            |_, _, _| done(),
        )],
    );

    fn tunnels() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut rek_sai = fixtures::unit(REK_SAI, fixtures::BF1, 0, "Rek'Sai - Swarm Queen", 5);
        rek_sai.domain = vec!["Order".into()];
        rek_sai.energy = Some(5);
        rek_sai.power = Some(1);
        fixture.table.cards.push(rek_sai);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(REK_SAI).unwrap(),
            &CARD
        ));
        fixture
    }

    fn unit_face(energy: u8) -> Face {
        Face::named("Crab")
            .with_kind(KIND_UNIT)
            .with_might(Some(2))
            .with_cost(Some(energy), None)
            .with_domain(vec!["Fury".into()])
    }

    fn spell_face() -> Face {
        Face::named("Spark")
            .with_kind(KIND_SPELL)
            .with_cost(Some(1), None)
            .with_domain(vec!["Fury".into()])
    }

    fn attacks(fixture: &mut Fixture) {
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        assert!(ctx.is_attacker(REK_SAI));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Zone(fixtures::BF1)],
            "her battlefield is locked as the trigger goes on the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: CONFIRM
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::MAIN_DECK),
                "skip".to_string()
            ]
        );
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn choose(fixture: &mut Fixture, label: &str) {
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, label).unwrap();
        pass_until_parked(&mut ctx);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face) {
        let action = Action::Reveal { card, face };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn reveal_both(fixture: &mut Fixture, top: Face, second: Face) {
        choose(fixture, &format!("{{zone {}}}", fixtures::MAIN_DECK));
        {
            let ctx = fixture.ctx();
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
        }
        arrives(fixture, TOP, top);
        arrives(fixture, SECOND, second);
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
            [
                format!("{{card {TOP}}}"),
                format!("{{card {SECOND}}}"),
                "skip".to_string()
            ]
        );
    }

    fn deck_of(ctx: &Ctx) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, 0)
            .map(|card| card.id)
            .collect()
    }

    fn legacy_v11(bytes: &[u8]) -> Vec<u8> {
        fn raw_value<'a>(reader: &mut Reader<'a>, bytes: &'a [u8]) -> &'a [u8] {
            let start = reader.position();
            reader.skip().unwrap();
            &bytes[start..reader.position()]
        }
        fn item<'a>(reader: &mut Reader<'a>, bytes: &'a [u8], writer: &mut Writer) {
            assert_eq!(reader.array_len(), Some(14));
            writer.array(13);
            for _ in 0..13 {
                writer.raw(raw_value(reader, bytes));
            }
            reader.skip().unwrap();
        }
        fn row<'a>(
            reader: &mut Reader<'a>,
            bytes: &'a [u8],
            writer: &mut Writer,
            len: usize,
            old: usize,
        ) {
            assert_eq!(reader.array_len(), Some(len));
            writer.array(old);
            for _ in 0..old {
                writer.raw(raw_value(reader, bytes));
            }
            for _ in old..len {
                reader.skip().unwrap();
            }
        }
        let mut reader = Reader::new(bytes);
        let mut writer = Writer::new();
        let fields = reader.map_len().unwrap();
        writer.map(fields);
        for _ in 0..fields {
            let start = reader.position();
            let key = match reader.item().unwrap() {
                CborItem::Text(key) => key,
                _ => panic!("non-text blob key"),
            };
            writer.raw(&bytes[start..reader.position()]);
            match key {
                "v" => {
                    reader.skip().unwrap();
                    writer.unsigned(11);
                }
                "ch" => {
                    let count = reader.array_len().unwrap();
                    writer.array(count);
                    for _ in 0..count {
                        item(&mut reader, bytes, &mut writer);
                    }
                }
                "q" => {
                    let count = reader.array_len().unwrap();
                    writer.array(count);
                    for _ in 0..count {
                        assert_eq!(reader.array_len(), Some(2));
                        writer.array(2);
                        item(&mut reader, bytes, &mut writer);
                        writer.raw(raw_value(&mut reader, bytes));
                    }
                }
                "s" => {
                    let count = reader.array_len().unwrap();
                    writer.array(count);
                    for _ in 0..count {
                        row(&mut reader, bytes, &mut writer, 16, 14);
                    }
                }
                "c" => {
                    let count = reader.array_len().unwrap();
                    writer.array(count);
                    for _ in 0..count {
                        row(&mut reader, bytes, &mut writer, 15, 13);
                    }
                }
                "pv" => {
                    let count = reader.array_len().unwrap();
                    writer.array(count);
                    for _ in 0..count {
                        assert_eq!(reader.array_len(), Some(4));
                        writer.array(3);
                        reader.skip().unwrap();
                        for _ in 0..3 {
                            writer.raw(raw_value(&mut reader, bytes));
                        }
                    }
                }
                _ => writer.raw(raw_value(&mut reader, bytes)),
            }
        }
        writer.finish()
    }

    #[test]
    fn the_script_is_a_unit_with_an_optional_attack_trigger_that_locks_here_and_asks_in_stages() {
        assert!(std::ptr::eq(
            script_of("Rek'Sai - Swarm Queen").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.optional);
        assert_eq!(ability.targets, &[HERE]);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = tunnels();
        let ctx = fixture.ctx();
        assert_eq!(
            play_locations_with_here(&ctx, 0, Some(Location::Battlefield(fixtures::BF1))),
            [Location::Base(0), Location::Battlefield(fixtures::BF1)]
        );
        assert_eq!(play_locations_with_here(&ctx, 0, None), [Location::Base(0)]);
    }

    #[test]
    fn a_revealed_unit_is_offered_her_battlefield_paid_for_and_played_there_and_the_other_recycled()
    {
        let mut fixture = tunnels();
        attacks(&mut fixture);
        reveal_both(&mut fixture, unit_face(2), spell_face());
        let ready = fixture.ctx().ready_runes_of(0).len();
        choose(&mut fixture, &format!("{{card {TOP}}}"));
        {
            let ctx = fixture.ctx();
            assert!(
                matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })),
                "a unit may be played here, so where is asked"
            );
            assert_eq!(
                fixtures::labels(&ctx),
                [
                    format!("{{zone {}}}", fixtures::BASE),
                    format!("{{zone {}}}", fixtures::BF1)
                ],
                "her base and here"
            );
            let prompt = ctx.blob.prompt.as_ref().unwrap().id;
            let mut ctx = ctx;
            assert_eq!(
                prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
                Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
            );
        }
        choose(&mut fixture, &format!("{{zone {}}}", fixtures::BF1));
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.location(TOP),
            Some(Location::Battlefield(fixtures::BF1)),
            "{:?}",
            ctx.blob.log
        );
        assert!(ctx.card(TOP).unwrap().exhausted);
        assert!(ctx.is_attacker(TOP), "she brought a friend to the fight");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 2,
            "the unit is paid for in full"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays the revealed {{card {TOP}}} for 2 energy"
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {TOP}}} to {{zone {}}}",
            fixtures::BF1
        )));
        assert_eq!(ctx.card(SECOND).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert_eq!(deck_of(&ctx), [SECOND, 20, 21], "the rest under the deck");
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 1".to_string()));
        assert!(!ctx.has_flag(TOP, FLAG_REVEALING));
        assert!(!ctx.has_flag(SECOND, FLAG_REVEALING));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_revealed_spell_is_played_onto_the_chain_and_declining_the_reveal_shows_nothing() {
        let mut fixture = tunnels();
        attacks(&mut fixture);
        reveal_both(&mut fixture, unit_face(2), spell_face());
        let ready = fixture.ctx().ready_runes_of(0).len();
        {
            let mut ctx = fixture.ctx();
            fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND}}}")).unwrap();
            assert!(
                ctx.blob.chain.iter().any(|held| matches!(
                    held.kind,
                    ItemKind::Spell { card } if card == SECOND
                )),
                "the spell is played onto the chain: {:?}",
                ctx.blob.chain
            );
            assert_eq!(ctx.card(SECOND).unwrap().zone, Some(fixtures::CHAIN));
            assert_eq!(ctx.ready_runes_of(0).len(), ready - 1, "paid in full");
            assert!(ctx.blob.log.contains(&format!(
                "{{seat 0}} plays the revealed {{card {SECOND}}} for 1 energy"
            )));
            pass_until_parked(&mut ctx);
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.card(SECOND).unwrap().zone,
            Some(fixtures::TRASH),
            "resolved as priority passed"
        );
        assert_eq!(deck_of(&ctx), [TOP, 20, 21]);
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 1".to_string()));
        drop(ctx);

        let mut fixture = tunnels();
        attacks(&mut fixture);
        choose(&mut fixture, "skip");
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx), [20, 21, SECOND, TOP], "untouched");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {REK_SAI}}} reveals nothing")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_saved_legacy_location_resume_plays_the_selected_card_and_recycles_the_rest() {
        let mut fixture = tunnels();
        attacks(&mut fixture);
        reveal_both(&mut fixture, unit_face(2), unit_face(1));
        {
            let mut ctx = fixture.ctx();
            let mut item = ctx.blob.chain[0].clone();
            item.stage = LOCATE;
            item.targets.push(TargetRef::Card(SECOND));
            ctx.blob.chain[0] = item.clone();
            ctx.ask_resume(&item, LOCATE, 1, 1);
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        fixture.blob = GameBlob::decode(&legacy_v11(&fixture.blob.encode())).expect("v11 reload");
        choose(&mut fixture, &format!("{{zone {}}}", fixtures::BF1));
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.location(SECOND),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.card(TOP).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert!(!ctx.has_flag(TOP, FLAG_REVEALING));
        assert!(!ctx.has_flag(SECOND, FLAG_REVEALING));
        assert_eq!(deck_of(&ctx), [TOP, 20, 21]);
    }

    #[test]
    fn skipping_the_play_recycles_both_and_an_unpayable_pick_is_recycled_too() {
        let mut fixture = tunnels();
        attacks(&mut fixture);
        reveal_both(&mut fixture, unit_face(2), spell_face());
        choose(&mut fixture, "skip");
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            deck_of(&ctx),
            [SECOND, TOP, 20, 21],
            "both under the deck in the order they were revealed"
        );
        assert!(ctx.blob.log.contains(&"{seat 0} plays nothing".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} recycles 2".to_string()));
        drop(ctx);

        let mut fixture = tunnels();
        attacks(&mut fixture);
        choose(&mut fixture, &format!("{{zone {}}}", fixtures::MAIN_DECK));
        arrives(&mut fixture, TOP, unit_face(9));
        arrives(&mut fixture, SECOND, spell_face());
        {
            let ctx = fixture.ctx();
            assert!(!playable(&ctx, 0, TOP), "nine energy is out of reach");
            assert_eq!(
                fixtures::labels(&ctx),
                [format!("{{card {SECOND}}}"), "skip".to_string()],
                "an unpayable card is not offered"
            );
        }
        choose(&mut fixture, "skip");
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck_of(&ctx), [SECOND, TOP, 20, 21]);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_revealed_spell_without_a_legal_target_is_not_offered_and_both_are_recycled() {
        let mut fixture = tunnels();
        attacks(&mut fixture);
        choose(&mut fixture, &format!("{{zone {}}}", fixtures::MAIN_DECK));
        fixture.scripts = fixture.scripts.clone().with_script(TOP, &ORPHAN);
        arrives(
            &mut fixture,
            TOP,
            Face::named("Orphan")
                .with_kind(KIND_SPELL)
                .with_cost(Some(1), None)
                .with_domain(vec!["Fury".into()]),
        );
        fixture.scripts = fixture.scripts.clone().with_script(TOP, &ORPHAN);
        arrives(&mut fixture, SECOND, unit_face(9));
        fixture.scripts = fixture.scripts.clone().with_script(TOP, &ORPHAN);
        let runes = fixture.ctx().ready_runes_of(0).len();
        {
            let ctx = fixture.ctx();
            assert!(!playable(&ctx, 0, TOP), "nobody to target");
            assert!(ctx.blob.prompt.is_none(), "{:?}", fixtures::labels(&ctx));
            assert!(ctx.blob.chain.is_empty());
            assert_eq!(ctx.ready_runes_of(0).len(), runes, "nothing was paid");
            assert_eq!(deck_of(&ctx), [SECOND, TOP, 20, 21], "both recycled");
            assert_eq!(ctx.card(TOP).unwrap().zone, Some(fixtures::MAIN_DECK));
            assert!(ctx
                .blob
                .log
                .contains(&"{seat 0} can play none of the revealed cards".to_string()));
            assert!(ctx.fault.is_none());
        }
    }

    #[test]
    fn a_one_card_deck_reveals_what_there_is_and_an_empty_one_nothing() {
        let mut fixture = tunnels();
        fixture
            .table
            .cards
            .retain(|card| ![20, 21, SECOND].contains(&card.id));
        fixture.resolve();
        attacks(&mut fixture);
        choose(&mut fixture, &format!("{{zone {}}}", fixtures::MAIN_DECK));
        {
            let ctx = fixture.ctx();
            assert_eq!(ctx.blob.chain[0].awaiting, [TOP]);
        }
        arrives(&mut fixture, TOP, unit_face(2));
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {TOP}}}"), "skip".to_string()]
        );
        drop(ctx);

        let mut empty = tunnels();
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        empty.resolve();
        empty.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = empty.ctx();
        cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to reveal".to_string()));
    }

    #[test]
    fn the_played_card_reports_the_reveal_as_its_origin() {
        let mut fixture = tunnels();
        attacks(&mut fixture);
        reveal_both(&mut fixture, unit_face(2), spell_face());
        choose(&mut fixture, &format!("{{card {TOP}}}"));
        choose(&mut fixture, &format!("{{zone {}}}", fixtures::BASE));
        let mut ctx = fixture.ctx();
        fixtures::pass_until_open(&mut ctx);
        assert_ne!(origin_of_a_revealed_play(), Origin::Banishment);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("plays {card 23}")));
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::Played {
                card: TOP,
                origin: Origin::Banishment,
                ..
            }
        )));
    }

    #[test]
    fn the_engine_serves_the_location_prompt_with_here_added() {
        let mut fixture = tunnels();
        attacks(&mut fixture);
        reveal_both(&mut fixture, unit_face(2), spell_face());
        choose(&mut fixture, &format!("{{card {TOP}}}"));
        let ctx = fixture.ctx();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ]
        );
    }
}
