use super::prelude::{
    a_card, activated, card_target, charm_destination, done, exhausting_self, legend, move_unit,
    named, target,
};
use super::{Card, Cost, Filter, Flow, Item, Stage, TargetKind, TargetSpec, Timing};
use crate::engine::ctx::Ctx;

pub const PRICE: Cost = Cost {
    energy: 2,
    power: &[],
};

pub const FRIENDLY_UNIT_AT_HOME_OR_BOUND_HOME: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::Movable,
    Filter::Or(&[Filter::InBase, Filter::MovableToBase]),
]);

pub const ACROSS_THE_BASE_LINE: TargetSpec = target(
    Filter::ToOrFromBaseOf(0),
    1,
    1,
    TargetKind::Zone,
    "where it goes",
);

const UNIT: usize = 0;
const DESTINATION: usize = 1;

fn sweeping_blade(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    match charm_destination(ctx, item, unit, DESTINATION) {
        Some(to) => {
            move_unit(ctx, item, unit, to);
        }
        None => ctx.narrate(format!("{{card {unit}}} can't make the move any more")),
    }
    done()
}

pub static CARD: Card = legend(
    "Yasuo - Unforgiven",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            PRICE,
            &[
                a_card(
                    FRIENDLY_UNIT_AT_HOME_OR_BOUND_HOME,
                    "a friendly unit to move to or from its base",
                ),
                ACROSS_THE_BASE_LINE,
            ],
            sweeping_blade,
        )),
        "move a friendly unit to or from its base",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::{Event, Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, play, priority};
    use crate::state::{ItemKind, ItemStatus, PromptWhy, TargetRef};
    use crate::Refusal;

    const YASUO: u32 = fixtures::LEGEND_CARD;
    const SCOUT: u32 = 90;

    fn wind(scout_at: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(YASUO).unwrap().name = CARD.name.into();
        let mut scout = fixtures::unit(SCOUT, scout_at, 0, "Scout", 2);
        scout.exhausted = true;
        fixture.table.cards.push(scout);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn moved_to(ctx: &Ctx, unit: u32, to: Location) -> bool {
        ctx.events.iter().any(|event| {
            matches!(
                event,
                Event::Moved { card, to: at, cause: MoveCause::Effect, .. } if *card == unit && *at == to
            )
        })
    }

    #[test]
    fn the_legend_has_one_sorcery_activation_for_two_energy_and_his_exhaust() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(PRICE));
        assert_eq!(PRICE.energy, 2);
        assert!(PRICE.power.is_empty());
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(
            ability.label,
            Some("move a friendly unit to or from its base")
        );
        assert!(
            ability.candidates.is_none(),
            "the destination is fixed as the ability goes on the chain"
        );
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(
            ability.targets[0].filter,
            FRIENDLY_UNIT_AT_HOME_OR_BOUND_HOME
        );
        assert_eq!(ability.targets[1], ACROSS_THE_BASE_LINE);
        assert_eq!(ability.targets[1].kind, TargetKind::Zone);
        assert_eq!((ability.targets[1].min, ability.targets[1].max), (1, 1));
        let mut fixture = wind(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(YASUO).unwrap(), &CARD));
        assert_eq!(cost::of_activation(&ctx, YASUO, 0).label(), "2 energy");
        assert_eq!(
            activate::offers(&ctx, 0)
                .iter()
                .map(|offer| (offer.label.clone(), offer.enabled))
                .collect::<Vec<_>>(),
            [(
                format!("{{card {YASUO}}}: move a friendly unit to or from its base (2 energy, exhaust)"),
                true
            )]
        );
    }

    #[test]
    fn a_unit_at_a_battlefield_is_offered_its_base_alone_and_comes_home() {
        let mut fixture = wind(fixtures::BF1);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, YASUO, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                "cancel".to_string(),
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {SCOUT}}}")
            ],
            "Vi in the base and the Scout at a battlefield · the enemy's units are not his"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{zone {}}}", fixtures::BASE), "cancel".to_string()],
            "to base is the only way home · the other battlefield is not offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BASE)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.card(YASUO).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy, two runes");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == YASUO
        ));
        assert_eq!(
            ctx.location(SCOUT),
            Some(Location::Battlefield(fixtures::BF1)),
            "nothing until it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "nothing is asked as it resolves");
        assert_eq!(ctx.location(SCOUT), Some(Location::Base(0)));
        assert!(moved_to(&ctx, SCOUT, Location::Base(0)));
        assert!(
            ctx.card(SCOUT).unwrap().exhausted,
            "a move keeps the unit exhausted"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_in_the_base_is_asked_which_battlefield_it_goes_to_before_the_ability_is_on_the_chain()
    {
        let mut fixture = wind(fixtures::BASE);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, YASUO, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 1, spec: 1 }),
            "the destination is a targeting question · opponents respond knowing it"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "both battlefields in play · the base is where it stands"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Finalized);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(SCOUT), TargetRef::Zone(fixtures::BF1)]
        );
        assert_eq!(
            ctx.location(SCOUT),
            Some(Location::Base(0)),
            "nothing until it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(SCOUT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(moved_to(&ctx, SCOUT, Location::Battlefield(fixtures::BF1)));
        assert!(
            ctx.card(SCOUT).unwrap().exhausted,
            "an effect move does not exhaust or ready"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn moving_into_the_enemys_battlefield_opens_a_showdown_like_any_effect_move() {
        let mut fixture = wind(fixtures::BASE);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, YASUO, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.blob.contester(fixtures::BF2),
            Some(0),
            "seat 1 holds it with a Sprite · Vi contests"
        );
    }

    #[test]
    fn a_cancelled_activation_pays_nothing_and_moves_nothing() {
        let mut fixture = wind(fixtures::BF1);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, YASUO, 0).unwrap();
        let item = match ctx.blob.why {
            Some(PromptWhy::Target { item, .. }) => item,
            other => panic!("{other:?}"),
        };
        ctx.blob.close_prompt();
        play::cancel(&mut ctx, item);
        assert!(!ctx.card(YASUO).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.is_empty());
        assert_eq!(
            ctx.location(SCOUT),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }

    #[test]
    fn the_activation_is_refused_for_the_wrong_seat_an_exhausted_legend_short_runes_and_a_busy_chain(
    ) {
        let mut fixture = wind(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, YASUO, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, YASUO, 1),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert!(activate::offers(&ctx, 1).is_empty());
        drop(ctx);
        let mut spent = wind(fixtures::BF1);
        spent.table.card_mut(YASUO).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, YASUO, 0),
            Err(Refusal::Exhausted)
        );
        drop(ctx);
        let mut broke = wind(fixtures::BF1);
        broke.table.card_mut(41).unwrap().exhausted = true;
        broke.table.card_mut(42).unwrap().exhausted = true;
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, YASUO, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            })
        );
        let offers = activate::offers(&ctx, 0);
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].enabled, "greyed, so the seat sees the price");
        drop(ctx);
        let mut busy = wind(fixtures::BF1);
        let mut ctx = busy.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            activate::activate(&mut ctx, 0, YASUO, 0),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
        assert!(!ctx.card(YASUO).unwrap().exhausted);
    }
}
