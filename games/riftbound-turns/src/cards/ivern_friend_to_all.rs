use super::daisy::{is_bird, is_cat, is_dog};
use super::poro_herder::is_poro;
use super::prelude::{done, friendly_units, on_conquer_me, on_hold_me, play, score_point, unit};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const TAGS: [&str; 4] = ["Bird", "Cat", "Dog", "Poro"];

pub fn chosen_tag(_ctx: &Ctx, _ivern: u32) -> Option<&'static str> {
    None
}

pub fn has_tag(ctx: &Ctx, unit: u32, tag: &str) -> bool {
    let printed = match tag {
        "Bird" => is_bird(ctx, unit),
        "Cat" => is_cat(ctx, unit),
        "Dog" => is_dog(ctx, unit),
        "Poro" => is_poro(ctx, unit),
        _ => false,
    };
    printed || chosen_tag(ctx, unit) == Some(tag)
}

pub fn your_units_span_the_four_tags(ctx: &Ctx, seat: u8) -> bool {
    let units = friendly_units(ctx, seat);
    TAGS.iter()
        .all(|tag| units.iter().any(|unit| has_tag(ctx, *unit, tag)))
}

pub fn gain_a_chosen_tag(ctx: &mut Ctx, item: &Item) -> Option<&'static str> {
    let me = item.kind.source();
    let seat = item.controller;
    ctx.narrate(format!(
        "{{seat {seat}}} chooses Bird, Cat, Dog or Poro for {{card {me}}} (the engine keeps no tag on the face yet)"
    ));
    chosen_tag(ctx, me)
}

fn befriend(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_a_chosen_tag(ctx, item);
    done()
}

fn menagerie(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    if your_units_span_the_four_tags(ctx, seat) {
        ctx.narrate(format!(
            "{{card {me}}} · a Bird, a Cat, a Dog and a Poro among {{seat {seat}}}'s units"
        ));
        score_point(ctx, seat);
    } else {
        ctx.narrate(format!("{{card {me}}} · the menagerie is incomplete"));
    }
    done()
}

