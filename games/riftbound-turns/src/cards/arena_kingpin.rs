use super::prelude::{
    a_unit, activated, card_target, done, exhausting_self, might_this_turn, named, unit,
    with_statics,
};
use super::{Card, Cost, Flow, Item, Stage, Static, Timing};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 3;

pub fn enters_ready(_: &Ctx, _: u32) -> bool {
    true
}

fn hype(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    might_this_turn(ctx, item, unit, BONUS, None);
    ctx.narrate(format!("{{card {unit}}} gets +{BONUS} Might this turn"));
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Arena Kingpin",
        &[],
        &[named(
            exhausting_self(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[a_unit("a unit to give +3 Might this turn")],
                hype,
            )),
            "give a unit +3 Might this turn",
        )],
    ),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{this_turn, UNIT};
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const KINGPIN: u32 = 90;

    fn kingpin(zone: u16, exhausted: bool) -> CardInfo {
        CardInfo {
            energy: Some(5),
            domain: vec!["Fury".into()],
            exhausted,
            ..fixtures::unit(KINGPIN, zone, 0, "Arena Kingpin", 3)
        }
    }

    fn his_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == KINGPIN)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn pit(zone: u16, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(kingpin(zone, exhausted));
        if zone == fixtures::HAND {
            fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
            for rune in [46, 47] {
                fixture
                    .table
                    .cards
                    .push(fixtures::rune(rune, 0, "Fury", false));
            }
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(KINGPIN).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_with_one_free_exhaust_ability_and_the_enters_ready_seam() {
        assert!(std::ptr::eq(script_of("Arena Kingpin").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!(ability.label, Some("give a unit +3 Might this turn"));
        assert_eq!(BONUS, 3);
        let mut fixture = pit(fixtures::HAND, false);
        let ctx = fixture.ctx();
        assert!(enters_ready(&ctx, KINGPIN), "unconditional");
        assert!(enters_ready(&ctx, fixtures::THEIR_UNIT));
    }

    #[test]
    fn the_hype_exhausts_him_for_nothing_else_and_gives_the_chosen_unit_three_might_for_the_turn() {
        let mut fixture = pit(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            his_offers(&ctx),
            [(
                format!("{{card {KINGPIN}}}: give a unit +3 Might this turn (exhaust)"),
                true
            )]
        );
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, KINGPIN, 0).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        let mut expected = vec![
            format!("{{card {}}}", fixtures::VI),
            format!("{{card {}}}", fixtures::SPRITE),
            format!("{{card {}}}", fixtures::THEIR_UNIT),
            format!("{{card {KINGPIN}}}"),
            "cancel".to_string(),
        ];
        expected.sort();
        assert_eq!(offered, expected, "any unit, himself and enemies included");
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(
            ctx.card(KINGPIN).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "no rune is touched");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == KINGPIN
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "nothing until it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +3 Might this turn",
            fixtures::VI
        )));
        assert_eq!(
            activate::activate(&mut ctx, 0, KINGPIN, 0),
            Err(Refusal::Exhausted),
            "one hype per ready"
        );
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_exhausted_kingpin_or_an_opponent_cannot_use_it_and_a_unit_that_left_gets_nothing() {
        let mut spent = pit(fixtures::BASE, true);
        let mut ctx = spent.ctx();
        assert!(his_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, KINGPIN, 0),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, KINGPIN, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
        drop(ctx);

        let mut fixture = pit(fixtures::BASE, false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, KINGPIN, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        ctx.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::TRASH);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.blob.log.iter().any(|line| line.contains("gets +3")));
    }

    #[test]
    fn played_from_hand_he_lands_in_the_base_for_five() {
        let mut fixture = pit(fixtures::HAND, false);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 6);
        fixtures::play_from_hand(&mut ctx, 0, KINGPIN).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "5E");
        assert_eq!(ctx.location(KINGPIN), Some(Location::Base(0)));
        assert!(
            !ctx.card(KINGPIN).unwrap().exhausted,
            "369.3 · I enter ready"
        );
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Hand, .. } if *card == KINGPIN
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_from_hand_he_enters_ready_and_can_hype_at_once() {
        let mut fixture = pit(fixtures::HAND, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, KINGPIN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(KINGPIN));
        assert!(
            !ctx.card(KINGPIN).unwrap().exhausted,
            "369.3 · I enter ready replaces the exhausted entry"
        );
        assert!(activate::activate(&mut ctx, 0, KINGPIN, 0).is_ok());
    }
}
