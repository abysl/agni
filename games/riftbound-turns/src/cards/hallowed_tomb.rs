use super::prelude::{asking, battlefield, done, optional, triggered, when, with_candidates};
use super::{base_name, Card, Flow, Item, Source, Stage, Trigger, Who, KIND_LEGEND};
use crate::engine::ctx::{Ctx, Event};
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const QUESTION: &str = "your Chosen Champion to return to your Champion Zone";
const RETURN: u8 = 1;
const TITLE_SEPARATOR: &str = " - ";

fn champion_tag(name: &str) -> &str {
    let name = base_name(name);
    name.split(TITLE_SEPARATOR).next().unwrap_or(name)
}

fn legend_tag(ctx: &Ctx, seat: u8) -> Option<String> {
    let legend = ctx.zones.legend?;
    ctx.table
        .held(legend, seat)
        .find(|card| card.is_kind(KIND_LEGEND))
        .map(|card| champion_tag(&card.name).to_string())
}

pub fn is_chosen_champion(ctx: &Ctx, seat: u8, card: u32) -> bool {
    let Some(held) = ctx.card(card) else {
        return false;
    };
    if held.owner != seat || !ctx.is_unit(card) {
        return false;
    }
    match ctx.chosen_champion(seat) {
        Some(name) => base_name(&held.name) == name,
        None => legend_tag(ctx, seat).is_some_and(|tag| champion_tag(&held.name) == tag),
    }
}

fn champion_zone_is_empty(ctx: &Ctx, seat: u8) -> bool {
    ctx.zones
        .champion
        .is_some_and(|zone| ctx.table.held(zone, seat).next().is_none())
}

fn chosen_champions_in_trash(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.trash_of(seat)
        .into_iter()
        .filter(|card| is_chosen_champion(ctx, seat, *card))
        .collect()
}

fn a_champion_waits_in_the_trash(ctx: &Ctx, event: &Event, _: Source) -> bool {
    let Event::Held { seat, .. } = event else {
        return false;
    };
    champion_zone_is_empty(ctx, *seat) && !chosen_champions_in_trash(ctx, *seat).is_empty()
}

