use super::prelude::{
    asking, done, play, spell, target, with_candidates, LimitedPlay, Location, Price,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::engine::{cost, pay, targets};
use crate::state::{Leave, Origin, TargetRef};

pub const QUESTION: &str = "a unit in your trash to play";
pub const STAGE_PICK: u8 = 1;

pub const UNIT_IN_TRASH: TargetSpec = target(
    Filter::And(&[Filter::Kind(KIND_UNIT), Filter::InTrash, Filter::Friendly]),
    0,
    1,
    TargetKind::Card,
    "a unit in your trash",
);

pub fn recruit_locations(ctx: &Ctx, seat: u8, unit: u32) -> Vec<Location> {
    ctx.limited_play_locations(seat, unit, &ctx.play_locations(seat))
}

pub fn playable_units(ctx: &Ctx, item: &Item, spec: &TargetSpec, price: Price) -> Vec<u32> {
    let seat = item.controller;
    targets::candidates(ctx, item, spec)
        .into_iter()
        .filter_map(|unit| match unit {
            TargetRef::Card(unit) => Some(unit),
            _ => None,
        })
        .filter(|unit| pay::affordable(ctx, seat, &cost::priced(ctx, *unit, price, false)))
        .filter(|unit| !recruit_locations(ctx, seat, *unit).is_empty())
        .collect()
}

pub fn recruit_candidates(
    ctx: &Ctx,
    item: &Item,
    stage: Stage,
    spec: &TargetSpec,
    price: Price,
) -> Vec<TargetRef> {
    if stage.0 != STAGE_PICK {
        return Vec::new();
    }
    playable_units(ctx, item, spec, price)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub fn play_from_trash(
    ctx: &mut Ctx,
    item: &Item,
    unit: u32,
    locations: Vec<Location>,
    price: Price,
) -> bool {
    let seat = item.controller;
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {unit}}} from the trash for {}",
        cost::priced(ctx, unit, price, false).label()
    ));
    ctx.play_limited(LimitedPlay {
        card: unit,
        by: seat,
        origin: Origin::Trash {
            leave: Leave::Recycle,
        },
        locations,
        price,
    })
    .is_ok()
}

