use super::frisky_hunter::play_birds;
use super::prelude::{
    a_play_location, activated, at_battlefield, done, exhausting_self, named, unit, usable_if,
    zone_target, Location,
};
use super::{Card, Cost, Flow, Item, Source, Stage, TargetSpec, Timing};
use crate::engine::ctx::Ctx;

pub const BIRDS: usize = 2;
pub const FLOCK: u8 = 0;
pub const WHERE: [TargetSpec; BIRDS] = [
    a_play_location("where the first Bird is played"),
    a_play_location("where the second Bird is played"),
];

pub fn perched(ctx: &Ctx, source: Source) -> bool {
    at_battlefield(ctx, source.card)
}

fn flock(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    for index in 0..BIRDS {
        let at = zone_target(item, index)
            .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            .unwrap_or(Location::Base(seat));
        play_birds(ctx, seat, at, 1);
    }
    done()
}

pub static CARD: Card = unit(
    "Ultrasoft Poro",
    &[],
    &[named(
        usable_if(
            exhausting_self(activated(Timing::Sorcery, Cost::FREE, &WHERE, flock)),
            perched,
        ),
        "play two Birds",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::frisky_hunter::tests::birds_of;
    use crate::cards::frisky_hunter::{is_bird, BIRD_MIGHT};
    use crate::cards::{script_of, SelfCost, TargetKind, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const PORO: u32 = 90;

    fn poro(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        let mut card = fixtures::unit(PORO, zone, seat, "Ultrasoft Poro", 5);
        card.domain = vec!["Order".into()];
        card.energy = Some(5);
        card.exhausted = exhausted;
        card
    }

    fn tundra(zone: u16, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(poro(zone, 0, exhausted));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PORO).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn poro_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == PORO)
            .map(|offer| (offer.label, offer.enabled))
            .collect()
    }

    #[test]
    fn the_script_is_one_free_exhaust_activation_gated_on_a_battlefield_choosing_two_locations() {
        assert!(std::ptr::eq(script_of("Ultrasoft Poro").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[usize::from(FLOCK)];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(
            ability.usable.is_some(),
            "377.2.b · only while at a battlefield"
        );
        assert_eq!(ability.label, Some("play two Birds"));
        assert_eq!(ability.targets.len(), BIRDS);
        for spec in ability.targets {
            assert_eq!(spec.kind, TargetKind::Zone);
            assert_eq!((spec.min, spec.max), (1, 1));
        }
        assert_eq!(BIRDS, 2);
    }

    #[test]
    fn at_a_battlefield_the_exhaust_plays_two_exhausted_birds_at_the_chosen_locations() {
        let mut fixture = tundra(fixtures::BF1, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            poro_offers(&ctx),
            [(format!("{{card {PORO}}}: play two Birds (exhaust)"), true)]
        );
        activate::activate(&mut ctx, 0, PORO, FLOCK).unwrap();
        for spec in 0..BIRDS as u8 {
            assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec }));
            assert_eq!(
                fixtures::labels(&ctx),
                ["{zone 8}", "{zone 9}", "cancel"],
                "the base and the held battlefield"
            );
            let zone = if spec == 0 { "{zone 9}" } else { "{zone 8}" };
            fixtures::choose(&mut ctx, 0, zone).unwrap();
        }
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.card(PORO).unwrap().exhausted, "the exhaust is the cost");
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "and nothing else is");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index } if source == PORO && index == FLOCK
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Zone(fixtures::BASE)
            ]
        );
        assert!(
            birds_of(&ctx, 0).is_empty(),
            "the Birds wait for resolution"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let birds = birds_of(&ctx, 0);
        assert_eq!(birds, [next, next + 1]);
        assert_eq!(
            ctx.location(birds[0]),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(birds[1]), Some(Location::Base(0)));
        for bird in birds {
            assert!(is_bird(&ctx, bird));
            assert!(ctx.is_token(bird));
            assert_eq!(ctx.current_might(bird), i32::from(BIRD_MIGHT));
            assert_eq!(ctx.deflect_of(bird), 1);
            assert_eq!(ctx.controller(bird), 0);
            assert!(
                ctx.card(bird).unwrap().exhausted,
                "185.2.d · they enter exhausted"
            );
        }
        assert!(birds_of(&ctx, 1).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, PORO, FLOCK),
            Err(Refusal::Exhausted),
            "spent for the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn in_the_base_the_ability_is_neither_offered_nor_usable() {
        let mut fixture = tundra(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        assert!(!perched(
            &ctx,
            Source {
                card: PORO,
                ability: FLOCK
            }
        ));
        assert!(poro_offers(&ctx).is_empty(), "not offered in the base");
        assert_eq!(
            activate::activate(&mut ctx, 0, PORO, FLOCK),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "377.2.b · use this ability only while I'm at a battlefield"
        );
        assert!(!ctx.card(PORO).unwrap().exhausted);
        assert!(birds_of(&ctx, 0).is_empty());
    }

    #[test]
    fn a_spent_poro_the_other_seat_and_a_cancelled_activation_play_nothing() {
        let mut fixture = tundra(fixtures::BF1, true);
        let mut ctx = fixture.ctx();
        assert!(poro_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, PORO, FLOCK),
            Err(Refusal::Exhausted)
        );
        drop(ctx);
        let mut fixture = tundra(fixtures::BF1, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, PORO, FLOCK),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, PORO, FLOCK).unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(!ctx.card(PORO).unwrap().exhausted);
        assert!(birds_of(&ctx, 0).is_empty());
    }
}
