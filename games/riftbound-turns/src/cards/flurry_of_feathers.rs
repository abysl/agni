use super::frisky_hunter::play_birds;
use super::prelude::{
    asking, counter_spell, done, play, spell, target, with_candidates, Location, SPELL_ON_CHAIN,
};
use super::{Card, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const BIRDS: usize = 4;
pub const QUESTION: &str = "where the Birds are played";
pub const STAGE_LOCATE: u8 = 1;
pub const COUNTERABLE: TargetSpec = target(
    SPELL_ON_CHAIN,
    0,
    1,
    TargetKind::Item,
    "a spell to counter, or none to play four Birds",
);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Counter,
    Birds,
}

pub fn mode_of(item: &Item) -> Mode {
    if item
        .targets
        .iter()
        .any(|target| matches!(target, TargetRef::Item(_)))
    {
        Mode::Counter
    } else {
        Mode::Birds
    }
}

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != STAGE_LOCATE {
        return Vec::new();
    }
    location_options(ctx, item.controller)
}

fn flurry(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == STAGE_LOCATE {
        let Some(at) = ctx
            .picks()
            .first()
            .and_then(|zone| u16::try_from(*zone).ok())
            .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            .filter(|at| ctx.play_locations(seat).contains(at))
        else {
            return done();
        };
        play_birds(ctx, seat, at, BIRDS);
        return done();
    }
    match mode_of(item) {
        Mode::Counter => {
            counter_spell(ctx, item, 0);
            done()
        }
        Mode::Birds => {
            let locations = ctx.play_locations(seat);
            if locations.len() > 1 {
                return Flow::Ask(ctx.ask_resume(item, STAGE_LOCATE, 1, 1));
            }
            play_birds(ctx, seat, Location::Base(seat), BIRDS);
            done()
        }
    }
}

