use super::prelude::{
    a_unit_at_a_battlefield, activated, ask_discard, card_target, deal, discarded, discarded_kind,
    done, exhausting_self, named, unit, usable_if, ONE_ENERGY,
};
use super::{Card, Flow, Item, Source, Stage, TargetSpec, Timing, KIND_GEAR};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 4;
pub const DISCARDED: u8 = 1;
pub const TARGET: TargetSpec = a_unit_at_a_battlefield("a unit at a battlefield to deal 4");

pub fn gear_in_hand(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.hand_of(seat)
        .into_iter()
        .filter(|card| ctx.kind_of(*card) == Some(KIND_GEAR))
        .collect()
}

pub fn can_discard_a_gear(ctx: &Ctx, source: Source) -> bool {
    !ctx.hand_of(ctx.controller(source.card)).is_empty()
}

fn bombard(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    if stage.0 != DISCARDED {
        return match ask_discard(ctx, item, DISCARDED) {
            Some(ask) => Flow::Ask(ask),
            None => {
                ctx.narrate(format!(
                    "{{card {me}}} has no gear to discard · nothing is dealt"
                ));
                done()
            }
        };
    }
    if discarded_kind(ctx) != Some(KIND_GEAR) {
        let card = discarded(ctx).unwrap_or(0);
        ctx.narrate(format!(
            "{{card {card}}} is not a gear · the cost is unpaid and nothing is dealt"
        ));
        return done();
    }
    let Some(target) = card_target(ctx, item, 0) else {
        ctx.narrate(format!("{{card {me}}} · its target is gone"));
        return done();
    };
    if deal(ctx, item, target, DAMAGE) {
        ctx.narrate(format!("{{card {target}}} takes {DAMAGE}"));
    }
    done()
}

pub static CARD: Card = unit(
    "Sky Cruiser",
    &[],
    &[named(
        usable_if(
            exhausting_self(activated(Timing::Sorcery, ONE_ENERGY, &[TARGET], bombard)),
            can_discard_a_gear,
        ),
        "discard a gear: deal 4 to a unit at a battlefield",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const CRUISER: u32 = 90;
    const TARGETED: u32 = 91;

    fn cruiser() -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(CRUISER, fixtures::BASE, 0, "Sky Cruiser", 3)
        }
    }

    fn hangar() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(cruiser());
        fixture
            .table
            .cards
            .push(fixtures::unit(TARGETED, fixtures::BF1, 1, "Jinx", 6));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CRUISER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn source() -> Source {
        Source {
            card: CRUISER,
            ability: 0,
        }
    }

    #[test]
    fn the_script_is_one_gated_one_energy_exhaust_activation_aimed_at_a_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Sky Cruiser").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.usable.is_some(), "an empty hand, no activation");
        assert_eq!(ability.targets, &[TARGET]);
        assert_eq!(TARGET.filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(
            ability.label,
            Some("discard a gear: deal 4 to a unit at a battlefield")
        );
        assert_eq!(DAMAGE, 4);
    }

    #[test]
    fn the_gate_reads_a_card_in_his_controllers_hand_since_the_plugin_cannot_see_its_kind() {
        let mut fixture = hangar();
        let ctx = fixture.ctx();
        assert_eq!(gear_in_hand(&ctx, 0), [fixtures::HAND_GEAR]);
        assert!(gear_in_hand(&ctx, 1).is_empty(), "a hidden face is no gear");
        assert!(can_discard_a_gear(&ctx, source()));
        drop(ctx);
        for card in fixture.table.cards.iter_mut() {
            if card.zone == Some(fixtures::HAND) && card.owner == 0 {
                card.name = String::new();
                card.kind = None;
            }
        }
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(gear_in_hand(&ctx, 0).is_empty());
        assert!(
            can_discard_a_gear(&ctx, source()),
            "the offer stands on the hand's size · the kind is checked after the discard reveals it"
        );
        drop(ctx);
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 0);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(!can_discard_a_gear(&ctx, source()), "an empty hand");
    }

    #[test]
    fn the_activation_pays_one_and_exhausts_him_and_the_discard_of_a_gear_deals_four_at_resolution()
    {
        let mut fixture = hangar();
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == CRUISER)
            .expect("offered");
        assert!(offer.enabled);
        assert_eq!(
            offer.label,
            format!("{{card {CRUISER}}}: discard a gear: deal 4 to a unit at a battlefield (1 energy, exhaust)")
        );
        let ready = ctx.ready_runes_of(0).len();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, CRUISER, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.retain(|label| label != "cancel");
        offered.sort_unstable();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {TARGETED}}}")
            ],
            "units at battlefields only · the two in bases are not offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {TARGETED}}}")).unwrap();
        assert!(ctx.card(CRUISER).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == CRUISER
        ));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the discard waits for resolution"
        );
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: DISCARDED
            })
        );
        assert_eq!(
            ctx.blob.chain.last().map(|top| top.status),
            Some(ItemStatus::Resolving)
        );
        assert_eq!(ctx.damage_on(TARGETED), 0, "nothing before the discard");
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.in_trash(fixtures::HAND_GEAR));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(TARGETED), 4);
        assert!(ctx.on_board(TARGETED), "four on a six is not lethal");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TARGETED}}} takes 4")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn discarding_something_other_than_a_gear_leaves_the_cost_unpaid_and_deals_nothing() {
        let mut fixture = hangar();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, CRUISER, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TARGETED}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(TARGETED), 0);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} is not a gear · the cost is unpaid and nothing is dealt",
            fixtures::HAND_SPELL
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_card_in_hand_or_a_unit_at_a_battlefield_the_ability_is_refused() {
        let mut fixture = hangar();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, CRUISER, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the usable gate refuses with the engine's one gate reason"
        );
        assert!(activate::offers(&ctx, 0)
            .iter()
            .all(|offer| offer.source != CRUISER));
        assert_eq!(
            activate::activate(&mut ctx, 1, CRUISER, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(!ctx.card(CRUISER).unwrap().exhausted);
        drop(ctx);
        let mut grounded = hangar();
        grounded.table.card_mut(TARGETED).unwrap().zone = Some(fixtures::BASE);
        grounded
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        grounded.resolve();
        let mut ctx = grounded.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, CRUISER, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets))
        );
    }

    #[test]
    #[ignore = "engine gap · non-resource costs at the pay stage (the Unlicensed Armory row): 'discard a gear' is a base cost paid with the energy and the exhaust before the ability reaches the chain, offered as a kind-filtered hand pick; today the discard is asked at resolution and any card can be picked"]
    fn the_gear_is_discarded_with_the_energy_before_the_ability_reaches_the_chain() {
        let mut fixture = hangar();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, CRUISER, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TARGETED}}}")).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::HAND_GEAR)]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "paid with the exhaust");
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert_eq!(ctx.damage_on(TARGETED), 4);
    }
}
