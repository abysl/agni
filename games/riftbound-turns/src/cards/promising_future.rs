use super::prelude::{
    asking, done, forget_revealing, play, spell, with_candidates, LimitedPlay, Location,
};
use super::whirlwind::seats_from_the_next_player;
use super::{Card, Flow, Item, Stage};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{Origin, Price, RevealedFrom, TargetRef, FLAG_REVEALING};

pub const LOOK: usize = 5;
pub const QUESTION: &str = "a card to keep from the top five, then where it is played";
pub const CHOOSE: u8 = 1;
pub const REVEALED: u8 = 17;
pub const LOCATE: u8 = 18;

fn seats_in_turn_order(ctx: &Ctx) -> Vec<u8> {
    let order = ctx.blob.order();
    let mut seats = Vec::new();
    let mut seat = ctx.turn_player();
    for _ in 0..ctx.players() {
        seats.push(seat);
        seat = order.next_seat(seat);
    }
    seats
}

pub fn looked(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default()
}

pub fn chosen_by(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .find(|card| {
            ctx.owner(*card) == seat
                && ctx
                    .card(*card)
                    .is_some_and(|held| held.zone == ctx.zones.chain)
        })
}

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, _: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        CHOOSE..REVEALED => seats_in_turn_order(ctx)
            .get(usize::from(stage.0 - CHOOSE))
            .map(|seat| looked(ctx, *seat))
            .unwrap_or_default()
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        REVEALED => Vec::new(),
        LOCATE.. => seats_from_the_next_player(ctx)
            .get(usize::from(stage.0 - LOCATE))
            .map(|seat| location_options(ctx, *seat))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub fn power_only(ctx: &Ctx, card: u32) -> cost::Cost {
    cost::priced(ctx, card, Price::PowerOnly, false)
}

pub fn queue_play(ctx: &mut Ctx, seat: u8, card: u32) -> bool {
    forget_revealing(ctx, card);
    ctx.play_limited(LimitedPlay {
        card,
        by: seat,
        origin: Origin::Revealed {
            from: RevealedFrom::Deck,
        },
        locations: if ctx.is_unit(card) {
            ctx.play_locations(seat)
        } else {
            Vec::new()
        },
        price: Price::PowerOnly,
    })
    .is_ok()
}

fn queue_play_at(ctx: &mut Ctx, seat: u8, card: u32, at: Location) -> bool {
    forget_revealing(ctx, card);
    ctx.play_limited(LimitedPlay {
        card,
        by: seat,
        origin: Origin::Revealed {
            from: RevealedFrom::Deck,
        },
        locations: vec![at],
        price: Price::PowerOnly,
    })
    .is_ok()
}

fn look_from(ctx: &mut Ctx, item: &Item, from: usize) -> Flow {
    let seats = seats_in_turn_order(ctx);
    for (index, seat) in seats.iter().enumerate().skip(from) {
        let top = looked(ctx, *seat);
        if top.is_empty() {
            ctx.narrate(format!("{{seat {seat}}} has no cards left to look at"));
            continue;
        }
        for card in &top {
            ctx.peek(*card, *seat);
        }
        ctx.narrate(format!(
            "{{seat {seat}}} looks at the top {} cards of their deck",
            top.len()
        ));
        let stage = u8::try_from(index)
            .ok()
            .and_then(|index| index.checked_add(CHOOSE))
            .unwrap_or(CHOOSE);
        return Flow::Ask(ctx.ask_seat_resume(item, *seat, stage, 1, 1));
    }
    let unseen: Vec<u32> = seats
        .iter()
        .filter_map(|seat| chosen_by(ctx, *seat))
        .filter(|card| !ctx.table.is_revealed(*card))
        .collect();
    if unseen.is_empty() {
        return play_from(ctx, item, 0);
    }
    Flow::Ask(ctx.await_faces(item, &unseen, REVEALED))
}

fn keep(ctx: &mut Ctx, seat: u8) {
    let top = looked(ctx, seat);
    let kept = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| top.contains(card))
        .or_else(|| top.first().copied());
    let Some(kept) = kept else {
        return;
    };
    let rest: Vec<u32> = top.into_iter().filter(|card| *card != kept).collect();
    for card in &rest {
        ctx.recycle_to_bottom(*card);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} keeps a card and recycles {}",
        rest.len()
    ));
    ctx.reveal_top(seat);
}

fn play_from(ctx: &mut Ctx, _item: &Item, from: usize) -> Flow {
    let seats = seats_from_the_next_player(ctx);
    for seat in seats.iter().skip(from) {
        let Some(card) = chosen_by(ctx, *seat) else {
            continue;
        };
        queue_play(ctx, *seat, card);
    }
    done()
}