pub fn recruit(
    ctx: &mut Ctx,
    item: &Item,
    stage: Stage,
    spec: &TargetSpec,
    price: Price,
    min: u8,
) -> Flow {
    let seat = item.controller;
    if stage.0 == STAGE_PICK {
        let units = playable_units(ctx, item, spec, price);
        let Some(unit) = ctx
            .picks()
            .first()
            .copied()
            .filter(|unit| units.contains(unit))
        else {
            return done();
        };
        let locations = recruit_locations(ctx, seat, unit);
        play_from_trash(ctx, item, unit, locations, price);
        return done();
    }
    if playable_units(ctx, item, spec, price).is_empty() {
        ctx.narrate(format!(
            "{{card {}}} finds no unit to play",
            item.kind.source()
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, STAGE_PICK, min, 1))
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    recruit_candidates(ctx, item, stage, &UNIT_IN_TRASH, Price::PowerOnly)
}

fn harrow(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    recruit(ctx, item, stage, &UNIT_IN_TRASH, Price::PowerOnly, 1)
}

pub static CARD: Card = spell(
    "The Harrowing",
    &[],
    &[asking(
        with_candidates(play(&[], harrow), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::cards::prelude::{draw, unit as unit_card};
    use crate::cards::script_of;
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    pub const HARROWING: u32 = 90;
    pub const CHEAP: u32 = 91;
    pub const DEAR: u32 = 92;
    pub const THEIRS: u32 = 93;
    const GRIMWYRM: u32 = 94;

    pub static DRAWS: Card = unit_card(
        "Crab",
        &[],
        &[play(&[], |ctx, item, _| {
            draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    pub fn trashed(id: u32, seat: u8, name: &str, energy: u8, power: u8) -> CardInfo {
        CardInfo {
            energy: Some(energy),
            power: Some(power),
            domain: vec!["Fury".into()],
            ..fixtures::unit(id, fixtures::TRASH, seat, name, 2)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut harrowing = fixtures::spell(HARROWING, fixtures::HAND, 0, "The Harrowing", 0, 0);
        harrowing.domain = vec!["Chaos".into()];
        fixture.table.cards.push(harrowing);
        fixture
            .table
            .cards
            .push(trashed(CHEAP, 0, "Big Lizard", 6, 1));
        fixture.table.cards.push(trashed(DEAR, 0, "Titan", 8, 5));
        fixture.table.cards.push(trashed(THEIRS, 1, "Jinx", 1, 0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_sorcery_spell_that_asks_at_resolution() {
        assert!(std::ptr::eq(script_of("The Harrowing").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_unit_is_offered_only_if_its_power_can_be_paid_and_is_played_for_power_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HARROWING).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CHEAP}}}")],
            "the 5-power Titan and the opponent's unit are not offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {HARROWING}}}: choose {QUESTION} (0 of 1)")
        );
        let runes = ctx.runes_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHEAP}}}")).unwrap();
        assert_eq!(
            ctx.location(CHEAP),
            Some(Location::Base(0)),
            "one location: no question · {:?}",
            ctx.blob.log
        );
        assert!(ctx.card(CHEAP).unwrap().exhausted);
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "one rune recycled for the power"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready,
            "no rune exhausted for energy"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Trash { leave: Leave::Recycle }, .. } if *card == CHEAP
        )));
        assert_eq!(ctx.trash_of(0), vec![DEAR, HARROWING]);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {CHEAP}}} from the trash for 1 Fury power"
        )));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_lone_candidate_is_played_unasked_and_its_play_trigger_fires_again() {
        let mut fixture = armed();
        fixture.table.cards.retain(|card| card.id != DEAR);
        fixture.table.card_mut(CHEAP).unwrap().name = "Crab".into();
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(CHEAP, &DRAWS);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, HARROWING).unwrap();
        fixtures::pass_until_open(&mut ctx);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", fixtures::labels(&ctx));
        assert_eq!(ctx.location(CHEAP), Some(Location::Base(0)));
        assert_eq!(ctx.blob.chain.len(), 1, "the Crab's own play trigger");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CHEAP
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_held_battlefield_is_asked_for_as_a_location_and_the_power_is_paid_after_the_choice() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HARROWING).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let runes = ctx.runes_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHEAP}}}")).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ],
            "the base and the held battlefield, nothing to cancel"
        );
        assert_eq!(ctx.runes_of(0).len(), runes, "not paid yet");
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(CHEAP),
            Some(Location::Battlefield(fixtures::BF1)),
            "{:?}",
            ctx.blob.log
        );
        assert_eq!(ctx.runes_of(0).len(), runes - 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_unaffordable_or_empty_trash_plays_nothing_and_the_opponent_cannot_answer() {
        let mut fixture = armed();
        fixture.table.cards.retain(|card| card.id != CHEAP);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HARROWING).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.trash_of(0), vec![DEAR, HARROWING]);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {HARROWING}}} finds no unit to play")));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HARROWING).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(CHEAP),
            Some(Location::Base(0)),
            "the lone candidate answers itself once the table settles"
        );
    }

    #[test]
    fn a_perched_grimwyrm_is_offered_only_with_a_conquest_to_land_on_and_never_goes_to_the_base() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(trashed(GRIMWYRM, 0, "Perched Grimwyrm", 0, 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.play_locations(0),
            [Location::Base(0)],
            "the seat's own list still holds the base"
        );
        assert!(
            recruit_locations(&ctx, 0, GRIMWYRM).is_empty(),
            "nothing conquered this turn: nowhere he may be played"
        );
        assert_eq!(recruit_locations(&ctx, 0, CHEAP), [Location::Base(0)]);
        fixtures::play_from_hand(&mut ctx, 0, HARROWING).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CHEAP}}}")],
            "he is not offered · his restriction leaves the effect nowhere to play him"
        );
        drop(ctx);

        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(trashed(GRIMWYRM, 0, "Perched Grimwyrm", 0, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(95, fixtures::BF1, 0, "Anchor", 2));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(
            recruit_locations(&ctx, 0, GRIMWYRM),
            [Location::Battlefield(fixtures::BF1)],
            "the effect's base and held battlefield, cut to where he may go"
        );
        fixtures::play_from_hand(&mut ctx, 0, HARROWING).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CHEAP}}}"), format!("{{card {GRIMWYRM}}}")]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {GRIMWYRM}}}")).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one place he may go: the base is never asked · {:?}",
            fixtures::labels(&ctx)
        );
        assert_eq!(
            ctx.location(GRIMWYRM),
            Some(Location::Battlefield(fixtures::BF1)),
            "{:?}",
            ctx.blob.log
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_engines_own_location_prompt_serves_a_limited_play_mid_resolution() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HARROWING).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let locations = ctx.play_locations(0);
        ctx.play_limited(LimitedPlay {
            card: CHEAP,
            by: 0,
            origin: Origin::Trash {
                leave: Leave::Recycle,
            },
            locations,
            price: Price::PowerOnly,
        })
        .unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        assert_eq!(ctx.card(CHEAP).unwrap().zone, Some(fixtures::CHAIN));
    }
}
