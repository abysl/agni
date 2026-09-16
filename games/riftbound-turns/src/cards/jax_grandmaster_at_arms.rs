use super::prelude::{
    a_card, a_friendly_unit, activated, attach_gear, card_target, done, exhausting_self, legend,
    named, Attached, ONE_ENERGY,
};
use super::{Card, Cost, Filter, Flow, Item, Stage, TargetSpec, Timing};
use crate::engine::ctx::Ctx;

pub const DETACHED_EQUIPMENT: Filter = Filter::And(&[
    Filter::Gear,
    Filter::Friendly,
    Filter::Equipment,
    Filter::Unattached,
]);

pub const ATTACHED_EQUIPMENT: Filter = Filter::And(&[
    Filter::Gear,
    Filter::Friendly,
    Filter::Equipment,
    Filter::Attached,
]);

pub const DETACHED_TARGET: TargetSpec =
    a_card(DETACHED_EQUIPMENT, "a detached Equipment you control");
pub const ATTACHED_TARGET: TargetSpec =
    a_card(ATTACHED_EQUIPMENT, "an attached Equipment you control");
pub const WEARER: TargetSpec = a_friendly_unit("a unit you control to equip");

fn arm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let (Some(gear), Some(unit)) = (card_target(ctx, item, 0), card_target(ctx, item, 1)) else {
        return done();
    };
    if attach_gear(ctx, gear, unit) == Attached::Already {
        ctx.narrate(format!("{{card {gear}}} is already on {{card {unit}}}"));
    }
    done()
}

