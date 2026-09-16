use super::prelude::{
    card_target, done, might_this_turn, move_destinations, move_unit, play, spell, target,
    zone_target, Location,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::targets;

pub const BOOST: i16 = 2;
const BATTLEFIELD: usize = 0;
const UNIT: usize = 1;

pub const A_BATTLEFIELD_YOU_CONTROL: TargetSpec = target(
    Filter::And(&[Filter::AtBattlefield, Filter::Friendly]),
    1,
    1,
    TargetKind::Zone,
    "a battlefield you control",
);

pub const A_FRIENDLY_UNIT_ELSEWHERE: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::Friendly,
        Filter::Movable,
        Filter::DifferentLocationFrom(BATTLEFIELD as u8),
    ]),
    1,
    1,
    TargetKind::Card,
    "a unit you control at a different location",
);

fn held_battlefield(ctx: &Ctx, item: &Item) -> Option<u16> {
    let zone = zone_target(item, BATTLEFIELD)?;
    (ctx.zones.is_battlefield(zone) && targets::valid(ctx, item, BATTLEFIELD)).then_some(zone)
}

fn resonate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    if let Some(zone) = held_battlefield(ctx, item) {
        let to = Location::Battlefield(zone);
        if move_destinations(ctx, unit).contains(&to) {
            move_unit(ctx, item, unit, to);
        }
    }
    might_this_turn(ctx, item, unit, BOOST, None);
    ctx.narrate(format!("{{card {unit}}} gets +{BOOST} Might this turn"));
    done()
}

pub static CARD: Card = spell(
    "Resonating Strike",
    &[Keyword::Hidden, Keyword::Reaction],
    &[play(
        &[A_BATTLEFIELD_YOU_CONTROL, A_FRIENDLY_UNIT_ELSEWHERE],
        resonate,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const STRIKE: u32 = 90;
    const SECOND: u32 = 91;
    const CALM_RUNE: u32 = 46;

    fn strike() -> CardInfo {
        CardInfo {
            domain: vec!["Calm".into()],
            ..fixtures::spell(STRIKE, fixtures::HAND, 0, "Resonating Strike", 2, 1)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(strike());
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF1, 0, "Second", 2));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(STRIKE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn moved_by_effect(ctx: &Ctx, unit: u32, zone: u16) -> bool {
        ctx.events.iter().any(|event| {
            matches!(
                event,
                Event::Moved { card, to: Location::Battlefield(at), cause: MoveCause::Effect, .. }
                    if *card == unit && *at == zone
            )
        })
    }

    #[test]
    fn the_script_is_a_hidden_reaction_over_a_held_battlefield_then_a_friendly_unit_elsewhere() {
        assert!(std::ptr::eq(script_of("Resonating Strike").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0], A_BATTLEFIELD_YOU_CONTROL);
        assert_eq!(ability.targets[0].kind, TargetKind::Zone);
        assert_eq!(ability.targets[1], A_FRIENDLY_UNIT_ELSEWHERE);
        assert_eq!(ability.targets[1].kind, TargetKind::Card);
        assert_eq!(BOOST, 2);
    }

    #[test]
    fn a_unit_in_your_base_marches_to_your_battlefield_and_gets_two_this_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "cancel"],
            "only the battlefield you hold; the other seat holds the second and the third is open"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "Vi in the base; the unit already there and the enemy units are not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Card(fixtures::VI)
            ]
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(moved_by_effect(&ctx, fixtures::VI, fixtures::BF1));
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::VI,
            zone: fixtures::BF1,
            seat: 0,
            index: agni_plugin_sdk::decide::TOP
        }));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +2 Might this turn".to_string()));
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_from_facedown_the_hiding_battlefield_is_the_only_destination_and_the_unit_may_come_from_anywhere(
    ) {
        let mut fixture = armed();
        fixture.table.card_mut(STRIKE).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(STRIKE).hidden_at = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(STRIKE, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            STRIKE,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "cancel"],
            "the battlefield it was hidden at"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "a spec that can never be met at the hiding battlefield is lifted from the restriction"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        let paid: Vec<&Effect> = ctx
            .effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .collect();
        assert!(paid.is_empty(), "a hidden card reacts for free: {paid:?}");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_battlefield_you_lost_in_response_is_no_destination_but_the_boost_still_lands() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.blob.set_holder(fixtures::BF1, Some(1));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "the battlefield mistargets, so no move"
        );
        assert!(!moved_by_effect(&ctx, fixtures::VI, fixtures::BF1));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "356.3.e.5 · the boost is on the unit, which is still a legal choice"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_unheld_battlefield_a_unit_already_there_an_enemy_and_a_gone_unit_are_refused_or_missed() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        for wrong in [fixtures::BF2, fixtures::BF3, fixtures::BASE] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[u32::from(wrong)]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a battlefield you control"
            );
        }
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        for wrong in [
            SECOND,
            fixtures::THEIR_UNIT,
            fixtures::SPRITE,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is already there, an enemy, or not on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.bounce(fixtures::VI);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("Might this turn")));
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut landless = armed();
        landless.blob.set_holder(fixtures::BF1, None);
        let mut ctx = landless.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "holding no battlefield, there is nothing to choose"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::HAND));
    }
}