fn champions_in_trash(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    chosen_champions_in_trash(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn may_return_the_champion(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == RETURN {
        let offered = chosen_champions_in_trash(ctx, seat);
        let picked = ctx
            .picks()
            .first()
            .copied()
            .filter(|card| offered.contains(card));
        let (Some(card), Some(zone)) = (picked, ctx.zones.champion) else {
            ctx.narrate(format!(
                "{{seat {seat}}} leaves their champion in the trash"
            ));
            return done();
        };
        if !champion_zone_is_empty(ctx, seat) {
            ctx.narrate(format!(
                "{{seat {seat}}}'s Champion Zone is no longer empty"
            ));
            return done();
        }
        ctx.emit(Effect::Move {
            card,
            zone,
            seat,
            index: TOP,
        });
        ctx.narrate(format!(
            "{{seat {seat}}} returns {{card {card}}} to their Champion Zone"
        ));
        return done();
    }
    if !champion_zone_is_empty(ctx, seat) || chosen_champions_in_trash(ctx, seat).is_empty() {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, RETURN, 0, 1))
}

pub static CARD: Card = battlefield(
    "Hallowed Tomb",
    &[],
    &[asking(
        with_candidates(
            when(
                optional(triggered(
                    Trigger::Hold(Who::You),
                    &[],
                    may_return_the_champion,
                )),
                a_champion_waits_in_the_trash,
            ),
            champions_in_trash,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::cleanup;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};

    const TOMB: u32 = fixtures::GROUNDS;
    const FALLEN_LILLIA: u32 = 90;
    const OTHER_UNIT_IN_TRASH: u32 = 91;
    const SECOND_LILLIA: u32 = 92;

    fn tomb_held_by_me() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(TOMB).unwrap().name = "Hallowed Tomb".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(TOMB).unwrap(), &CARD));
        fixture
    }

    fn with_the_champion_fallen() -> Fixture {
        let mut fixture = tomb_held_by_me();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::CHAMPION_CARD);
        fixture.table.cards.push(fixtures::unit(
            FALLEN_LILLIA,
            fixtures::TRASH,
            0,
            "Lillia - Fae Fawn",
            3,
        ));
        fixture.table.cards.push(fixtures::unit(
            OTHER_UNIT_IN_TRASH,
            fixtures::TRASH,
            0,
            "Jinx",
            2,
        ));
        fixture.resolve();
        fixture
    }

    fn tomb_items(ctx: &Ctx) -> Vec<u8> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, .. } if source == TOMB => Some(item.controller),
                _ => None,
            })
            .collect()
    }

    fn hold(ctx: &mut Ctx) {
        assert_eq!(cleanup::score_holds(ctx, 0), [fixtures::BF1]);
        settle(ctx).unwrap();
    }

    fn champion_zone(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::CHAMPION, seat)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_tomb_is_a_conditional_optional_hold_trigger_that_asks_at_resolution() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Hallowed Tomb").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Hold(Who::You));
        assert!(ability.optional);
        assert!(ability.condition.is_some());
        assert!(ability.candidates.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(champion_tag("Lillia - Fae Fawn"), "Lillia");
        assert_eq!(champion_tag("Jinx - Rebel (Alternate Art)"), "Jinx");
        assert_eq!(champion_tag("Vi"), "Vi");
    }

    #[test]
    fn holding_with_an_empty_champion_zone_offers_the_fallen_champion_and_returns_it() {
        let mut fixture = with_the_champion_fallen();
        let mut ctx = fixture.ctx();
        assert!(champion_zone(&ctx, 0).is_empty());
        assert!(is_chosen_champion(&ctx, 0, FALLEN_LILLIA));
        assert!(!is_chosen_champion(&ctx, 0, OTHER_UNIT_IN_TRASH));
        hold(&mut ctx);
        assert_eq!(tomb_items(&ctx), [0]);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: RETURN
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {FALLEN_LILLIA}}}"), "skip".to_string()],
            "only the Chosen Champion, never another unit in the trash"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {TOMB}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {FALLEN_LILLIA}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: FALLEN_LILLIA,
            zone: fixtures::CHAMPION,
            seat: 0,
            index: TOP
        }));
        assert_eq!(champion_zone(&ctx, 0), [FALLEN_LILLIA]);
        assert!(!ctx.trash_of(0).contains(&FALLEN_LILLIA));
        assert!(ctx.trash_of(0).contains(&OTHER_UNIT_IN_TRASH));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} returns {{card {FALLEN_LILLIA}}} to their Champion Zone"
        )));
        assert_eq!(ctx.points(0), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_leaves_the_champion_in_the_trash() {
        let mut fixture = with_the_champion_fallen();
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(champion_zone(&ctx, 0).is_empty());
        assert!(ctx.trash_of(0).contains(&FALLEN_LILLIA));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} leaves their champion in the trash".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_occupied_champion_zone_or_an_empty_trash_never_triggers_the_tomb() {
        let mut fixture = tomb_held_by_me();
        fixture.table.cards.push(fixtures::unit(
            FALLEN_LILLIA,
            fixtures::TRASH,
            0,
            "Lillia - Fae Fawn",
            3,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(champion_zone(&ctx, 0), [fixtures::CHAMPION_CARD]);
        hold(&mut ctx);
        assert!(tomb_items(&ctx).is_empty(), "the zone is not empty");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.points(0), 1);
        drop(ctx);
        let mut nobody = tomb_held_by_me();
        nobody
            .table
            .cards
            .retain(|card| card.id != fixtures::CHAMPION_CARD);
        nobody.table.cards.push(fixtures::unit(
            OTHER_UNIT_IN_TRASH,
            fixtures::TRASH,
            0,
            "Jinx",
            2,
        ));
        nobody.resolve();
        let mut ctx = nobody.ctx();
        hold(&mut ctx);
        assert!(
            tomb_items(&ctx).is_empty(),
            "no Chosen Champion in the trash"
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(champion_zone(&ctx, 0).is_empty());
    }

    #[test]
    fn only_the_champion_chosen_at_deck_building_is_offered() {
        let mut fixture = with_the_champion_fallen();
        fixture.table.cards.push(fixtures::unit(
            SECOND_LILLIA,
            fixtures::TRASH,
            0,
            "Lillia - Dreaming Fawn",
            4,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        hold(&mut ctx);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {FALLEN_LILLIA}}}"), "skip".to_string()]
        );
    }
}
