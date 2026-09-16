use super::daisy::{is_bird, is_cat, is_dog};
use super::poro_herder::is_poro;
use super::prelude::{
    asking, battlefield, done, exhausting_self, legend, optional, triggered, with_candidates,
    with_statics,
};
use super::{
    base_name, Ability, Card, Flow, Grant, Item, Scope, Stage, Static, Trigger, Who, TOKEN_BRUSH,
};
use crate::engine::ctx::{Ctx, Token};
use crate::state::TargetRef;

const STAGE_SWAP: u8 = 1;
pub const SWAP_QUESTION: &str = "swap this Brush back for the battlefield it replaced";

pub const BRUSH: &str = TOKEN_BRUSH;
pub const BRUSH_BONUS: i16 = 1;
pub const IVERN: &str = "Ivern";
pub const BRUSH_DWELLER_TAGS: [&str; 5] = ["Bird", "Cat", "Dog", "Poro", IVERN];

pub fn is_ivern(ctx: &Ctx, card: u32) -> bool {
    ctx.is_unit(card)
        && ctx.card(card).is_some_and(|held| {
            base_name(&held.name)
                .split(" - ")
                .next()
                .is_some_and(|name| name == IVERN)
        })
}

pub fn is_brush_dweller(ctx: &Ctx, unit: u32) -> bool {
    is_ivern(ctx, unit)
        || is_poro(ctx, unit)
        || is_bird(ctx, unit)
        || is_cat(ctx, unit)
        || is_dog(ctx, unit)
}

fn a_brush_dweller(ctx: &Ctx, _: u32, unit: u32) -> bool {
    is_brush_dweller(ctx, unit)
}

pub fn is_brush(ctx: &Ctx, card: u32) -> bool {
    ctx.is_battlefield_card(card) && ctx.card(card).is_some_and(|held| held.name == BRUSH)
}

fn regrow(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(TargetRef::Zone(zone)) = item.subject else {
        return done();
    };
    if ctx.replace_battlefield(zone, Token::Brush).is_none() {
        ctx.narrate(format!("{{zone {zone}}} stands · nothing there to replace"));
    }
    done()
}

fn the_brush_itself(_: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    vec![TargetRef::Card(item.kind.source())]
}

fn swap_back(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let brush = item.kind.source();
    match stage.0 {
        STAGE_SWAP => {
            if ctx.picks().contains(&brush) {
                if let Some(zone) = ctx.card(brush).and_then(|held| held.zone) {
                    ctx.swap_back(zone);
                }
            } else {
                ctx.narrate(format!("{{card {brush}}} stays"));
            }
            done()
        }
        _ => Flow::Ask(ctx.ask_resume(item, STAGE_SWAP, 0, 1)),
    }
}

const fn may_swap_back(trigger: Trigger) -> Ability {
    asking(
        with_candidates(
            optional(triggered(trigger, &[], swap_back)),
            the_brush_itself,
        ),
        SWAP_QUESTION,
    )
}

pub static CARD: Card = legend(
    "Ivern - Green Father",
    &[],
    &[
        optional(exhausting_self(triggered(
            Trigger::Conquer(Who::You),
            &[],
            regrow,
        ))),
        optional(exhausting_self(triggered(
            Trigger::Hold(Who::You),
            &[],
            regrow,
        ))),
    ],
);

