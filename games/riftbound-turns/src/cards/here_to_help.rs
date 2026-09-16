use super::prelude::{
    asking, done, forget_revealing, play, remembered_cards, spell, with_candidates, LimitedPlay,
    Location, Price,
};
use super::{Card, Flow, Item, Keyword, Stage, KIND_UNIT};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;
use crate::state::{Origin, TargetRef, FLAG_REVEALING};

pub const DISCOUNT: u8 = 3;
pub const QUESTION: &str = "a unit in your hand to play to a battlefield you control for 3 less";
pub const PICKED: u8 = 1;
pub const REVEALED: u8 = 2;
pub const LOCATED: u8 = 3;

pub fn held_battlefields_open_to_units(ctx: &Ctx, seat: u8) -> Vec<Location> {
    ctx.held_battlefields(seat)
        .into_iter()
        .filter(|zone| ctx.units_played_here(*zone))
        .map(Location::Battlefield)
        .collect()
}

pub fn discounted_price(ctx: &Ctx, unit: u32) -> cost::Cost {
    let printed = cost::printed_of(ctx, unit, false);
    cost::Cost {
        energy: printed.energy.saturating_sub(DISCOUNT),
        ..printed
    }
}

fn picked_in_hand(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .find(|card| ctx.owner(*card) == seat && ctx.in_hand(*card))
}

pub fn play_from_hand_to_a_held_battlefield_with_a_discount(
    ctx: &mut Ctx,
    item: &Item,
    unit: u32,
    at: Location,
) -> bool {
    let seat = item.controller;
    if !ctx.in_hand(unit) || ctx.owner(unit) != seat {
        ctx.narrate(format!("{{card {unit}}} is no longer in hand"));
        return false;
    }
    if ctx.kind_of(unit) != Some(KIND_UNIT) {
        ctx.narrate(format!("{{card {unit}}} is not a unit · it stays in hand"));
        return false;
    }
    if !held_battlefields_open_to_units(ctx, seat).contains(&at) {
        ctx.narrate(format!(
            "{{card {unit}}} stays in hand · {} is no longer yours to play to",
            describe(at)
        ));
        return false;
    }
    let price = Price::LessEnergy(DISCOUNT);
    let quote = cost::priced(ctx, unit, price, false);
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {unit}}} from hand to {} for {}",
        describe(at),
        quote.label()
    ));
    let result = ctx.play_limited(LimitedPlay {
        card: unit,
        by: seat,
        origin: Origin::Hand,
        locations: vec![at],
        price,
    });
    result.is_ok()
}

fn queue_help(ctx: &mut Ctx, item: &Item, unit: u32, locations: Vec<Location>) -> bool {
    let seat = item.controller;
    if !ctx.in_hand(unit) || ctx.owner(unit) != seat || ctx.kind_of(unit) != Some(KIND_UNIT) {
        return false;
    }
    ctx.play_limited(LimitedPlay {
        card: unit,
        by: seat,
        origin: Origin::Hand,
        locations,
        price: Price::LessEnergy(DISCOUNT),
    })
    .is_ok()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    let seat = item.controller;
    match stage.0 {
        PICKED => ctx.hand_of(seat).into_iter().map(TargetRef::Card).collect(),
        LOCATED => held_battlefields_open_to_units(ctx, seat)
            .into_iter()
            .filter_map(|at| ctx.zone_of(at))
            .map(|(zone, _)| TargetRef::Zone(zone))
            .collect(),
        _ => Vec::new(),
    }
}

