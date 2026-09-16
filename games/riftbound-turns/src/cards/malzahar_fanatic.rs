use super::prelude::{
    a_card, activated, card_target, done, exhausting_self, friendly_gear, friendly_units, named,
    unit, usable_if, FRIENDLY_UNIT_OR_GEAR,
};
use super::{Card, Cost, Flow, Item, Source, Stage, Timing};
use crate::engine::ctx::{Cause, Ctx, Killed};

pub const ADDS: u8 = 2;

pub fn kill_candidates(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut cards = friendly_units(ctx, seat);
    cards.extend(friendly_gear(ctx, seat));
    cards.sort_unstable();
    cards
}

pub fn add_reaches_the_pay_stage(_: &Ctx, _: Source) -> bool {
    false
}

pub fn add_rainbow(ctx: &mut Ctx, seat: u8, count: u8) -> bool {
    ctx.narrate(format!(
        "{{seat {seat}}} would add {count} any power · the engine has no rune pool to add to"
    ));
    false
}

fn feed_the_void(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(card) = card_target(ctx, item, 0) else {
        return done();
    };
    if ctx.kill(card, Cause::Cost) == Killed::NotOnBoard {
        return done();
    }
    ctx.narrate(format!(
        "{{card {card}}} is killed for the {{card {}}} ability",
        item.kind.source()
    ));
    add_rainbow(ctx, item.controller, ADDS);
    done()
}

pub static CARD: Card = unit(
    "Malzahar - Fanatic",
    &[],
    &[named(
        usable_if(
            exhausting_self(activated(
                Timing::Action,
                Cost::FREE,
                &[a_card(
                    FRIENDLY_UNIT_OR_GEAR,
                    "a friendly unit or gear to kill",
                )],
                feed_the_void,
            )),
            add_reaches_the_pay_stage,
        ),
        "add 2 any power",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MALZAHAR: u32 = 90;
    const SCOUT: u32 = 91;
    const TRINKET: u32 = 92;

    fn malzahar() -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Mind".into()],
            ..fixtures::unit(MALZAHAR, fixtures::BASE, 0, "Malzahar - Fanatic", 3)
        }
    }

    fn cult() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(malzahar());
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn malzahar_has_one_action_ability_that_exhausts_him_and_kills_a_friendly_unit_or_gear() {
        assert!(std::ptr::eq(
            script_of("Malzahar - Fanatic").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let feed = &CARD.abilities[0];
        assert_eq!(feed.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(feed.self_cost, SelfCost::Exhaust);
        assert_eq!(feed.cost, Some(Cost::FREE));
        assert_eq!(feed.targets.len(), 1);
        assert_eq!(feed.targets[0].filter, FRIENDLY_UNIT_OR_GEAR);
        assert_eq!(feed.label, Some("add 2 any power"));
        assert!(feed.usable.is_some());
        assert_eq!(ADDS, 2);
    }

    #[test]
    fn the_candidates_are_his_controllers_units_and_gear_himself_included() {
        let mut fixture = cult();
        let ctx = fixture.ctx();
        assert_eq!(
            kill_candidates(&ctx, 0),
            [fixtures::VI, MALZAHAR, SCOUT, TRINKET]
        );
        assert_eq!(
            kill_candidates(&ctx, 1),
            [fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
    }

    #[test]
    fn until_the_pay_stage_takes_add_abilities_it_is_neither_offered_nor_usable() {
        let mut fixture = cult();
        let mut ctx = fixture.ctx();
        assert!(!add_reaches_the_pay_stage(
            &ctx,
            Source {
                card: MALZAHAR,
                ability: 0
            }
        ));
        assert!(activate::offers(&ctx, 0)
            .iter()
            .all(|offer| offer.source != MALZAHAR));
        assert_eq!(
            activate::activate(&mut ctx, 0, MALZAHAR, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, MALZAHAR, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(!ctx.card(MALZAHAR).unwrap().exhausted);
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::BF1));
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
        assert!(!add_rainbow(&mut ctx, 0, ADDS));
        assert!(ctx.blob.log.contains(
            &"{seat 0} would add 2 any power · the engine has no rune pool to add to".to_string()
        ));
    }

    #[test]
    #[ignore = "engine gap · [Add] abilities as payment sources (pay::choose_payment knows only Gold) and a kill as an activation cost paid at the pay stage"]
    fn killing_a_friendly_unit_and_exhausting_him_adds_two_any_power_that_pay_the_next_spell() {
        let mut fixture = cult();
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        activate::activate(&mut ctx, 0, MALZAHAR, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(
            ctx.card(SCOUT).unwrap().zone,
            Some(fixtures::TRASH),
            "the kill is paid before the ability is on the chain"
        );
        assert!(ctx.card(MALZAHAR).unwrap().exhausted);
        assert!(
            ctx.blob.chain.is_empty(),
            "429 · abilities that add resources can't be reacted to"
        );
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "two any power pay Spark's two energy"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }
}