pub static CARD: Card = spell(
    "Flurry of Feathers",
    &[Keyword::Reaction],
    &[asking(
        with_candidates(play(&[COUNTERABLE], flurry), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::frisky_hunter::{
        bird_face_until_token_bird_lands, is_bird, BIRD, BIRD_ARRIVES_READY, BIRD_DEFLECT,
        BIRD_MIGHT,
    };
    use crate::cards::{script_of, Trigger, KIND_UNIT};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::engine::{priority, prompts};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const FLURRY: u32 = 90;
    const THEIR_FLURRY: u32 = 91;
    const MY_CALM: [u32; 3] = [46, 47, 48];
    const THEIR_CALM: [u32; 5] = [100, 101, 102, 103, 104];

    fn flurry_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Flurry of Feathers", 4, 2);
        card.domain = vec!["Calm".into()];
        card
    }

    fn roost() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(flurry_card(FLURRY, 0));
        fixture.table.cards.push(flurry_card(THEIR_FLURRY, 1));
        for rune in MY_CALM {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        for rune in THEIR_CALM {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Calm", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn birds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == BIRD && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_script_is_a_reaction_whose_optional_spell_pick_names_the_mode() {
        assert!(std::ptr::eq(
            script_of("Flurry of Feathers").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Flurry of Feathers");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, &[COUNTERABLE]);
        assert_eq!((COUNTERABLE.min, COUNTERABLE.max), (0, 1));
        assert_eq!(COUNTERABLE.kind, TargetKind::Item);
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert_eq!((BIRD_MIGHT, BIRD_DEFLECT, BIRDS), (1, 1, 4));
        const { assert!(!BIRD_ARRIVES_READY) };
        let face = bird_face_until_token_bird_lands();
        assert_eq!(face.name, BIRD);
        assert_eq!(face.kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(face.might, Some(BIRD_MIGHT));
        let mut item = ChainItem::new(1, ItemKind::Spell { card: FLURRY }, 0, Origin::Hand);
        assert_eq!(mode_of(&item), Mode::Birds);
        item.targets.push(TargetRef::Item(7));
        assert_eq!(mode_of(&item), Mode::Counter);
    }

    #[test]
    fn choosing_a_spell_counters_it_and_no_bird_is_played() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_FLURRY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 71} on the chain", "skip", "cancel"]
        );
        fixtures::choose(&mut ctx, 1, "{card 71} on the chain").unwrap();
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Item(1)]);
        assert_eq!(mode_of(&ctx.blob.chain[1]), Mode::Counter);
        assert_eq!(
            ctx.ready_runes_of(1).len(),
            3,
            "four energy off seven runes, two of which then recycle for the Calm power"
        );
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.log.contains(&"{card 71} is countered".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line == "{card 71} resolves"));
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(
            birds_of(&ctx, 1).is_empty(),
            "the other mode was not chosen"
        );
        assert!(ctx.blob.log.contains(&"{card 91} resolves".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_the_spell_plays_four_exhausted_birds_with_deflect_to_the_base() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLURRY).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing on the chain to counter: the optional pick is skipped unasked"
        );
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert_eq!(mode_of(&ctx.blob.chain[0]), Mode::Birds);
        assert!(birds_of(&ctx, 0).is_empty(), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "one play location: no question");
        assert!(ctx.blob.chain.is_empty());
        let birds = birds_of(&ctx, 0);
        assert_eq!(birds.len(), BIRDS);
        for bird in &birds {
            assert_eq!(ctx.location(*bird), Some(Location::Base(0)));
            assert!(ctx.is_unit(*bird));
            assert!(ctx.is_token(*bird), "179 · a token, not a card");
            assert!(is_bird(&ctx, *bird));
            assert_eq!(ctx.current_might(*bird), 1);
            assert!(ctx.card(*bird).unwrap().exhausted, "369.3");
            assert_eq!(ctx.deflect_of(*bird), 1);
            assert!(ctx.has_keyword(*bird, Keyword::Deflect(1)));
            assert!(ctx.events.iter().any(|event| matches!(
                event,
                Event::Played { card, controller: 0, origin: Origin::Board, .. } if card == bird
            )));
            assert!(ctx
                .blob
                .log
                .contains(&format!("{{seat 0}} plays {{card {bird}}} to their base")));
        }
        assert!(birds_of(&ctx, 1).is_empty());
        assert_eq!(ctx.card(FLURRY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_a_held_battlefield_the_birds_ask_where_they_land() {
        let mut fixture = roost();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLURRY).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_LOCATE
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {FLURRY}}}: choose {QUESTION} (0 of 1)")
        );
        assert!(birds_of(&ctx, 0).is_empty());
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        let birds = birds_of(&ctx, 0);
        assert_eq!(birds.len(), BIRDS);
        for bird in birds {
            assert_eq!(
                ctx.location(bird),
                Some(Location::Battlefield(fixtures::BF1))
            );
        }
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_countered_spell_that_already_left_the_chain_is_still_the_counter_mode() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_FLURRY).unwrap();
        fixtures::choose(&mut ctx, 1, "{card 71} on the chain").unwrap();
        ctx.blob.chain.remove(0);
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            birds_of(&ctx, 1).is_empty(),
            "the pick chose the counter mode; a stale target does not turn into Birds"
        );
    }

    #[test]
    fn an_ability_on_the_chain_is_refused_and_a_short_purse_never_opens_the_play() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        let mut trigger = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: fixtures::VI,
                index: 0,
            },
            0,
            Origin::Board,
        );
        trigger.status = crate::state::ItemStatus::Finalized;
        ctx.blob.chain.push(trigger);
        ctx.blob.priority = Some(crate::state::Priority {
            active: 0,
            passes: 0,
        });
        let probe = ChainItem::new(9, ItemKind::Spell { card: FLURRY }, 0, Origin::Hand);
        assert_eq!(
            crate::engine::targets::candidates(&ctx, &probe, &COUNTERABLE),
            Vec::<TargetRef>::new(),
            "an ability is not a spell"
        );
        fixtures::play_from_hand(&mut ctx, 0, FLURRY).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing to offer, so the pick is skipped and the Birds mode stands"
        );
        assert_eq!(mode_of(&ctx.blob.chain[1]), Mode::Birds);
        drop(ctx);

        let mut poor = roost();
        poor.table.cards.retain(|card| !MY_CALM.contains(&card.id));
        poor.resolve();
        let ctx = poor.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: FLURRY,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert!(
            legal::classify(&ctx, 0, &entry).is_err(),
            "four runes cannot pay four energy and two Calm"
        );
    }

    #[test]
    #[ignore = "engine gap · Token::Bird (1 Might, no domain, the Bird tag, [Deflect]) in engine/ctx.rs plus the manifest token, TOKEN_BIRD and is_token_name; frisky_hunter::spawn_bird builds the face by hand and grants the Deflect until the token's own face carries it"]
    fn a_bird_token_resolves_to_a_script_that_prints_its_deflect() {
        let mut fixture = roost();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLURRY).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let bird = birds_of(&ctx, 0)[0];
        assert!(crate::cards::is_token_name(BIRD));
        let script = ctx.script(bird).expect("the Bird token has a face");
        assert!(script.has_keyword(Keyword::Deflect(BIRD_DEFLECT)));
        assert!(ctx.state_of(bird).is_none_or(|row| row.granted.is_empty()));
    }
}
