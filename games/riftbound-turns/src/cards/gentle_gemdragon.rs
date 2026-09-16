use super::herald_of_scales::is_dragon;
use super::prelude::{asking, done, on_you_play_card, play, ready, unit, when, with_candidates};
use super::{Card, Flow, Item, Source, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};
use crate::state::TargetRef;

pub const RUNES: u8 = 2;
pub const QUESTION: &str = "up to two runes to ready";
pub const STAGE_RUNES: u8 = 1;

fn another_dragon(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Played { card, kind, .. } = event else {
        return false;
    };
    *card != source.card && kind == KIND_UNIT && is_dragon(ctx, *card)
}

fn exhausted_runes(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    ctx.runes_of(item.controller)
        .into_iter()
        .filter(|rune| rune.exhausted)
        .map(|rune| TargetRef::Card(rune.id))
        .collect()
}

fn gleam(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        STAGE_RUNES => {
            let offered = exhausted_runes(ctx, item, stage);
            let picked: Vec<u32> = ctx
                .picks()
                .iter()
                .copied()
                .filter(|rune| offered.contains(&TargetRef::Card(*rune)))
                .take(usize::from(RUNES))
                .collect();
            for rune in picked {
                if ready(ctx, rune) {
                    ctx.narrate(format!("{{card {rune}}} readies"));
                }
            }
            done()
        }
        _ => {
            if exhausted_runes(ctx, item, Stage(STAGE_RUNES)).is_empty() {
                ctx.narrate(format!("{{seat {seat}}} has no exhausted rune to ready"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_RUNES, 0, RUNES))
        }
    }
}

