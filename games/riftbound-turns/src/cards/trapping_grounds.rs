use super::frisky_hunter::spawn_bird;
use super::prelude::{
    asking, battlefield, excess_damage_assigned_in_my_attack, on_conquer, when, with_candidates,
    Location,
};
use super::{Ability, Card, Event, Flow, Item, Source, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const EXCESS_FOR_THE_BIRD: u8 = 3;
pub const LOCATE: u8 = 1;
pub const QUESTION: &str = "where the Bird is played";

pub fn conquered_after_an_attack_with_three_excess(
    ctx: &Ctx,
    event: &Event,
    source: Source,
) -> bool {
    let Event::Conquered { zone, seat, .. } = event else {
        return false;
    };
    ctx.card(source.card).and_then(|held| held.zone) == Some(*zone)
        && excess_damage_assigned_in_my_attack(ctx, *seat, *zone)
            .is_some_and(|excess| excess >= EXCESS_FOR_THE_BIRD)
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
        LOCATE => location_options(ctx, item.controller),
        _ => Vec::new(),
    }
}

pub fn play_a_bird(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == LOCATE {
        let at = ctx
            .picks()
            .first()
            .and_then(|zone| u16::try_from(*zone).ok())
            .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            .filter(|at| ctx.play_locations(seat).contains(at))
            .unwrap_or(Location::Base(seat));
        spawn_bird(ctx, seat, at);
        return Flow::Done;
    }
    let locations = ctx.play_locations(seat);
    if locations.len() > 1 {
        return Flow::Ask(ctx.ask_resume(item, LOCATE, 1, 1));
    }
    let at = locations.first().copied().unwrap_or(Location::Base(seat));
    spawn_bird(ctx, seat, at);
    Flow::Done
}

pub const WHEN_YOU_CONQUER_HERE: Ability = asking(
    with_candidates(
        when(
            on_conquer(&[], play_a_bird),
            conquered_after_an_attack_with_three_excess,
        ),
        candidates,
    ),
    QUESTION,
);