pub static CARD: Card = unit(
    "Ivern - Friend to All",
    &[],
    &[
        play(&[], befriend),
        on_conquer_me(&[], menagerie),
        on_hold_me(&[], menagerie),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const IVERN: u32 = 90;
    const BIRD: u32 = 91;
    const CAT: u32 = 92;
    const DOG: u32 = 93;
    const PORO: u32 = 94;

    fn ivern(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(1),
            power: None,
            domain: vec!["Order".into()],
            ..fixtures::unit(IVERN, zone, seat, "Ivern - Friend to All", 6)
        }
    }

    fn friends(zone: u16, names: &[(u32, &str)]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ivern(zone, 0));
        for (id, name) in names {
            fixture
                .table
                .cards
                .push(fixtures::unit(*id, fixtures::BASE, 0, name, 1));
        }
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    const ALL_FOUR: [(u32, &str); 4] = [
        (BIRD, "Soaring Scout"),
        (CAT, "Pakaa Cub"),
        (DOG, "Loyal Pup"),
        (PORO, "Pouty Poro"),
    ];

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_play_trigger_and_a_conquer_and_a_hold_trigger_of_his_own() {
        assert!(std::ptr::eq(
            script_of("Ivern - Friend to All").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Ivern - Friend to All");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 3);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[1].trigger, Trigger::Conquer(Who::Me));
        assert_eq!(CARD.abilities[2].trigger, Trigger::Hold(Who::Me));
        for ability in CARD.abilities {
            assert!(ability.targets.is_empty());
            assert!(!ability.optional);
            assert!(ability.cost.is_none() && ability.condition.is_none());
        }
        assert_eq!(TAGS, ["Bird", "Cat", "Dog", "Poro"]);
    }

    #[test]
    fn the_condition_needs_all_four_tags_among_his_controllers_units() {
        let mut fixture = friends(fixtures::BF1, &ALL_FOUR);
        let ctx = fixture.ctx();
        assert!(your_units_span_the_four_tags(&ctx, 0));
        assert!(!your_units_span_the_four_tags(&ctx, 1));
        assert!(has_tag(&ctx, BIRD, "Bird") && !has_tag(&ctx, BIRD, "Cat"));
        assert!(has_tag(&ctx, PORO, "Poro"));
        assert!(
            !has_tag(&ctx, IVERN, "Bird"),
            "seam · no chosen tag on the face"
        );
        assert_eq!(chosen_tag(&ctx, IVERN), None);
        drop(ctx);
        let mut fixture = friends(fixtures::BF1, &ALL_FOUR[..3]);
        let ctx = fixture.ctx();
        assert!(!your_units_span_the_four_tags(&ctx, 0), "no Poro");
        drop(ctx);
        let mut fixture = friends(fixtures::BF1, &ALL_FOUR);
        fixture.table.card_mut(DOG).unwrap().seat = 1;
        fixture.table.card_mut(DOG).unwrap().owner = 1;
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(
            !your_units_span_the_four_tags(&ctx, 0),
            "their Dog is not yours"
        );
    }

    #[test]
    fn conquering_with_the_menagerie_complete_scores_a_second_point_when_the_trigger_resolves() {
        let mut fixture = friends(fixtures::BF1, &ALL_FOUR);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Conquered { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![IVERN]
        )));
        assert_eq!(ctx.points(0), 1, "the conquer's point; the trigger waits");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == IVERN
        ));
        assert!(ctx.blob.prompt.is_none(), "he asks nothing");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 2);
        assert_eq!(ctx.points(1), 0);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {IVERN}}} · a Bird, a Cat, a Dog and a Poro among {{seat 0}}'s units"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn holding_with_it_complete_scores_too_and_the_condition_is_read_at_resolution() {
        let mut fixture = friends(fixtures::BF1, &ALL_FOUR);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 2 } if source == IVERN
        ));
        assert_eq!(ctx.points(0), 1);
        ctx.kill(PORO, crate::engine::ctx::Cause::Cleanup { last_item: None });
        settle(&mut ctx).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.points(0), 1, "the Poro died in response");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {IVERN}}} · the menagerie is incomplete")));
    }

    #[test]
    fn a_missing_tag_scores_nothing_and_a_conquer_without_him_is_not_his() {
        let mut fixture = friends(fixtures::BF1, &ALL_FOUR[..3]);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        drop(ctx);
        let mut fixture = friends(fixtures::BASE, &ALL_FOUR);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "Vi conquered; he stayed home");
        assert_eq!(ctx.points(0), 1);
    }

    #[test]
    fn playing_him_narrates_the_owed_choice_and_gains_no_tag_today() {
        let mut fixture = friends(fixtures::HAND, &ALL_FOUR);
        fixture.blob.set_contested(fixtures::BF1, None);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, IVERN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "seam · no named modes to offer");
        assert!(ctx.on_board(IVERN));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} chooses Bird, Cat, Dog or Poro for {{card {IVERN}}} (the engine keeps no tag on the face yet)"
        )));
        assert_eq!(chosen_tag(&ctx, IVERN), None);
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · tags on the face and named modes: CardInfo carries no tags and Resume options are TargetRefs, so the play trigger cannot offer Bird, Cat, Dog or Poro and CardState keeps no chosen tag for chosen_tag to read; with both, he counts as the tag he chose"]
    fn his_chosen_tag_completes_the_menagerie() {
        let mut fixture = friends(fixtures::HAND, &ALL_FOUR[..3]);
        fixture.blob.set_contested(fixtures::BF1, None);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, IVERN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, "Poro").unwrap();
        assert_eq!(chosen_tag(&ctx, IVERN), Some("Poro"));
        assert!(has_tag(&ctx, IVERN, "Poro"));
        assert!(your_units_span_the_four_tags(&ctx, 0));
    }
}
