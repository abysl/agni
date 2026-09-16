use super::daisy::{is_bird, is_cat, is_dog};
use super::poro_herder::is_poro;
use super::prelude::{
    asking, buff, done, draw_revealed, forget_revealing, friendly_units, on_hold_me, play, unit,
    with_candidates,
};
use super::{Ability, Card, Flow, Item, Stage, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::state::{TargetRef, FLAG_REVEALING};
use agni_plugin_sdk::decide::{Effect, TOP};

pub const LOOK: usize = 3;
pub const QUESTION: &str =
    "a unit among the top three to reveal and draw, then a friendly unit to buff";
pub const PICK: u8 = 1;
pub const REVEALED: u8 = 2;
pub const BUFF: u8 = 3;

pub fn is_bird_cat_dog_or_poro(ctx: &Ctx, card: u32) -> bool {
    is_bird(ctx, card) || is_cat(ctx, card) || is_dog(ctx, card) || is_poro(ctx, card)
}

pub fn looked(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default()
}

pub fn may_be_unit(ctx: &Ctx, card: u32) -> bool {
    ctx.kind_of(card).is_none_or(|kind| kind == KIND_UNIT)
}

pub fn on_top(ctx: &Ctx, item: &Item) -> Vec<TargetRef> {
    looked(ctx, item.controller)
        .into_iter()
        .filter(|card| may_be_unit(ctx, *card))
        .map(TargetRef::Card)
        .collect()
}

pub fn look_at_the_top_three(ctx: &mut Ctx, item: &Item, pick: u8) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    if top.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no cards left to look at"));
        return done();
    }
    for card in &top {
        ctx.peek(*card, seat);
    }
    ctx.narrate(format!(
        "{{seat {seat}}} looks at the top {} cards of their deck",
        top.len()
    ));
    Flow::Ask(ctx.ask_resume(item, pick, 0, 1))
}

fn recycle_all(ctx: &mut Ctx, seat: u8, cards: &[u32]) {
    for card in cards {
        ctx.recycle_to_bottom(*card);
    }
    if !cards.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} recycles {}", cards.len()));
    }
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

pub fn reveal_the_pick(ctx: &mut Ctx, item: &Item, revealed: u8) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    let Some(card) = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| top.contains(card) && may_be_unit(ctx, *card))
    else {
        ctx.narrate(format!("{{seat {seat}}} reveals nothing"));
        recycle_all(ctx, seat, &top);
        return done();
    };
    let rest: Vec<u32> = top.into_iter().filter(|held| *held != card).collect();
    if !reveal_from_the_deck(ctx, seat, card) {
        return done();
    }
    recycle_all(ctx, seat, &rest);
    Flow::Ask(ctx.await_faces(item, &[card], revealed))
}

pub fn revealing(ctx: &Ctx, seat: u8) -> Option<u32> {
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

pub fn draw_the_revealed_unit(ctx: &mut Ctx, seat: u8) -> Option<u32> {
    let card = revealing(ctx, seat)?;
    forget_revealing(ctx, card);
    if ctx.is_unit(card) {
        draw_revealed(ctx, seat, card);
        Some(card)
    } else {
        ctx.recycle_to_bottom(card);
        ctx.narrate(format!(
            "{{card {card}}} is not a unit · it is recycled with the rest"
        ));
        None
    }
}

fn buffable(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    friendly_units(ctx, seat)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        PICK => on_top(ctx, item),
        BUFF => buffable(ctx, item.controller),
        _ => Vec::new(),
    }
}

fn boon(ctx: &mut Ctx, unit: u32) {
    if buff(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is buffed"));
    } else {
        ctx.narrate(format!("{{card {unit}}} already has a buff"));
    }
}

fn nurture(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        PICK => reveal_the_pick(ctx, item, REVEALED),
        REVEALED => {
            let companion =
                revealing(ctx, seat).is_some_and(|card| is_bird_cat_dog_or_poro(ctx, card));
            let drawn = draw_the_revealed_unit(ctx, seat);
            if drawn.is_none() || !companion {
                return done();
            }
            match buffable(ctx, seat).as_slice() {
                [] => done(),
                [TargetRef::Card(only)] => {
                    boon(ctx, *only);
                    done()
                }
                _ => Flow::Ask(ctx.ask_resume(item, BUFF, 1, 1)),
            }
        }
        BUFF => {
            if let Some(unit) = ctx
                .picks()
                .first()
                .copied()
                .filter(|unit| buffable(ctx, seat).contains(&TargetRef::Card(*unit)))
            {
                boon(ctx, unit);
            }
            done()
        }
        _ => look_at_the_top_three(ctx, item, PICK),
    }
}

const ON_PLAY: Ability = asking(with_candidates(play(&[], nurture), candidates), QUESTION);
const ON_HOLD: Ability = asking(
    with_candidates(on_hold_me(&[], nurture), candidates),
    QUESTION,
);

