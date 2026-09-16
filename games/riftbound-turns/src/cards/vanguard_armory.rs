use super::faithful_manufactor::play_recruits;
use super::prelude::{
    a_play_location, activated, done, exhausting_self, gear, named, zone_target, Location,
};
use super::{Card, Cost, Flow, Item, Stage, TargetSpec, Timing};
use crate::engine::ctx::Ctx;

pub const RECRUITS: usize = 3;
pub const MUSTER: u8 = 0;
pub const WHERE: [TargetSpec; RECRUITS] = [
    a_play_location("where the first Recruit is played"),
    a_play_location("where the second Recruit is played"),
    a_play_location("where the third Recruit is played"),
];

fn muster(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    for index in 0..RECRUITS {
        let at = zone_target(item, index)
            .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            .unwrap_or(Location::Base(seat));
        play_recruits(ctx, seat, at, 1);
    }
    done()
}

pub static CARD: Card = gear(
    "Vanguard Armory",
    &[],
    &[named(
        exhausting_self(activated(Timing::Sorcery, Cost::FREE, &WHERE, muster)),
        "play three Recruits",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::recruits_of;
    use crate::cards::faithful_manufactor::RECRUIT;
    use crate::cards::{script_of, SelfCost, TargetKind, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ARMORY: u32 = 90;

    fn armory(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Order".into()],
            exhausted,
            ..fixtures::gear(ARMORY, zone, seat, "Vanguard Armory", 7)
        }
    }

    fn barracks(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(armory(fixtures::BASE, 0, exhausted));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ARMORY).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_one_free_exhaust_activation_choosing_three_play_locations() {
        assert!(std::ptr::eq(script_of("Vanguard Armory").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[usize::from(MUSTER)];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.label, Some("play three Recruits"));
        assert_eq!(ability.targets.len(), RECRUITS);
        for spec in ability.targets {
            assert_eq!(spec.kind, TargetKind::Zone);
            assert_eq!((spec.min, spec.max), (1, 1));
        }
        assert_eq!(RECRUITS, 3);
    }

    #[test]
    fn the_exhaust_plays_three_exhausted_recruits_at_the_three_chosen_locations() {
        let mut fixture = barracks(false);
        let mut ctx = fixture.ctx();
        let offers: Vec<(String, bool)> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == ARMORY)
            .map(|offer| (offer.label, offer.enabled))
            .collect();
        assert_eq!(
            offers,
            [(
                format!("{{card {ARMORY}}}: play three Recruits (exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, ARMORY, MUSTER).unwrap();
        for spec in 0..RECRUITS as u8 {
            assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec }));
            assert_eq!(
                fixtures::labels(&ctx),
                ["{zone 8}", "{zone 9}", "cancel"],
                "the base and the held battlefield, never the other seat's"
            );
            let zone = if spec == 1 { "{zone 9}" } else { "{zone 8}" };
            fixtures::choose(&mut ctx, 0, zone).unwrap();
        }
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.card(ARMORY).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "and nothing else is");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Zone(fixtures::BASE),
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Zone(fixtures::BASE)
            ]
        );
        assert!(
            recruits_of(&ctx, 0).is_empty(),
            "the Recruits wait for resolution"
        );
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits, [next, next + 1, next + 2]);
        assert_eq!(ctx.location(recruits[0]), Some(Location::Base(0)));
        assert_eq!(
            ctx.location(recruits[1]),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(recruits[2]), Some(Location::Base(0)));
        for recruit in recruits {
            assert!(ctx.is_token(recruit));
            assert!(ctx.is_unit(recruit));
            assert_eq!(ctx.card(recruit).unwrap().name, RECRUIT);
            assert_eq!(ctx.current_might(recruit), 1);
            assert_eq!(ctx.controller(recruit), 0);
            assert!(
                ctx.card(recruit).unwrap().exhausted,
                "185.2.d · they enter exhausted"
            );
        }
        assert!(recruits_of(&ctx, 1).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, ARMORY, MUSTER),
            Err(Refusal::Exhausted),
            "spent for the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn all_three_may_share_one_location_and_a_cancelled_activation_spends_nothing() {
        let mut fixture = barracks(false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, ARMORY, MUSTER).unwrap();
        for _ in 0..RECRUITS {
            fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        }
        resolve_chain(&mut ctx);
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits.len(), RECRUITS);
        assert!(recruits
            .iter()
            .all(|recruit| ctx.location(*recruit) == Some(Location::Battlefield(fixtures::BF1))));
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)).len(),
            RECRUITS
        );
        drop(ctx);
        let mut fixture = barracks(false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, ARMORY, MUSTER).unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(!ctx.card(ARMORY).unwrap().exhausted);
        assert!(ctx.effects.is_empty());
        assert!(recruits_of(&ctx, 0).is_empty());
    }

    #[test]
    fn a_spent_armory_the_other_seat_and_a_closed_chain_are_refused() {
        let mut fixture = barracks(true);
        let mut ctx = fixture.ctx();
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == ARMORY));
        assert_eq!(
            activate::activate(&mut ctx, 0, ARMORY, MUSTER),
            Err(Refusal::Exhausted)
        );
        drop(ctx);
        let mut fixture = barracks(false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, ARMORY, MUSTER),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            activate::activate(&mut ctx, 0, ARMORY, MUSTER),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "151.2 · gear abilities are played in an Open State"
        );
        assert!(!ctx.card(ARMORY).unwrap().exhausted);
        assert!(recruits_of(&ctx, 0).is_empty());
    }

    #[test]
    #[ignore = "engine gap · Token::Recruit: engine/ctx.rs has no Recruit face, so faithful_manufactor::spawn_recruit builds it by hand; with the token, play_recruits spawns Token::Recruit and cards/mod.rs knows the name"]
    fn the_engine_knows_the_recruit_as_a_token_name() {
        assert!(crate::cards::is_token_name(RECRUIT));
    }
}