pub static CARD: Card = unit(
    "Gentle Gemdragon",
    &[],
    &[
        asking(with_candidates(play(&[], gleam), exhausted_runes), QUESTION),
        asking(
            with_candidates(
                when(on_you_play_card(&[], gleam), another_dragon),
                exhausted_runes,
            ),
            QUESTION,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::herald_of_scales::DRAGONS;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const GEMDRAGON: u32 = 90;
    const DRAKE: u32 = 91;
    const KNIGHT: u32 = 92;
    const SPENT_A: u32 = 46;
    const SPENT_B: u32 = 47;
    const SPENT_C: u32 = 48;

    fn gemdragon(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Body".into()],
            ..fixtures::unit(GEMDRAGON, zone, seat, "Gentle Gemdragon", 8)
        }
    }

    fn cheap_unit(id: u32, name: &str) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            ..fixtures::unit(id, fixtures::HAND, 0, name, 3)
        }
    }

    fn lair(gemdragon_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(gemdragon(gemdragon_zone, 0));
        fixture.table.cards.push(cheap_unit(DRAKE, "Dune Drake"));
        fixture.table.cards.push(cheap_unit(KNIGHT, "Brute"));
        for id in [SPENT_A, SPENT_B, SPENT_C] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Body", true));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn enter(ctx: &mut Ctx, card: u32) {
        play_engine::begin(ctx, 0, card, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(ctx).unwrap();
    }

    fn is_ready(ctx: &Ctx, rune: u32) -> bool {
        !ctx.card(rune).unwrap().exhausted
    }

    #[test]
    fn the_script_asks_the_same_rune_question_on_its_own_play_and_on_another_dragons() {
        assert!(std::ptr::eq(script_of("Gentle Gemdragon").unwrap(), &CARD));
        assert_eq!(CARD.name, "Gentle Gemdragon");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].condition.is_none());
        assert_eq!(CARD.abilities[1].trigger, Trigger::YouPlayCard);
        assert!(CARD.abilities[1].condition.is_some());
        for ability in CARD.abilities {
            assert!(ability.targets.is_empty());
            assert!(!ability.optional, "up to two: the skip is the zero");
            assert_eq!(ability.question, Some(QUESTION));
            assert!(ability.candidates.is_some());
        }
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(RUNES, 2);
    }

    #[test]
    fn the_dragons_are_read_off_the_heralds_list_which_names_the_unleashed_prints() {
        let mut fixture = lair(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(is_dragon(&ctx, GEMDRAGON));
        assert!(
            is_dragon(&ctx, DRAKE),
            "an Origins Dragon through the herald's list"
        );
        assert!(!is_dragon(&ctx, KNIGHT));
        assert!(!is_dragon(&ctx, fixtures::VI));
        assert!(!is_dragon(&ctx, 999));
        for name in ["Elder Dragon", "Gentle Gemdragon", "Inviolus Vox"] {
            assert!(DRAGONS.contains(&name), "{name} is on the herald's list");
        }
        let played = |card: u32| Event::Played {
            card,
            controller: 0,
            kind: KIND_UNIT.to_string(),
            origin: Origin::Hand,
            paid_additional: false,
        };
        let source = Source {
            card: GEMDRAGON,
            ability: 1,
        };
        assert!(another_dragon(&ctx, &played(DRAKE), source));
        assert!(!another_dragon(&ctx, &played(KNIGHT), source));
        assert!(
            !another_dragon(&ctx, &played(GEMDRAGON), source),
            "its own play is the first ability's"
        );
    }

    #[test]
    fn playing_it_offers_the_exhausted_runes_and_readies_up_to_two_picks() {
        let mut fixture = lair(fixtures::HAND);
        let mut ctx = fixture.ctx();
        enter(&mut ctx, GEMDRAGON);
        assert!(ctx.blob.prompt.is_none(), "the pick waits for the trigger");
        assert_eq!(ctx.blob.chain.len(), 1, "one trigger, not two");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == GEMDRAGON
        ));
        fixtures::pass_until_open(&mut ctx);
        let item = ctx.blob.chain[0].id;
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: STAGE_RUNES
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {SPENT_A}}}"),
                format!("{{card {SPENT_B}}}"),
                format!("{{card {SPENT_C}}}"),
                "done".to_string(),
                "skip".to_string()
            ],
            "only the exhausted runes"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {GEMDRAGON}}}: choose {QUESTION} (0 of 2)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SPENT_A}}}")).unwrap();
        assert!(ctx.blob.prompt.is_some(), "a second pick is open");
        fixtures::choose(&mut ctx, 0, &format!("{{card {SPENT_C}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(is_ready(&ctx, SPENT_A));
        assert!(is_ready(&ctx, SPENT_C));
        assert!(!is_ready(&ctx, SPENT_B), "two, not three");
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| **effect == Effect::ready(SPENT_A)
                    || **effect == Effect::ready(SPENT_C))
                .count(),
            2
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SPENT_A}}} readies")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn another_dragon_you_play_asks_too_but_a_plain_unit_or_the_opponents_dragon_does_not() {
        let mut fixture = lair(fixtures::BASE);
        let mut ctx = fixture.ctx();
        enter(&mut ctx, KNIGHT);
        assert!(ctx.blob.chain.is_empty(), "a Brute is no Dragon");
        enter(&mut ctx, DRAKE);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == GEMDRAGON
        ));
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                stage: STAGE_RUNES,
                ..
            })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {SPENT_B}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(is_ready(&ctx, SPENT_B));
        assert!(!is_ready(&ctx, SPENT_A));
        drop(ctx);
        let mut theirs = lair(fixtures::BASE);
        theirs.table.card_mut(DRAKE).unwrap().seat = 1;
        theirs.table.card_mut(DRAKE).unwrap().owner = 1;
        theirs.resolve();
        let mut ctx = theirs.ctx();
        play_engine::begin(&mut ctx, 1, DRAKE, Origin::Hand, Some(Location::Base(1))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "their Dragon is not one you play"
        );
    }

    #[test]
    fn with_no_exhausted_rune_the_trigger_resolves_without_asking() {
        let mut fixture = lair(fixtures::HAND);
        for rune in [SPENT_A, SPENT_B, SPENT_C] {
            fixture.table.card_mut(rune).unwrap().exhausted = false;
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        enter(&mut ctx, GEMDRAGON);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no exhausted rune to ready".to_string()));
    }

    #[test]
    #[ignore = "CardInfo carries no tags: is_dragon reads the printed name over herald_of_scales::DRAGONS, so a Dragon-tagged unit under a name the list does not know is invisible; the engine owes a tags row on the face and Filter::Tag"]
    fn a_dragon_tagged_unit_under_a_name_the_lists_do_not_know_still_counts() {
        let mut fixture = lair(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::unit(200, fixtures::BASE, 0, "Wyrmling", 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_dragon(&ctx, 200));
    }
}
