use super::prelude::{
    a_card, activated, card_target, card_targets, done, empower, empowered, exhausting_self,
    legend, named, ready, target, usable_if, GEAR, ONE_ENERGY,
};
use super::{Card, Cost, Flow, Item, Keyword, Power, Stage, TargetKind, TargetSpec, Timing};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 2,
    power: &[Power::Rainbow, Power::Rainbow],
};
pub const GEAR_WHILE_EMPOWERED: u8 = 2;

pub const TWO_GEAR: TargetSpec = target(
    GEAR,
    GEAR_WHILE_EMPOWERED,
    GEAR_WHILE_EMPOWERED,
    TargetKind::Card,
    "two gear to ready",
);

fn ready_gear(ctx: &mut Ctx, gear: u32) {
    if ready(ctx, gear) {
        ctx.narrate(format!("{{card {gear}}} readies"));
    }
}

fn ready_one(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(gear) = card_target(ctx, item, 0) {
        ready_gear(ctx, gear);
    }
    done()
}

fn ready_two(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for gear in card_targets(ctx, item) {
        ready_gear(ctx, gear);
    }
    done()
}

pub static CARD: Card = legend(
    "Jayce - Defender of Tomorrow",
    &[Keyword::Empower(EMPOWER)],
    &[
        empower(EMPOWER),
        named(
            exhausting_self(activated(
                Timing::Sorcery,
                ONE_ENERGY,
                &[a_card(GEAR, "a gear to ready")],
                ready_one,
            )),
            "ready a gear",
        ),
        named(
            usable_if(
                exhausting_self(activated(
                    Timing::Sorcery,
                    ONE_ENERGY,
                    &[TWO_GEAR],
                    ready_two,
                )),
                empowered,
            ),
            "ready two gear",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::{Event, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const JAYCE: u32 = fixtures::LEGEND_CARD;
    const HAMMER: u32 = 90;
    const CANNON: u32 = 91;
    const THEIR_GEAR: u32 = 92;
    const SPARE_RUNES: [u32; 3] = [46, 47, 48];

    fn gear(id: u32, seat: u8, name: &str) -> CardInfo {
        CardInfo {
            exhausted: true,
            ..fixtures::gear(id, fixtures::BASE, seat, name, 2)
        }
    }

    fn lab(empowered: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(JAYCE).unwrap().name = CARD.name.into();
        fixture.table.cards.push(gear(HAMMER, 0, "Mercury Hammer"));
        fixture.table.cards.push(gear(CANNON, 0, "Mercury Cannon"));
        fixture.table.cards.push(gear(THEIR_GEAR, 1, "Zaun Wrench"));
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        if empowered {
            fixture.table.counters.push(CounterInfo {
                target: Target::Card(JAYCE),
                counter: COUNTER_EMPOWERED,
                value: 1,
            });
            fixture.table.counters.sort();
        }
        fixture.resolve();
        fixture
    }

    fn labels(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn resolve_all(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_prints_empower_and_two_one_energy_exhausts_the_second_for_empowered() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 3);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.label, Some("empower"));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        let one = &CARD.abilities[1];
        assert_eq!(one.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(one.cost, Some(ONE_ENERGY));
        assert_eq!(one.self_cost, SelfCost::Exhaust);
        assert!(one.usable.is_none(), "828.1.b.1 · the plain ability stays");
        assert_eq!(one.targets[0].filter, GEAR);
        assert_eq!((one.targets[0].min, one.targets[0].max), (1, 1));
        let two = &CARD.abilities[2];
        assert_eq!(two.cost, Some(ONE_ENERGY));
        assert_eq!(two.self_cost, SelfCost::Exhaust);
        assert!(two.usable.is_some());
        assert_eq!(two.targets, [TWO_GEAR]);
        assert_eq!((TWO_GEAR.min, TWO_GEAR.max), (2, 2));
        assert_eq!(TWO_GEAR.filter, GEAR);
    }

    #[test]
    fn unempowered_he_offers_empower_and_the_single_ready_and_refuses_the_pair() {
        let mut fixture = lab(false);
        let mut ctx = fixture.ctx();
        assert_eq!(
            labels(&ctx),
            [
                (
                    format!("{{card {JAYCE}}}: empower (2 energy and 2 any power)"),
                    true
                ),
                (
                    format!("{{card {JAYCE}}}: ready a gear (1 energy, exhaust)"),
                    true
                ),
            ]
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, JAYCE, 2),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the Empowered ability is not his yet"
        );
        activate::activate(&mut ctx, 0, JAYCE, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {HAMMER}}}"),
                format!("{{card {CANNON}}}"),
                format!("{{card {THEIR_GEAR}}}"),
                "cancel".to_string()
            ],
            "a gear · theirs too"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_GEAR}}}")).unwrap();
        assert!(ctx.card(JAYCE).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 5, "one energy from one rune");
        assert!(
            ctx.card(THEIR_GEAR).unwrap().exhausted,
            "nothing until it resolves"
        );
        resolve_all(&mut ctx);
        assert!(!ctx.card(THEIR_GEAR).unwrap().exhausted);
        assert!(ctx.card(HAMMER).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == THEIR_GEAR
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn empowered_he_readies_two_chosen_gear_for_one_energy_and_the_exhaust() {
        let mut fixture = lab(true);
        let mut ctx = fixture.ctx();
        assert!(ctx.is_empowered(JAYCE));
        assert_eq!(
            labels(&ctx),
            [
                (
                    format!("{{card {JAYCE}}}: ready a gear (1 energy, exhaust)"),
                    true
                ),
                (
                    format!("{{card {JAYCE}}}: ready two gear (1 energy, exhaust)"),
                    true
                ),
            ],
            "377.2.b · Empower is gone, the pair is offered"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, JAYCE, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        activate::activate(&mut ctx, 0, JAYCE, 2).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.min, prompt.max), (2, 2));
        fixtures::choose(&mut ctx, 0, &format!("{{card {HAMMER}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {CANNON}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(HAMMER), TargetRef::Card(CANNON)]
        );
        assert!(ctx.card(JAYCE).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 5);
        resolve_all(&mut ctx);
        assert!(!ctx.card(HAMMER).unwrap().exhausted);
        assert!(!ctx.card(CANNON).unwrap().exhausted);
        assert!(
            ctx.card(THEIR_GEAR).unwrap().exhausted,
            "the third gear was not chosen"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, JAYCE, 1),
            Err(Refusal::Exhausted),
            "one exhaust a turn, whichever ability"
        );
    }

    #[test]
    fn with_one_gear_on_the_board_the_pair_has_no_legal_targets() {
        let mut fixture = lab(true);
        fixture
            .table
            .cards
            .retain(|card| card.id != CANNON && card.id != THEIR_GEAR);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, JAYCE, 2),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "355.8 · two gear must be chosen"
        );
        assert_eq!(
            labels(&ctx),
            [(
                format!("{{card {JAYCE}}}: ready a gear (1 energy, exhaust)"),
                true
            )]
        );
    }

    #[test]
    fn empower_pays_two_energy_and_two_rainbow_and_puts_the_ability_on_the_chain() {
        let mut fixture = lab(false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, JAYCE, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target, no confirm");
        assert!(
            !ctx.card(JAYCE).unwrap().exhausted,
            "827.1 · Empower never exhausts him"
        );
        assert_eq!(
            (ctx.ready_runes_of(0).len(), ctx.runes_of(0).len()),
            (4, 5),
            "two runes exhaust for the energy, two runes recycle for the rainbow"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        drop(ctx);
        let mut poor = lab(false);
        poor.table
            .cards
            .retain(|card| !SPARE_RUNES.contains(&card.id) && card.id != 43 && card.id != 42);
        poor.resolve();
        let mut ctx = poor.ctx();
        assert_eq!(ctx.runes_of(0).len(), 2, "one ready and the exhausted one");
        assert!(
            activate::activate(&mut ctx, 0, JAYCE, 0).is_err(),
            "one ready rune cannot make two energy"
        );
        assert!(!activate::offers(&ctx, 0)[0].enabled, "the offer is greyed");
    }

    #[test]
    fn resolving_empower_makes_him_empowered_and_opens_the_pair() {
        let mut fixture = lab(false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, JAYCE, 0).unwrap();
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(JAYCE));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Empowered { card, .. } if *card == JAYCE)));
        assert!(activate::legal(&ctx, 0, JAYCE, 2).is_ok());
        assert_eq!(
            activate::activate(&mut ctx, 0, JAYCE, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
    }
}