pub static BRUSH_TOKEN: Card = with_statics(
    battlefield(
        BRUSH,
        &[],
        &[
            may_swap_back(Trigger::Conquer(Who::You)),
            may_swap_back(Trigger::Hold(Who::You)),
        ],
    ),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: a_brush_dweller,
        grants: &[Grant::Might(BRUSH_BONUS)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, prompts, settle, statics};
    use crate::state::{ItemKind, PromptWhy};

    const IVERN_LEGEND: u32 = fixtures::LEGEND_CARD;
    const PORO: u32 = 90;
    const IVERN_UNIT: u32 = 91;
    const PIGEON: u32 = 92;
    const PUP: u32 = 93;

    fn grove() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(IVERN_LEGEND).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(IVERN_LEGEND).unwrap(),
            &CARD
        ));
        fixture
    }

    fn brush_at_bf1() -> Fixture {
        brush_at_bf1_beside(grove())
    }

    fn thicket() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        brush_at_bf1_beside(fixture)
    }

    fn brush_at_bf1_beside(mut fixture: Fixture) -> Fixture {
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = BRUSH.into();
        fixture
            .table
            .cards
            .push(fixtures::unit(PORO, fixtures::BF1, 0, "Fluffy Poro", 2));
        fixture.table.cards.push(fixtures::unit(
            IVERN_UNIT,
            fixtures::BF1,
            1,
            "Ivern - Sapling",
            4,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(PIGEON, fixtures::BF1, 0, "Pigeon Flock", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(PUP, fixtures::BASE, 1, "Loyal Pup", 1));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &BRUSH_TOKEN);
        fixture
    }

    fn hold(ctx: &mut Ctx, seat: u8) {
        cleanup::score_holds(ctx, seat);
        settle(ctx).unwrap();
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_has_a_may_conquer_and_a_may_hold_trigger_that_exhaust_him() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Conquer(Who::You));
        assert_eq!(CARD.abilities[1].trigger, Trigger::Hold(Who::You));
        for ability in CARD.abilities {
            assert!(ability.optional);
            assert!(ability.cost.is_none());
            assert_eq!(ability.self_cost, SelfCost::Exhaust);
            assert!(
                ability.targets.is_empty(),
                "that battlefield is the subject"
            );
            assert!(ability.condition.is_none());
        }
        assert_eq!(BRUSH_BONUS, 1);
        assert_eq!(BRUSH_DWELLER_TAGS.len(), 5);
    }

    #[test]
    fn the_brush_face_is_a_battlefield_token_with_a_dweller_aura_and_a_may_swap_back_on_a_score() {
        assert!(
            std::ptr::eq(script_of(BRUSH).unwrap(), &BRUSH_TOKEN),
            "the token resolves by name"
        );
        assert_eq!(Token::Brush.face().name, BRUSH);
        assert_eq!(BRUSH_TOKEN.name, BRUSH);
        assert!(BRUSH_TOKEN.keywords.is_empty());
        assert!(BRUSH_TOKEN.has_aura());
        assert!(matches!(
            BRUSH_TOKEN.statics,
            [Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Might(1)],
                ..
            }]
        ));
        assert_eq!(BRUSH_TOKEN.abilities.len(), 2);
        assert_eq!(BRUSH_TOKEN.abilities[0].trigger, Trigger::Conquer(Who::You));
        assert_eq!(BRUSH_TOKEN.abilities[1].trigger, Trigger::Hold(Who::You));
        assert!(BRUSH_TOKEN.abilities.iter().all(|ability| ability.optional));
    }

    #[test]
    fn a_dweller_is_an_ivern_a_poro_a_bird_a_cat_or_a_dog_by_printed_name() {
        let mut fixture = brush_at_bf1();
        let ctx = fixture.ctx();
        assert!(is_ivern(&ctx, IVERN_UNIT));
        assert!(!is_ivern(&ctx, IVERN_LEGEND), "the legend is no unit");
        assert!(!is_ivern(&ctx, fixtures::VI));
        assert!(is_brush_dweller(&ctx, IVERN_UNIT));
        assert!(is_brush_dweller(&ctx, PORO));
        assert!(!is_brush_dweller(&ctx, fixtures::VI));
        assert!(
            !is_brush_dweller(&ctx, PIGEON),
            "a Bird under a name daisy::BIRDS does not know is invisible until CardInfo carries tags"
        );
        assert!(is_brush_dweller(&ctx, PUP), "a Dog by its printed name");
        assert!(is_brush(&ctx, fixtures::GROUNDS));
        assert!(!is_brush(&ctx, fixtures::ROCKFALL));
        assert!(!is_brush(&ctx, fixtures::VI));
    }

    #[test]
    fn in_brush_the_dwellers_of_either_seat_read_one_more_might_and_the_others_read_nothing() {
        let mut fixture = brush_at_bf1();
        let mut ctx = fixture.ctx();
        assert!(matches!(
            statics::grants_on(&ctx, PORO).as_slice(),
            [Grant::Might(1)]
        ));
        assert_eq!(ctx.current_might(PORO), 3);
        assert_eq!(
            ctx.current_might(IVERN_UNIT),
            5,
            "the opponent's Ivern is in Brush too"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3, "Vi is no dweller");
        assert_eq!(ctx.current_might(PIGEON), 1);
        ctx.recall(PORO, false);
        assert_eq!(ctx.current_might(PORO), 2, "in the base there is no Brush");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn holding_asks_to_exhaust_him_and_yes_replaces_the_battlefield_when_the_trigger_resolves() {
        let mut fixture = grove();
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, .. } if *zone == fixtures::BF1
        )));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 }),
            "392.2 · the may is the cost confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "exhaust {{card {IVERN_LEGEND}}} for the {{card {IVERN_LEGEND}}} trigger · {{zone {}}}?",
                fixtures::BF1
            )
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(IVERN_LEGEND).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == IVERN_LEGEND
        ));
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let brush = ctx
            .table
            .cards
            .iter()
            .find(|held| held.name == BRUSH && held.zone == Some(fixtures::BF1))
            .map(|held| held.id)
            .expect("a Brush token stands at the battlefield");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} is replaced with {{card {brush}}} at {{zone {}}}",
            fixtures::GROUNDS,
            fixtures::BF1
        )));
        assert!(is_brush(&ctx, brush));
        assert!(ctx.is_token(brush));
        assert_eq!(ctx.replaced_by(brush), Some(fixtures::GROUNDS));
        assert!(
            ctx.in_banishment(fixtures::GROUNDS),
            "438.5 · the replaced card waits in Banishment"
        );
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Banished { .. })),
            "438.4 · replacing is not banishing"
        );
        assert!(ctx.zones.is_battlefield(fixtures::BF1));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_fires_the_first_trigger_and_declining_or_an_exhausted_ivern_does_nothing() {
        let mut fixture = grove();
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(IVERN_LEGEND).unwrap().exhausted);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {IVERN_LEGEND}}} trigger is removed · its cost is declined"
        )));
        drop(ctx);
        let mut spent = grove();
        spent.table.card_mut(IVERN_LEGEND).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        hold(&mut ctx, 0);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {IVERN_LEGEND}}} trigger is removed · its source is exhausted"
        )));
        drop(ctx);
        let mut theirs = grove();
        let mut ctx = theirs.ctx();
        hold(&mut ctx, 1);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "seat 1's hold is not his");
    }

    #[test]
    fn a_score_in_brush_asks_to_swap_back_and_a_brush_that_replaced_nothing_stays() {
        let mut fixture = thicket();
        fixture.table.cards.retain(|card| card.id != IVERN_UNIT);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &BRUSH_TOKEN);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.card(IVERN_LEGEND).unwrap().name,
            "Lillia - Bashful Bloom"
        );
        hold(&mut ctx, 0);
        assert!(ctx.blob.prompt.is_none(), "a free may asks as it resolves");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == fixtures::GROUNDS
        ));
        resolve_top(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!(
                "{{card {}}}: choose {SWAP_QUESTION} (0 of 1)",
                fixtures::GROUNDS
            )
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::GROUNDS),
                "skip".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::GROUNDS)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} has nothing to swap back to",
            fixtures::GROUNDS
        )));
        assert!(
            ctx.on_board(fixtures::GROUNDS),
            "438.7.c · a Brush that replaced nothing never swaps back"
        );
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut stays = thicket();
        stays.scripts = stays
            .scripts
            .clone()
            .with_script(fixtures::GROUNDS, &BRUSH_TOKEN);
        let mut ctx = stays.ctx();
        hold(&mut ctx, 0);
        resolve_top(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} stays", fixtures::GROUNDS)));
    }

    #[test]
    fn a_hold_replaces_the_battlefield_with_brush_and_a_later_score_there_may_swap_it_back() {
        let mut fixture = grove();
        fixture
            .table
            .cards
            .push(fixtures::unit(PORO, fixtures::BF1, 0, "Fluffy Poro", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        resolve_top(&mut ctx);
        let brush = ctx
            .table
            .cards
            .iter()
            .find(|held| held.name == BRUSH && held.zone == Some(fixtures::BF1))
            .map(|held| held.id)
            .expect("a Brush token stands at the battlefield");
        assert!(ctx.is_token(brush));
        assert!(
            !ctx.on_board(fixtures::GROUNDS),
            "438.1 · the replaced card waits in Banishment"
        );
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "438.1 · statuses are inherited"
        );
        assert_eq!(ctx.current_might(PORO), 3);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        ctx.exhaust(IVERN_LEGEND);
        ctx.blob.clear_scored();
        hold(&mut ctx, 0);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 }),
            "203.3 · the exhausted Ivern's trigger still exists and is ordered before it is removed"
        );
        fixtures::choose(
            &mut ctx,
            0,
            &format!("{{card {brush}}} trigger 2 · {{zone {}}}", fixtures::BF1),
        )
        .unwrap();
        assert!(ctx.blob.log.contains(&format!(
            "{{card {IVERN_LEGEND}}} trigger is removed · its source is exhausted"
        )));
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_top(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {brush}}}")).unwrap();
        assert!(ctx.on_board(fixtures::GROUNDS), "438.7 · swapped back");
        assert!(ctx.card(brush).is_none());
        assert_eq!(ctx.current_might(PORO), 2);
    }
}