fn help(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        PICKED => {
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
                return help(ctx, item, Stage(REVEALED));
            }
            ctx.narrate(format!("{{seat {seat}}} reveals {{card {card}}}"));
            Flow::Ask(ctx.await_faces(item, &[card], REVEALED))
        }
        REVEALED => {
            let Some(unit) = picked_in_hand(ctx, seat) else {
                return done();
            };
            forget_revealing(ctx, unit);
            if ctx.kind_of(unit) != Some(KIND_UNIT) {
                ctx.narrate(format!("{{card {unit}}} is not a unit · it stays in hand"));
                return done();
            }
            match held_battlefields_open_to_units(ctx, seat).as_slice() {
                [] => {
                    ctx.narrate(format!(
                        "{{card {unit}}} stays in hand · no battlefield to play to"
                    ));
                    done()
                }
                locations => {
                    queue_help(ctx, item, unit, locations.to_vec());
                    done()
                }
            }
        }
        LOCATED => {
            let Some(unit) = remembered_cards(item).last().copied() else {
                return done();
            };
            let Some(at) = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            else {
                ctx.narrate(format!("{{card {unit}}} stays in hand"));
                return done();
            };
            play_from_hand_to_a_held_battlefield_with_a_discount(ctx, item, unit, at);
            done()
        }
        _ => {
            if held_battlefields_open_to_units(ctx, seat).is_empty() {
                ctx.narrate(format!(
                    "{{seat {seat}}} controls no battlefield to play a unit to"
                ));
                return done();
            }
            if ctx.hand_of(seat).is_empty() {
                ctx.narrate(format!("{{seat {seat}}} has no card in hand"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = spell(
    "Here to Help",
    &[Keyword::Hidden, Keyword::Action],
    &[asking(
        with_candidates(play(&[], help), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Cost, Keyword, Trigger, KIND_SPELL};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, prompts, settle};
    use crate::state::{Expiry, GameBlob, ItemStatus, PromptWhy};
    use agni_plugin_sdk::cbor::{Item as CborItem, Reader, Writer};
    use agni_plugin_sdk::decide::{Action, Effect};
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::{CardInfo, Face};

    const HELP: u32 = 90;
    const BRUTE: u32 = 91;
    const SPARK: u32 = 92;
    const DEAR: u32 = 93;
    const FAR_FIELD: u32 = 94;
    const SECOND_BRUTE: u32 = 95;

    fn face_of(id: u32) -> Face {
        match id {
            SPARK => Face::named("Spark")
                .with_kind(KIND_SPELL)
                .with_cost(Some(4), Some(0))
                .with_domain(vec!["Fury".into()]),
            DEAR => Face::named("Dear")
                .with_kind(KIND_UNIT)
                .with_cost(Some(9), Some(0))
                .with_might(Some(6))
                .with_domain(vec!["Fury".into()]),
            _ => Face::named("Brute")
                .with_kind(KIND_UNIT)
                .with_cost(Some(4), Some(0))
                .with_might(Some(3))
                .with_domain(vec!["Fury".into()]),
        }
    }

    fn reveal_action(card: u32) -> Action {
        Action::Reveal {
            card,
            face: face_of(card),
        }
    }

    fn camp(held: &[u16]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Body".into()],
            ..fixtures::spell(HELP, fixtures::HAND, 0, "Here to Help", 2, 1)
        });
        fixture.table.cards.retain(|card| {
            !(card.zone == Some(fixtures::HAND) && card.seat == 0) || card.id == HELP
        });
        for id in [BRUTE, SPARK, DEAR] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::HAND, 0));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Body", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.table.cards.push(fixtures::card(
            FAR_FIELD,
            fixtures::BF3,
            0,
            "Far Field",
            "Battlefield",
        ));
        for zone in held {
            fixture.blob.set_holder(*zone, Some(0));
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(HELP).unwrap(), &CARD));
        fixture
    }

    fn revealed_camp(held: &[u16]) -> Fixture {
        let mut fixture = camp(held);
        for id in [BRUTE, SPARK, DEAR] {
            fixture.table.apply_entry(&reveal_action(id), 0).unwrap();
        }
        fixture.resolve();
        fixture
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, HELP).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target at play time");
        fixtures::pass_until_open(ctx);
    }

    fn ready_runes(ctx: &Ctx, seat: u8) -> usize {
        ctx.runes_of(seat)
            .into_iter()
            .filter(|rune| !rune.exhausted)
            .count()
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
    fn the_script_is_a_hidden_action_that_asks_for_a_hand_card_and_then_a_battlefield() {
        assert!(std::ptr::eq(script_of("Here to Help").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(DISCOUNT, 3);
    }

    #[test]
    fn the_price_is_the_printed_cost_less_three_energy_and_never_below_zero() {
        let mut fixture = revealed_camp(&[fixtures::BF1]);
        let ctx = fixture.ctx();
        assert_eq!(discounted_price(&ctx, BRUTE).energy, 1);
        assert_eq!(discounted_price(&ctx, DEAR).energy, 6);
        assert!(discounted_price(&ctx, BRUTE).power.is_empty());
        assert_eq!(
            held_battlefields_open_to_units(&ctx, 0),
            [Location::Battlefield(fixtures::BF1)]
        );
        assert!(held_battlefields_open_to_units(&ctx, 1).is_empty());
    }

    #[test]
    fn every_hand_card_is_offered_the_pick_is_revealed_and_a_unit_is_played_to_the_held_battlefield_for_three_less(
    ) {
        let mut fixture = camp(&[fixtures::BF1]);
        let mut ctx = fixture.ctx();
        let runes = ready_runes(&ctx, 0);
        cast(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICKED
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BRUTE}}}"),
                format!("{{card {SPARK}}}"),
                format!("{{card {DEAR}}}"),
                "skip".to_string()
            ],
            "every hand card is offered · the fold carries no hand faces"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {HELP}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        let parked = &ctx.blob.chain[0];
        assert_eq!(
            (parked.status, parked.stage),
            (ItemStatus::Resolving, REVEALED)
        );
        assert_eq!(parked.awaiting, [BRUTE]);
        assert!(ctx.effects.contains(&Effect::Reveal { card: BRUTE }));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} reveals {{card {BRUTE}}}")));
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let mut ctx = fixture.ctx_for(0, &reveal_action(BRUTE));
        assert!(chain::face_arrived(&mut ctx, BRUTE).unwrap());
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "the spell and the play both resolved"
        );
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.card(BRUTE).unwrap().exhausted,
            "369.3 · a played unit enters exhausted"
        );
        assert!(!ctx.has_flag(BRUTE, FLAG_REVEALING));
        assert_eq!(
            ready_runes(&ctx, 0),
            runes - 2 - 1,
            "Here to Help's 2E 1P off two runes (the Body rune is exhausted, then recycled), then one energy for the Brute's 4 less 3"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {BRUTE}}} to {{zone {}}}",
            fixtures::BF1
        )));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == BRUTE
        )));
        assert_eq!(ctx.card(HELP).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_here_to_help_keeps_each_saved_pick_across_reload_and_uses_both_held_fields() {
        let mut fixture = camp(&[fixtures::BF1, fixtures::BF3]);
        fixture
            .table
            .cards
            .push(fixtures::hidden(SECOND_BRUTE, fixtures::HAND, 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.grant(HELP, Keyword::Repeat(Cost::FREE), Expiry::Permanent,));
        fixtures::play_from_hand(&mut ctx, 0, HELP).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: crate::state::SLOT_REPEAT as u8,
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        let runes = ready_runes(&ctx, 0);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            (ctx.blob.chain[0].status, ctx.blob.chain[0].stage),
            (ItemStatus::Resolving, REVEALED)
        );
        assert_eq!(ctx.blob.chain[0].awaiting, [BRUTE]);
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx_for(0, &reveal_action(BRUTE));
        assert!(chain::face_arrived(&mut ctx, BRUTE).unwrap());
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICKED,
            })
        );
        assert_eq!(
            ctx.location(BRUTE),
            None,
            "the child waits until the parent finishes"
        );
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Resolving);
        assert_eq!(ctx.blob.queue.len(), 1, "the first child is deferred");
        assert_eq!(ready_runes(&ctx, 0), runes, "the child has not been paid");
        fixtures::choose(&mut ctx, 0, &format!("{{card {SECOND_BRUTE}}}")).unwrap();
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx_for(0, &reveal_action(SECOND_BRUTE));
        assert!(chain::face_arrived(&mut ctx, SECOND_BRUTE).unwrap());
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::PlayLocation { item: _ })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BF1),
                format!("{{zone {}}}", fixtures::BF3)
            ]
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert!(prompts::answer(&mut ctx, 0, Pick { prompt, option: 2 }).is_err());
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF3)).unwrap();
        let table = ctx.table.clone();
        let blob = ctx.blob.encode();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&blob).unwrap();
        let mut ctx = fixture.ctx();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF3)),
            "the first child keeps its selected destination"
        );
        assert_eq!(
            ctx.location(SECOND_BRUTE),
            Some(Location::Battlefield(fixtures::BF1)),
            "the second execution resolves its own saved pick"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_two_held_battlefields_the_destination_is_asked_after_the_reveal() {
        let mut fixture = revealed_camp(&[fixtures::BF1, fixtures::BF3]);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })),
            "the queued child asks where after the parent resolves"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BF1),
                format!("{{zone {}}}", fixtures::BF3)
            ],
            "the held battlefields only · never the base"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF3)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF3))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn legacy_located_stage_resumes_after_v11_reload_and_plays_once_to_selected_field() {
        let mut fixture = camp(&[fixtures::BF1, fixtures::BF3]);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].awaiting, [BRUTE]);
        ctx.table.apply_entry(&reveal_action(BRUTE), 0).unwrap();
        let item = &mut ctx.blob.chain[0];
        item.status = ItemStatus::Resolving;
        item.stage = LOCATED;
        item.awaiting.clear();
        item.targets.push(TargetRef::Card(BRUTE));
        let table = ctx.table.clone();
        let bytes = legacy_v11(&ctx.blob.encode());
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&bytes).expect("legacy v11 stage row");
        let mut ctx = fixture.ctx();
        let item = ctx.blob.chain[0].clone();
        ctx.ask_resume(&item, LOCATED, 1, 1);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BF1),
                format!("{{zone {}}}", fixtures::BF3),
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF3)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF3))
        );
        assert_eq!(
            ctx.table
                .cards
                .iter()
                .filter(|card| card.id == BRUTE && card.zone == Some(fixtures::BF3))
                .count(),
            1
        );
        assert_eq!(ctx.blob.chain.len(), 0);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Played { card, .. } if *card == BRUTE))
                .count(),
            1
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_revealed_spell_stays_in_hand_and_so_does_a_unit_that_is_still_too_dear() {
        let mut fixture = revealed_camp(&[fixtures::BF1]);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {SPARK}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(SPARK));
        assert!(!ctx.has_flag(SPARK, FLAG_REVEALING));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SPARK}}} is not a unit · it stays in hand"
        )));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = revealed_camp(&[fixtures::BF1]);
        let mut ctx = fixture.ctx();
        let runes = ready_runes(&ctx, 0);
        cast(&mut ctx);
        let after_the_spell = ready_runes(&ctx, 0);
        assert_eq!(after_the_spell, runes - 2);
        fixtures::choose(&mut ctx, 0, &format!("{{card {DEAR}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(DEAR));
        assert_eq!(ready_runes(&ctx, 0), after_the_spell, "nothing is paid");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DEAR}}} can't be played · its cost can't be paid"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_plays_nothing_and_without_a_held_battlefield_or_a_hand_nothing_is_asked() {
        let mut fixture = revealed_camp(&[fixtures::BF1]);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} plays nothing".to_string()));
        assert!(ctx.in_hand(BRUTE));
        drop(ctx);

        let mut fixture = revealed_camp(&[]);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} controls no battlefield to play a unit to".to_string()));
        drop(ctx);

        let mut fixture = revealed_camp(&[fixtures::BF1]);
        fixture.table.cards.retain(|card| {
            !(card.zone == Some(fixtures::HAND) && card.seat == 0) || card.id == HELP
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card in hand".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · a hand play inside a resolution rides Origin::Banishment (legal::locations_for refuses Origin::Hand under the closed chain and decide is blind to hand faces, the Ava Achiever row): Event::Played should report Origin::Hand so Rek'Sai - Breacher's from-anywhere-but-hand reading and the origin-aware triggers see a hand play, and a face-aware candidate list should offer only affordable units instead of every hand card"]
    fn the_play_reports_its_hand_origin_and_only_affordable_units_are_offered() {
        let mut fixture = revealed_camp(&[fixtures::BF1]);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BRUTE}}}"), "skip".to_string()],
            "the spell and the unit that is too dear are not offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, origin: Origin::Hand, .. } if *card == BRUTE
        )));
    }
}