pub static CARD: Card = legend(
    "Jax - Grandmaster At Arms",
    &[],
    &[
        named(
            exhausting_self(activated(
                Timing::Sorcery,
                ONE_ENERGY,
                &[DETACHED_TARGET, WEARER],
                arm,
            )),
            "attach a detached Equipment",
        ),
        named(
            exhausting_self(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[ATTACHED_TARGET, WEARER],
                arm,
            )),
            "move an attached Equipment",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, equip, gear, while_attached, with_statics};
    use crate::cards::{script_of, Grant, Keyword, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, attach, cost, play, priority};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;

    const JAX: u32 = fixtures::LEGEND_CARD;
    const LAMP: u32 = 90;
    const CLUB: u32 = 91;
    const SQUIRE: u32 = 92;

    static LAMP_CARD: Card = with_statics(
        gear("Lamp", &[Keyword::Equip(ONE_ENERGY)], &[equip(ONE_ENERGY)]),
        &[while_attached(&[Grant::Might(1)])],
    );

    static CLUB_CARD: Card = gear("Club", &[], &[]);

    fn dojo() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(JAX).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::gear(LAMP, fixtures::BASE, 0, "Lamp", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(CLUB, fixtures::BASE, 0, "Club", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(SQUIRE, fixtures::BF1, 0, "Squire", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LAMP, &LAMP_CARD)
            .with_script(CLUB, &CLUB_CARD);
        assert!(std::ptr::eq(fixture.scripts.of_card(JAX).unwrap(), &CARD));
        fixture
    }

    fn labels(offers: &[activate::Offer]) -> Vec<(String, bool)> {
        offers
            .iter()
            .filter(|offer| offer.source == JAX)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_has_two_exhaust_activations_one_priced_at_an_energy_and_one_free() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let first = &CARD.abilities[0];
        assert_eq!(first.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(first.cost, Some(ONE_ENERGY));
        assert_eq!(first.self_cost, SelfCost::Exhaust);
        assert_eq!(first.label, Some("attach a detached Equipment"));
        assert_eq!(first.targets[0].filter, DETACHED_EQUIPMENT);
        assert_eq!(first.targets[1], WEARER);
        let second = &CARD.abilities[1];
        assert_eq!(second.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(second.cost, Some(Cost::FREE));
        assert_eq!(second.self_cost, SelfCost::Exhaust);
        assert_eq!(second.label, Some("move an attached Equipment"));
        assert_eq!(second.targets[0].filter, ATTACHED_EQUIPMENT);
        assert_eq!(second.targets[1], WEARER);
        let mut fixture = dojo();
        let ctx = fixture.ctx();
        assert_eq!(cost::of_activation(&ctx, JAX, 0).label(), "1 energy");
        assert!(cost::of_activation(&ctx, JAX, 1).is_free());
    }

    #[test]
    fn the_strip_offers_the_attach_with_a_loose_equipment_and_the_move_only_once_one_is_worn() {
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        assert_eq!(
            labels(&activate::offers(&ctx, 0)),
            [(
                format!("{{card {JAX}}}: attach a detached Equipment (1 energy, exhaust)"),
                true
            )],
            "nothing is attached yet, so only the first is offered"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, JAX, 1),
            Err(Refusal::Illegal(Reason::NoLegalTargets))
        );
        assert_eq!(
            attach::attach(&mut ctx, LAMP, fixtures::VI),
            attach::Attached::Yes
        );
        assert_eq!(
            labels(&activate::offers(&ctx, 0)),
            [(
                format!("{{card {JAX}}}: move an attached Equipment (exhaust)"),
                true
            )],
            "the Lamp is worn, so only the move is offered"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, JAX, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "the Club has no Equip · it is not Equipment"
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, JAX, 1),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
    }

    #[test]
    fn one_energy_and_his_exhaust_attach_a_loose_equipment_to_a_chosen_unit_when_it_resolves() {
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, JAX, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {LAMP}}}"), "cancel".to_string()],
            "the Club is no Equipment"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {LAMP}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                "cancel".to_string(),
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {SQUIRE}}}")
            ],
            "his controller's units, anywhere"
        );
        assert!(!ctx.card(JAX).unwrap().exhausted, "nothing before the plan");
        fixtures::choose(&mut ctx, 0, &format!("{{card {SQUIRE}}}")).unwrap();
        assert!(ctx.card(JAX).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "one rune paid the energy");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == JAX
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(LAMP), TargetRef::Card(SQUIRE)]
        );
        assert_eq!(attached_to(&ctx, LAMP), None, "nothing until it resolves");
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, LAMP), Some(SQUIRE));
        assert_eq!(ctx.current_might(SQUIRE), 3, "the Lamp's +1 lands");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {LAMP}}} is attached to {{card {SQUIRE}}}")));
        assert_eq!(
            activate::activate(&mut ctx, 0, JAX, 1),
            Err(Refusal::Exhausted),
            "he is spent for the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn his_free_exhaust_moves_a_worn_equipment_onto_another_unit_and_the_old_wearer_loses_it() {
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        assert_eq!(
            attach::attach(&mut ctx, LAMP, fixtures::VI),
            attach::Attached::Yes
        );
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, JAX, 1).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {LAMP}}}"), "cancel".to_string()]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {LAMP}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SQUIRE}}}")).unwrap();
        assert!(ctx.card(JAX).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "no energy is asked");
        resolve_top(&mut ctx);
        assert_eq!(attached_to(&ctx, LAMP), Some(SQUIRE));
        assert_eq!(ctx.current_might(SQUIRE), 3);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "Vi lost the Lamp");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn moving_an_equipment_onto_its_own_wearer_changes_nothing_and_says_so() {
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        assert_eq!(
            attach::attach(&mut ctx, LAMP, fixtures::VI),
            attach::Attached::Yes
        );
        activate::activate(&mut ctx, 0, JAX, 1).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {LAMP}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_top(&mut ctx);
        assert_eq!(attached_to(&ctx, LAMP), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 4, "the +1 is not doubled");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {LAMP}}} is already on {{card {}}}",
            fixtures::VI
        )));
    }

    #[test]
    fn a_cancelled_activation_and_an_unaffordable_or_exhausted_jax_attach_nothing() {
        let mut fixture = dojo();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, JAX, 0).unwrap();
        let item = match ctx.blob.why {
            Some(PromptWhy::Target { item, .. }) => item,
            other => panic!("{other:?}"),
        };
        ctx.blob.close_prompt();
        play::cancel(&mut ctx, item);
        assert!(!ctx.card(JAX).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(attached_to(&ctx, LAMP), None);
        drop(ctx);
        let mut broke = dojo();
        for rune in [41, 42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, JAX, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            })
        );
        assert_eq!(
            labels(&activate::offers(&ctx, 0)),
            [(
                format!("{{card {JAX}}}: attach a detached Equipment (1 energy, exhaust)"),
                false
            )],
            "greyed, not gone"
        );
        drop(ctx);
        let mut spent = dojo();
        spent.table.card_mut(JAX).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, JAX, 0),
            Err(Refusal::Exhausted)
        );
        assert!(labels(&activate::offers(&ctx, 0)).is_empty());
    }
}
