use super::prelude::{
    a_card, activated, card_target, done, exhausting_self, gear, named, with_statics,
};
use super::{Card, Cost, Filter, Flow, Item, Stage, Static, Timing};
use crate::engine::ctx::Ctx;

pub const ANOTHER_GEAR: Filter = Filter::And(&[Filter::Gear, Filter::NotSelf]);

fn infuse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(gear) = card_target(ctx, item, 0) else {
        return done();
    };
    if ctx.empower_by(gear, item.controller) {
        ctx.narrate(format!("{{card {gear}}} is empowered"));
    } else {
        ctx.narrate(format!("{{card {gear}}} was Empowered already"));
    }
    done()
}

pub static CARD: Card = with_statics(
    gear(
        "Hextech Formula",
        &[],
        &[named(
            exhausting_self(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[a_card(ANOTHER_GEAR, "another gear to empower")],
                infuse,
            )),
            "empower another gear",
        )],
    ),
    &[Static::EntersExhausted],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, play as play_engine, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const FORMULA: u32 = 90;
    const MY_GEAR: u32 = 91;
    const THEIR_GEAR: u32 = 92;

    fn formula(zone: u16, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            exhausted,
            ..fixtures::gear(FORMULA, zone, 0, "Hextech Formula", 2)
        }
    }

    fn lab(zone: u16, exhausted: bool, with_gear: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(formula(zone, exhausted));
        if with_gear {
            fixture
                .table
                .cards
                .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
            fixture
                .table
                .cards
                .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Relic", 1));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FORMULA).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_enters_exhausted_and_exhausts_to_empower_another_gear() {
        assert!(std::ptr::eq(script_of("Hextech Formula").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.has_static(Static::EntersExhausted));
        assert_eq!(CARD.abilities.len(), 1);
        let infuse = &CARD.abilities[0];
        assert_eq!(infuse.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(infuse.cost, Some(Cost::FREE));
        assert_eq!(infuse.self_cost, SelfCost::Exhaust);
        assert!(infuse.usable.is_none());
        assert_eq!(infuse.targets.len(), 1);
        assert_eq!(infuse.targets[0].filter, ANOTHER_GEAR);
        assert_eq!((infuse.targets[0].min, infuse.targets[0].max), (1, 1));
    }

    #[test]
    fn played_from_hand_it_enters_exhausted_and_cannot_be_used_this_turn() {
        let mut fixture = lab(fixtures::HAND, false, true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FORMULA).unwrap();
        assert_eq!(ctx.card(FORMULA).unwrap().zone, Some(fixtures::BASE));
        assert!(
            ctx.card(FORMULA).unwrap().exhausted,
            "369.3 · the entry itself is replaced"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FORMULA}}} enters exhausted")));
        assert_eq!(
            activate::activate(&mut ctx, 0, FORMULA, 0),
            Err(Refusal::Exhausted)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_formula_offers_every_other_gear_and_the_pick_becomes_empowered() {
        let mut fixture = lab(fixtures::BASE, false, true);
        let mut ctx = fixture.ctx();
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == FORMULA)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {FORMULA}}}: empower another gear (exhaust)")
        );
        activate::activate(&mut ctx, 0, FORMULA, 0).unwrap();
        assert!(
            !ctx.card(FORMULA).unwrap().exhausted,
            "the exhaust is paid after the target is chosen"
        );
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        let mut expected = [
            format!("{{card {MY_GEAR}}}"),
            format!("{{card {THEIR_GEAR}}}"),
            "cancel".to_string(),
        ];
        expected.sort();
        assert_eq!(
            offered, expected,
            "another gear, yours or theirs, never itself"
        );
        let item = ctx.blob.queue[0].item.id;
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[FORMULA]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit is not a gear"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_GEAR}}}")).unwrap();
        assert!(ctx.card(FORMULA).unwrap().exhausted);
        assert!(!ctx.is_empowered(THEIR_GEAR), "not until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_empowered(THEIR_GEAR));
        assert!(ctx.events.contains(&Event::Empowered {
            card: THEIR_GEAR,
            by: 0
        }));
        assert!(!ctx.is_empowered(FORMULA));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_no_other_gear_the_activation_is_refused_and_so_is_the_other_seat() {
        let mut fixture = lab(fixtures::BASE, false, false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, FORMULA, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets))
        );
        assert!(activate::offers(&ctx, 0)
            .iter()
            .all(|offer| offer.source != FORMULA));
        drop(ctx);
        let mut fixture = lab(fixtures::BASE, false, true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, FORMULA, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
    }
}