pub static CARD: Card = unit("Ivern - Nurturer", &[], &[ON_PLAY, ON_HOLD]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::daisy::{BIRDS, CATS, DOGS};
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, Who, KIND_SPELL, KIND_UNIT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, cleanup, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use agni_plugin_sdk::decide::{Action, BOTTOM};
    use agni_plugin_sdk::table::{CardInfo, Face};

    const IVERN: u32 = 90;
    const ALLY: u32 = 91;
    const TOP: [u32; 3] = [23, 22, 21];

    fn ivern(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(1),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(IVERN, zone, seat, "Ivern - Nurturer", 4)
        }
    }

    fn grove(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ivern(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
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

    fn hold(fixture: &mut Fixture) {
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == IVERN
        ));
        assert!(ctx.blob.prompt.is_none(), "the look comes at resolution");
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
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    #[test]
    fn the_script_is_a_play_trigger_and_a_hold_trigger_that_share_one_asking_run() {
        assert!(std::ptr::eq(script_of("Ivern - Nurturer").unwrap(), &CARD));
        assert_eq!(CARD.name, "Ivern - Nurturer");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[1].trigger, Trigger::Hold(Who::Me));
        for ability in CARD.abilities {
            assert!(ability.targets.is_empty());
            assert!(!ability.optional);
            assert!(ability.candidates.is_some());
            assert_eq!(ability.question, Some(QUESTION));
            assert!(ability.cost.is_none() && ability.condition.is_none());
        }
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(LOOK, 3);
        for list in [&BIRDS[..], &CATS[..], &DOGS[..]] {
            assert!(list.windows(2).all(|pair| pair[0] < pair[1]), "sorted");
        }
    }

    #[test]
    fn the_tag_lists_read_printed_names_and_poros_through_the_poro_herder_seam() {
        let mut fixture = grove(fixtures::BF1);
        for (id, name) in [
            (92, "Soaring Scout"),
            (93, "Pakaa Cub"),
            (94, "Loyal Pup"),
            (95, "Pouty Poro"),
            (96, "Sunlit Guardian"),
        ] {
            fixture
                .table
                .cards
                .push(fixtures::unit(id, fixtures::BASE, 0, name, 1));
        }
        fixture
            .table
            .cards
            .push(fixtures::gear(97, fixtures::BASE, 0, "Eclipse Herald", 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_bird(&ctx, 92) && is_bird_cat_dog_or_poro(&ctx, 92));
        assert!(is_cat(&ctx, 93) && is_bird_cat_dog_or_poro(&ctx, 93));
        assert!(is_dog(&ctx, 94) && is_bird_cat_dog_or_poro(&ctx, 94));
        assert!(is_bird_cat_dog_or_poro(&ctx, 95));
        assert!(!is_bird_cat_dog_or_poro(&ctx, 96));
        assert!(!is_bird_cat_dog_or_poro(&ctx, IVERN));
        assert!(!is_bird(&ctx, 97), "a gear of a Bird's name is no unit");
    }

    #[test]
    fn holding_looks_at_three_reveals_the_pick_draws_a_unit_and_a_poro_buffs_a_friendly_unit() {
        let mut fixture = grove(fixtures::BF1);
        assert_eq!(deck_of(&fixture.ctx(), 0), [20, 21, 22, 23]);
        let hand = fixture.ctx().hand_of(0).len();
        hold(&mut fixture);
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        for card in TOP {
            assert!(
                ctx.effects.is_empty() || ctx.effects.contains(&Effect::Peek { card, seat: 0 }),
                "the peeks were applied in the earlier decide"
            );
        }
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}", "{card 22}", "{card 21}", "skip"],
            "the top three, top first, and a skip"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {IVERN}}}: choose {QUESTION} (0 of 1)")
        );
        drop(ctx);
        pick(&mut fixture, "{card 22}");
        let ctx = fixture.ctx();
        let parked = &ctx.blob.chain[0];
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.stage, REVEALED);
        assert_eq!(parked.awaiting, [22]);
        assert_eq!(ctx.card(22).unwrap().zone, Some(fixtures::CHAIN));
        assert!(ctx.has_flag(22, FLAG_REVEALING));
        assert_eq!(
            deck_of(&ctx, 0),
            [21, 23, 20],
            "the rest recycled under the deck, top first"
        );
        assert!(ctx.blob.prompt.is_none(), "the host's reveal is awaited");
        drop(ctx);
        arrives(
            &mut fixture,
            22,
            Face::named("Pouty Poro").with_kind(KIND_UNIT),
        );
        let ctx = fixture.ctx();
        assert_eq!(ctx.card(22).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.blob.seat(0).draws, 1, "drawn, not merely put into hand");
        assert!(!ctx.has_flag(22, FLAG_REVEALING));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: BUFF
            }),
            "a Poro: a friendly unit is buffed"
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {IVERN}}}"),
                format!("{{card {ALLY}}}")
            ]
        );
        drop(ctx);
        pick(&mut fixture, &format!("{{card {ALLY}}}"));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.is_buffed(ALLY));
        assert!(!ctx.is_buffed(IVERN));
        assert!(ctx.blob.log.contains(&format!("{{card {ALLY}}} is buffed")));
    }

    #[test]
    fn a_top_card_already_known_to_be_a_spell_is_not_offered_and_a_pick_of_it_reveals_nothing() {
        let mut fixture = grove(fixtures::BF1);
        fixture.table.card_mut(22).unwrap().kind = Some(KIND_SPELL.into());
        fixture.resolve();
        hold(&mut fixture);
        let ctx = fixture.ctx();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}", "{card 21}", "skip"],
            "only a card that may still be a unit is offered"
        );
        drop(ctx);
        let mut fixture = grove(fixtures::BF1);
        fixture.table.card_mut(22).unwrap().kind = Some(KIND_SPELL.into());
        fixture.resolve();
        hold(&mut fixture);
        let mut ctx = fixture.ctx();
        ctx.picked = vec![22];
        let item = ctx.blob.chain[0].clone();
        let flow = reveal_the_pick(&mut ctx, &item, REVEALED);
        assert_eq!(flow, Flow::Done, "a known spell is never revealed");
        assert!(!ctx.has_flag(22, FLAG_REVEALING));
        assert_eq!(
            deck_of(&ctx, 0),
            [21, 22, 23, 20],
            "all three recycled under, top first"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals nothing".to_string()));
    }

    #[test]
    fn a_unit_that_is_no_companion_is_drawn_without_a_buff_and_a_spell_is_recycled() {
        let mut fixture = grove(fixtures::BF1);
        let hand = fixture.ctx().hand_of(0).len();
        hold(&mut fixture);
        pick(&mut fixture, "{card 23}");
        arrives(&mut fixture, 23, Face::named("Jinx").with_kind(KIND_UNIT));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "no buff for a Jinx");
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(!ctx.is_buffed(ALLY));
        drop(ctx);

        let mut fixture = grove(fixtures::BF1);
        hold(&mut fixture);
        pick(&mut fixture, "{card 23}");
        arrives(&mut fixture, 23, Face::named("Spark").with_kind(KIND_SPELL));
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(
            deck_of(&ctx, 0),
            [23, 21, 22, 20],
            "the spell goes under with the rest"
        );
        assert_eq!(ctx.blob.seat(0).draws, 0);
        assert!(
            ctx.effects.is_empty()
                || ctx.effects.contains(&Effect::Move {
                    card: 23,
                    zone: fixtures::MAIN_DECK,
                    seat: 0,
                    index: BOTTOM
                })
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 23} is not a unit · it is recycled with the rest".to_string()));
    }

    #[test]
    fn skipping_recycles_all_three_and_an_empty_deck_looks_at_nothing() {
        let mut fixture = grove(fixtures::BF1);
        let hand = fixture.ctx().hand_of(0).len();
        hold(&mut fixture);
        pick(&mut fixture, "skip");
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(deck_of(&ctx, 0), [21, 22, 23, 20], "all three go under");
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} reveals nothing".to_string()));
        drop(ctx);

        let mut fixture = grove(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.seat != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to look at".to_string()));
    }

    #[test]
    fn playing_him_looks_too_and_with_him_as_the_only_friendly_unit_the_buff_needs_no_pick() {
        let mut fixture = grove(fixtures::HAND);
        fixture
            .table
            .cards
            .retain(|card| card.id != ALLY && card.id != fixtures::VI);
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, IVERN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(IVERN));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 2,
                stage: PICK
            })
        );
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        pick(&mut fixture, "{card 21}");
        arrives(
            &mut fixture,
            21,
            Face::named("Loyal Pup").with_kind(KIND_UNIT),
        );
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "he is the only friendly unit");
        assert!(ctx.is_buffed(IVERN));
        assert_eq!(ctx.card(21).unwrap().zone, Some(fixtures::HAND));
    }

    #[test]
    fn a_hold_without_him_and_the_opponents_hold_trigger_nothing() {
        let mut fixture = grove(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the ally holds; he sits in the base"
        );
        drop(ctx);
        let mut fixture = grove(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · tags on the face: CardInfo carries no tags, so is_bird_cat_dog_or_poro reads printed-name lists and poro_herder::is_poro; a unit tagged Bird, Cat, Dog or Poro under a name the lists do not know is invisible until a tags row lands"]
    fn a_tagged_unit_the_lists_do_not_know_still_counts() {
        let mut fixture = grove(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(92, fixtures::BASE, 0, "Unlisted Hound", 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_bird_cat_dog_or_poro(&ctx, 92));
    }
}