pub static CARD: Card = battlefield("Trapping Grounds", &[], &[WHEN_YOU_CONQUER_HERE]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::frisky_hunter::is_bird;
    use crate::cards::{script_of, Keyword, Trigger, Who};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle, showdown};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const GROUNDS: u32 = fixtures::GROUNDS;
    const HUNTER: u32 = 90;
    const PREY: u32 = 91;
    const GUARD: u32 = 92;

    static WIRED_WITHOUT_THE_EXCESS_FOR_THE_TEST: Card = battlefield(
        "Trapping Grounds",
        &[],
        &[asking(
            with_candidates(on_conquer(&[], play_a_bird), candidates),
            QUESTION,
        )],
    );

    fn grounds() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(GROUNDS).unwrap().name = "Trapping Grounds".into();
        fixture
            .table
            .cards
            .push(fixtures::unit(HUNTER, fixtures::BF1, 0, "Hunter", 6));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GROUNDS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn wired() -> Fixture {
        let mut fixture = grounds();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GROUNDS, &WIRED_WITHOUT_THE_EXCESS_FOR_THE_TEST);
        fixture
    }

    fn against(prey_might: u8) -> Fixture {
        let mut fixture = grounds();
        fixture
            .table
            .cards
            .push(fixtures::unit(PREY, fixtures::BF1, 1, "Prey", prey_might));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn fight(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        let open = ctx.blob.showdown.clone().expect("a combat opens");
        assert!(open.combat);
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        settle(ctx).unwrap();
    }

    fn conquer(ctx: &mut Ctx, seat: u8) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            Established::Conquered(seat)
        );
        settle(ctx).unwrap();
    }

    fn grounds_items(ctx: &Ctx) -> Vec<u16> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(
                |item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == GROUNDS),
            )
            .map(|item| item.id)
            .collect()
    }

    fn birds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| is_bird(ctx, card.id) && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_grounds_are_one_conditioned_conquer_trigger_that_asks_where_the_bird_lands() {
        assert!(std::ptr::eq(script_of("Trapping Grounds").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::You));
        assert!(std::ptr::fn_addr_eq(
            ability.condition.unwrap(),
            conquered_after_an_attack_with_three_excess as fn(&Ctx, &Event, Source) -> bool
        ));
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(!ability.optional);
        assert_eq!(EXCESS_FOR_THE_BIRD, 3);
    }

    #[test]
    fn the_bird_it_plays_is_frisky_hunters_exhausted_one_might_bird_with_deflect() {
        let mut fixture = grounds();
        let mut ctx = fixture.ctx();
        let bird = spawn_bird(&mut ctx, 0, Location::Base(0)).unwrap();
        assert!(is_bird(&ctx, bird));
        assert!(!is_bird(&ctx, fixtures::VI));
        assert_eq!(birds_of(&ctx, 0), [bird]);
        assert_eq!(ctx.location(bird), Some(Location::Base(0)));
        assert_eq!(ctx.controller(bird), 0);
        assert_eq!(ctx.current_might(bird), 1);
        assert!(ctx.card(bird).unwrap().exhausted);
        assert!(ctx.has_keyword(bird, Keyword::Deflect(1)));
        assert!(ctx.is_token(bird), "a spawned face is a token to the table");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_condition_reads_a_conquer_here_with_three_excess_and_nothing_else() {
        let mut fixture = against(4);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 6 might vs defenders 4 might"));
        assert!(!ctx.on_board(PREY), "six on four is lethal");
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        let conquered = Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![HUNTER],
        };
        let source = Source {
            card: GROUNDS,
            ability: 0,
        };
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(2),
            "two excess is not three"
        );
        assert!(!conquered_after_an_attack_with_three_excess(
            &ctx, &conquered, source
        ));
        assert!(
            grounds_items(&ctx).is_empty(),
            "two excess is recorded, so no Bird today"
        );
        assert!(birds_of(&ctx, 0).is_empty());
        ctx.record_excess_damage(0, fixtures::BF1, 3);
        assert!(conquered_after_an_attack_with_three_excess(
            &ctx, &conquered, source
        ));
        assert!(!conquered_after_an_attack_with_three_excess(
            &ctx,
            &Event::Conquered {
                zone: fixtures::BF2,
                seat: 0,
                units: vec![HUNTER],
            },
            source
        ));
        assert!(!conquered_after_an_attack_with_three_excess(
            &ctx,
            &Event::Conquered {
                zone: fixtures::BF1,
                seat: 1,
                units: vec![HUNTER],
            },
            source
        ));
    }

    #[test]
    fn wired_to_every_conquer_the_holder_picks_where_the_bird_lands_when_two_locations_are_open() {
        let mut fixture = wired();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx, 0);
        assert_eq!(ctx.points(0), 1);
        let items = grounds_items(&ctx);
        assert_eq!(items.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: items[0],
                stage: LOCATE
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ],
            "the base and the battlefield just conquered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {GROUNDS}}}: choose where the Bird is played (0 of 1)")
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0);
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 1
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        let birds = birds_of(&ctx, 0);
        assert_eq!(birds.len(), 1);
        assert_eq!(
            ctx.location(birds[0]),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.has_keyword(birds[0], Keyword::Deflect(1)));
        assert!(ctx.card(birds[0]).unwrap().exhausted);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn wired_after_a_fight_every_held_battlefield_is_offered_and_the_base_is_a_fine_answer() {
        let mut fixture = wired();
        fixture.blob.set_holder(fixtures::BF2, Some(0));
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().name = "Open Ground".into();
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::unit(GUARD, fixtures::BF2, 0, "Guard", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(PREY, fixtures::BF1, 1, "Prey", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GROUNDS, &WIRED_WITHOUT_THE_EXCESS_FOR_THE_TEST);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert!(!ctx.on_board(PREY));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(grounds_items(&ctx).len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1),
                format!("{{zone {}}}", fixtures::BF2)
            ],
            "the base and both held battlefields (Rockfall Path, which refuses plays, is renamed for the test)"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BASE)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        let birds = birds_of(&ctx, 0);
        assert_eq!(birds.len(), 1);
        assert_eq!(ctx.location(birds[0]), Some(Location::Base(0)));
        assert!(ctx.on_board(HUNTER), "the Hunter keeps the grounds");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponent_conquering_is_not_asked_by_the_holders_seat_and_a_conquer_elsewhere_is_silent()
    {
        let mut theirs = wired();
        theirs.table.card_mut(HUNTER).unwrap().zone = Some(fixtures::BASE);
        theirs.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        theirs.blob.set_contested(fixtures::BF1, Some(1));
        theirs.resolve();
        theirs.scripts = theirs
            .scripts
            .clone()
            .with_script(GROUNDS, &WIRED_WITHOUT_THE_EXCESS_FOR_THE_TEST);
        let mut ctx = theirs.ctx();
        conquer(&mut ctx, 1);
        assert_eq!(grounds_items(&ctx).len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        fixtures::choose(&mut ctx, 1, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(birds_of(&ctx, 1).len(), 1);
        assert!(birds_of(&ctx, 0).is_empty());
        drop(ctx);
        let mut elsewhere = wired();
        elsewhere.table.card_mut(HUNTER).unwrap().zone = Some(fixtures::BF2);
        elsewhere
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        elsewhere.blob.set_holder(fixtures::BF2, None);
        elsewhere.blob.set_contested(fixtures::BF1, None);
        elsewhere.blob.set_contested(fixtures::BF2, Some(0));
        elsewhere.resolve();
        elsewhere.scripts = elsewhere
            .scripts
            .clone()
            .with_script(GROUNDS, &WIRED_WITHOUT_THE_EXCESS_FOR_THE_TEST);
        let mut ctx = elsewhere.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF2),
            Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(grounds_items(&ctx).is_empty());
        assert!(birds_of(&ctx, 0).is_empty());
    }

    #[test]
    fn three_excess_damage_after_an_attack_plays_a_bird_when_the_trigger_resolves() {
        let mut fixture = against(3);
        let mut ctx = fixture.ctx();
        fight(&mut ctx);
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(3)
        );
        assert_eq!(grounds_items(&ctx).len(), 1);
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(birds_of(&ctx, 0).len(), 1);
    }
}