fn promise(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        CHOOSE..REVEALED => {
            let index = usize::from(stage.0 - CHOOSE);
            if let Some(seat) = seats_in_turn_order(ctx).get(index).copied() {
                keep(ctx, seat);
            }
            look_from(ctx, item, index + 1)
        }
        REVEALED => play_from(ctx, item, 0),
        LOCATE.. => {
            let index = usize::from(stage.0 - LOCATE);
            if let Some(seat) = seats_from_the_next_player(ctx).get(index).copied() {
                if let Some(card) = chosen_by(ctx, seat) {
                    let at = ctx
                        .picks()
                        .first()
                        .and_then(|zone| u16::try_from(*zone).ok())
                        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
                        .filter(|at| ctx.play_locations(seat).contains(at));
                    if let Some(at) = at {
                        queue_play_at(ctx, seat, card, at);
                    }
                }
            }
            play_from(ctx, item, index + 1)
        }
        _ => look_from(ctx, item, 0),
    }
}

pub static CARD: Card = spell(
    "Promising Future",
    &[],
    &[asking(
        with_candidates(play(&[], promise), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, might_this_turn, unit as unit_card};
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::script_of;
    use crate::cards::{KIND_SPELL, KIND_UNIT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, prompts, settle};
    use crate::state::{GameBlob, ItemKind, ItemStatus, PromptWhy, FLAG_REVEALING};
    use crate::Refusal;
    use agni_plugin_sdk::cbor::{Item as CborItem, Reader, Writer};
    use agni_plugin_sdk::decide::Action;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::Face;

    const PROMISE: u32 = 90;
    const MY_TOP: u32 = 23;
    const THEIR_TOP: u32 = 25;

    static PUMP: Card = spell(
        "Pump",
        &[],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = crate::cards::prelude::card_target(ctx, item, 0) {
                might_this_turn(ctx, item, unit, 2, None);
            }
            Flow::Done
        })],
    );

    static CRAB: Card = unit_card("Crab", &[], &[]);

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut promise = fixtures::spell(PROMISE, fixtures::HAND, 0, "Promising Future", 0, 0);
        promise.domain = vec!["Mind".into()];
        fixture.table.cards.push(promise);
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PROMISE).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing is targeted");
        fixtures::pass_until_open(&mut ctx);
        for card in deck_of(&ctx, 0) {
            assert!(ctx
                .effects
                .contains(&agni_plugin_sdk::decide::Effect::Peek { card, seat: 0 }));
            assert!(!ctx
                .effects
                .contains(&agni_plugin_sdk::decide::Effect::Peek { card, seat: 1 }));
        }
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn pick(fixture: &mut Fixture, seat: u8, label: &str) {
        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, seat, label).unwrap();
        pass_until_parked(&mut ctx);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn arrives(fixture: &mut Fixture, card: u32, face: Face, script: &'static Card) -> bool {
        let action = Action::Reveal { card, face };
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
        let mut ctx = fixture.ctx_for(ctx_owner(card), &action);
        let arrived = chain::face_arrived(&mut ctx, card).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.scripts = fixture.scripts.clone().with_script(card, script);
        arrived
    }

    fn ctx_owner(card: u32) -> u8 {
        u8::from(card == THEIR_TOP)
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
    fn the_script_is_a_targetless_sorcery_whose_stages_cover_looks_reveal_and_locations() {
        assert!(std::ptr::eq(script_of("Promising Future").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(CARD.abilities[0].question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(REVEALED - CHOOSE, 16, "room for sixteen seats of looks");
        assert_eq!(LOCATE - REVEALED, 1);
    }

    #[test]
    fn each_seat_looks_and_keeps_in_turn_order_then_the_next_player_plays_first_for_power_only() {
        let mut fixture = armed();
        cast(&mut fixture);
        {
            let ctx = fixture.ctx();
            assert_eq!(
                ctx.blob.why,
                Some(PromptWhy::Resume {
                    item: 1,
                    stage: CHOOSE
                })
            );
            let prompt = ctx.blob.prompt.clone().unwrap();
            assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
            assert_eq!(
                fixtures::labels(&ctx),
                ["{card 23}", "{card 22}", "{card 21}", "{card 20}"],
                "a deck of four shows four"
            );
        }
        pick(&mut fixture, 0, "{card 23}");
        {
            let ctx = fixture.ctx();
            assert_eq!(deck_of(&ctx, 0), [20, 21, 22], "the rest went under");
            assert_eq!(ctx.card(MY_TOP).unwrap().zone, Some(fixtures::CHAIN));
            assert!(ctx.has_flag(MY_TOP, FLAG_REVEALING));
            assert_eq!(
                ctx.blob.why,
                Some(PromptWhy::Resume {
                    item: 1,
                    stage: CHOOSE + 1
                })
            );
            assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
            assert_eq!(fixtures::labels(&ctx), ["{card 25}", "{card 24}"]);
            assert_eq!(
                prompts::status(&ctx, ctx.blob.why.unwrap()),
                format!("{{seat 1}}: choose {QUESTION} (0 of 1)")
            );
        }
        pick(&mut fixture, 1, "{card 25}");
        {
            let ctx = fixture.ctx();
            assert_eq!(deck_of(&ctx, 1), [24]);
            let parked = &ctx.blob.chain[0];
            assert_eq!(parked.status, ItemStatus::Resolving);
            assert_eq!(parked.stage, REVEALED);
            assert_eq!(parked.awaiting, [MY_TOP, THEIR_TOP]);
        }
        assert!(arrives(
            &mut fixture,
            MY_TOP,
            Face::named("Scuttle Crab")
                .with_kind(KIND_UNIT)
                .with_might(Some(2))
                .with_cost(Some(6), Some(1))
                .with_domain(vec!["Fury".into()]),
            &crate::cards::scuttle_crab::CARD,
        ));
        {
            let ctx = fixture.ctx();
            assert_eq!(
                ctx.blob.chain[0].awaiting,
                [THEIR_TOP],
                "still parked on the other face"
            );
        }
        let my_runes = fixture.ctx().runes_of(0).len();
        let my_ready = fixture.ctx().ready_runes_of(0).len();
        let their_runes = fixture.ctx().runes_of(1).len();
        assert!(arrives(
            &mut fixture,
            THEIR_TOP,
            Face::named("Pump")
                .with_kind(KIND_SPELL)
                .with_cost(Some(4), Some(1))
                .with_domain(vec!["Mind".into()]),
            &PUMP,
        ));
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 2, spec: 0 }),
            "the next player's spell goes first and asks its target: {:?}",
            ctx.blob.log
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 1);
        assert!(!prompt.cancel);
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(
            ctx.runes_of(1).len(),
            their_runes - 1,
            "one Mind rune paid the power"
        );
        assert_eq!(ctx.runes_of(0).len(), my_runes - 1, "my power was paid too");
        assert_eq!(ctx.ready_runes_of(0).len(), my_ready, "no energy");
        assert_eq!(
            ctx.location(MY_TOP),
            Some(Location::Base(0)),
            "{:?}",
            ctx.blob.log
        );
        assert!(ctx.card(MY_TOP).unwrap().exhausted);
        assert_eq!(
            ctx.blob.chain.len(),
            2,
            "the spell below, my unit's play trigger above · {:?} {:?} {:?}",
            ctx.blob.log,
            ctx.blob.chain,
            ctx.blob.queue
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Spell { card } if card == THEIR_TOP
        ));
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Revealed {
                from: RevealedFrom::Deck
            }
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4);
        assert_eq!(
            ctx.card(THEIR_TOP).unwrap().zone,
            Some(fixtures::TRASH),
            "the resolved spell is trashed"
        );
        assert_eq!(ctx.card(THEIR_TOP).unwrap().seat, 1);
        assert_eq!(ctx.card(PROMISE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_held_battlefield_asks_the_owner_where_and_an_unpayable_card_goes_back_on_top() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        cast(&mut fixture);
        pick(&mut fixture, 0, "{card 22}");
        pick(&mut fixture, 1, "{card 24}");
        assert!(arrives(
            &mut fixture,
            22,
            Face::named("Crab")
                .with_kind(KIND_UNIT)
                .with_might(Some(2))
                .with_cost(Some(6), Some(0)),
            &CRAB,
        ));
        assert!(arrives(
            &mut fixture,
            24,
            Face::named("Crab")
                .with_kind(KIND_UNIT)
                .with_might(Some(2))
                .with_cost(Some(1), Some(5)),
            &CRAB,
        ));
        let mut ctx = fixture.ctx();
        assert!(
            ctx.blob
                .log
                .contains(&"{card 24} can't be played · its cost can't be paid".to_string()),
            "{:?}",
            ctx.blob.log
        );
        assert_eq!(deck_of(&ctx, 1), [25, 24]);
        assert!(!ctx.has_flag(24, FLAG_REVEALING));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::PlayLocation { item: 3 }),
            "the deferred unit child asks after the parent resolves"
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(22),
            Some(Location::Battlefield(fixtures::BF1)),
            "{:?}",
            ctx.blob.log
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn legacy_locate_stage_resumes_after_v11_reload_and_pays_the_selected_battlefield_play() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        cast(&mut fixture);
        pick(&mut fixture, 0, "{card 23}");
        pick(&mut fixture, 1, "{card 25}");
        let mut ctx = fixture.ctx();
        let face = Face::named("Legacy Crab")
            .with_kind(KIND_UNIT)
            .with_might(Some(2))
            .with_cost(Some(1), Some(1))
            .with_domain(vec!["Mind".into()]);
        ctx.table
            .apply_entry(
                &Action::Reveal {
                    card: THEIR_TOP,
                    face: face.clone(),
                },
                1,
            )
            .unwrap();
        ctx.table
            .apply_entry(&Action::Reveal { card: MY_TOP, face }, 0)
            .unwrap();
        ctx.set_flag(MY_TOP, FLAG_REVEALING, false);
        let item = &mut ctx.blob.chain[0];
        item.status = ItemStatus::Resolving;
        item.stage = LOCATE;
        item.awaiting.clear();
        let table = ctx.table.clone();
        let bytes = legacy_v11(&ctx.blob.encode());
        drop(ctx);
        fixture.commit(table);
        fixture.blob = GameBlob::decode(&bytes).expect("legacy v11 locate row");
        let mut ctx = fixture.ctx();
        let item = ctx.blob.chain[0].clone();
        ctx.ask_seat_resume(&item, 1, LOCATE, 1, 1);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1),
            ]
        );
        let power = ctx.runes_of(1).len();
        fixtures::choose(&mut ctx, 1, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(THEIR_TOP),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(MY_TOP), None);
        assert_eq!(ctx.runes_of(1).len(), power - 1, "{:?}", ctx.blob.log);
        assert_eq!(
            ctx.table
                .cards
                .iter()
                .filter(|card| card.id == THEIR_TOP && card.zone == Some(fixtures::BF1))
                .count(),
            1
        );
        assert_eq!(ctx.blob.chain.len(), 0);
        assert!(!ctx.has_flag(THEIR_TOP, FLAG_REVEALING));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| line.contains(&format!("plays {{card {THEIR_TOP}}}")))
                .count(),
            1,
            "{:?}",
            ctx.blob.log
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_empty_deck_is_skipped_and_the_other_seat_still_looks() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        fixture.resolve();
        cast(&mut fixture);
        let ctx = fixture.ctx();
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to look at".to_string()));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: CHOOSE + 1
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        drop(ctx);
        pick(&mut fixture, 1, "{card 25}");
        let ctx = fixture.ctx();
        assert_eq!(ctx.blob.chain[0].awaiting, [THEIR_TOP]);
    }

    #[test]
    fn the_wrong_seat_cannot_answer_a_look() {
        let mut fixture = armed();
        cast(&mut fixture);
        let mut ctx = fixture.ctx();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert!(ctx.effects.iter().all(|effect| !matches!(
            effect,
            agni_plugin_sdk::decide::Effect::Peek { seat: 1, .. }
        )));
        drop(ctx);
        pick(&mut fixture, 0, "{card 23}");
        let mut ctx = fixture.ctx();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 }))
        );
    }

    #[test]
    fn a_kept_spell_without_a_legal_target_goes_back_on_top_of_the_deck() {
        let mut fixture = armed();
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture.table.tokens.clear();
        fixture.resolve();
        cast(&mut fixture);
        pick(&mut fixture, 0, "{card 23}");
        pick(&mut fixture, 1, "{card 25}");
        assert!(arrives(
            &mut fixture,
            MY_TOP,
            Face::named("Pump")
                .with_kind(KIND_SPELL)
                .with_cost(Some(4), Some(1))
                .with_domain(vec!["Fury".into()]),
            &PUMP,
        ));
        assert!(arrives(
            &mut fixture,
            THEIR_TOP,
            Face::named("Pump")
                .with_kind(KIND_SPELL)
                .with_cost(Some(4), Some(1))
                .with_domain(vec!["Mind".into()]),
            &PUMP,
        ));
        let ctx = fixture.ctx();
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, MY_TOP]);
        assert_eq!(deck_of(&ctx, 1), [24, THEIR_TOP]);
        assert!(ctx.banished_of(0).is_empty() && ctx.banished_of(1).is_empty());
    }
}
