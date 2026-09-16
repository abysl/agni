use super::prelude::{
    a_card, activated, bounce, card_target, done, exhausting_self, facedown_of, gear, named,
};
use super::{Card, Cost, Filter, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;
use crate::engine::hide;

pub const ANOTHER_FRIENDLY_GEAR_UNIT_OR_HIDDEN_CARD: Filter = Filter::And(&[
    Filter::Or(&[Filter::Gear, Filter::Unit, Filter::Facedown]),
    Filter::Friendly,
    Filter::NotSelf,
]);

pub fn returnable_hidden_cards(ctx: &Ctx, seat: u8) -> Vec<u32> {
    facedown_of(ctx, seat)
}

pub fn return_to_owners_hand(ctx: &mut Ctx, card: u32) -> bool {
    if hide::is_facedown(ctx, card) {
        let owner = ctx.controller(card);
        hide::drop_facedown(ctx, card);
        ctx.narrate(format!(
            "{{seat {owner}}}'s hidden card returns to their hand"
        ));
        return true;
    }
    bounce(ctx, card)
}

fn unpack(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(card) = card_target(ctx, item, 0) {
        return_to_owners_hand(ctx, card);
    }
    done()
}

pub static CARD: Card = gear(
    "Pack of Wonders",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            Cost::FREE,
            &[a_card(
                ANOTHER_FRIENDLY_GEAR_UNIT_OR_HIDDEN_CARD,
                "another friendly gear, unit, or hidden card to return to hand",
            )],
            unpack,
        )),
        "return another friendly gear, unit, or hidden card to hand",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;

    const PACK: u32 = 90;
    const TRINKET: u32 = 91;

    fn packed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut pack = fixtures::gear(PACK, fixtures::BASE, 0, "Pack of Wonders", 2);
        pack.domain = vec!["Chaos".into()];
        fixture.table.cards.push(pack);
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn offered_cards(ctx: &Ctx) -> Vec<Option<u32>> {
        prompts::offered(ctx).iter().map(|opt| opt.card).collect()
    }

    #[test]
    fn the_script_is_an_exhaust_activation_over_another_friendly_gear_unit_or_hidden_card() {
        let fixture = packed();
        assert!(std::ptr::eq(fixture.scripts.of_card(PACK).unwrap(), &CARD));
        assert_eq!(CARD.name, "Pack of Wonders");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(
            ability.targets[0].filter,
            ANOTHER_FRIENDLY_GEAR_UNIT_OR_HIDDEN_CARD
        );
    }

    #[test]
    fn the_pack_offers_friendly_gear_and_units_but_never_itself_and_bounces_the_pick() {
        let mut fixture = packed();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, PACK, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            offered_cards(&ctx),
            [Some(fixtures::VI), Some(TRINKET), None],
            "Vi and the trinket, not the pack, not the enemy pieces"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {TRINKET}}}")).unwrap();
        assert!(ctx.card(PACK).unwrap().exhausted);
        assert_eq!(ctx.card(TRINKET).unwrap().zone, Some(fixtures::BASE));
        resolve_chain(&mut ctx);
        assert_eq!(ctx.card(TRINKET).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(TRINKET).unwrap().seat, 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TRINKET}}} returns to hand")));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_at_a_battlefield_comes_home_to_hand_with_its_state_dropped() {
        let mut fixture = packed();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.stun(fixtures::VI);
        activate::activate(&mut ctx, 0, PACK, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::HAND));
        assert!(!ctx.is_stunned(fixtures::VI));
        assert!(
            ctx.blob.holder(fixtures::BF1).is_none(),
            "the battlefield is left empty"
        );
    }

    #[test]
    fn enemy_pieces_and_the_pack_itself_are_refused_and_an_empty_board_refuses_the_activation() {
        let mut fixture = packed();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, PACK, 0).unwrap();
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[PACK]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "another, not itself"
        );
        ctx.blob.close_prompt();
        crate::engine::play::cancel(&mut ctx, 1);
        assert!(!ctx.card(PACK).unwrap().exhausted);
        drop(ctx);
        fixture
            .table
            .cards
            .retain(|card| card.id != TRINKET && card.id != fixtures::VI);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, PACK, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets))
        );
    }

    #[test]
    fn a_facedown_card_of_the_controller_is_a_returnable_hidden_card_and_returning_it_unhides_it() {
        let mut fixture = packed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        hide::hide(&mut ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        assert_eq!(returnable_hidden_cards(&ctx, 0), [fixtures::HAND_HIDDEN]);
        assert!(returnable_hidden_cards(&ctx, 1).is_empty());
        assert!(return_to_owners_hand(&mut ctx, fixtures::HAND_HIDDEN));
        assert!(!hide::is_facedown(&ctx, fixtures::HAND_HIDDEN));
        assert_eq!(
            ctx.card(fixtures::HAND_HIDDEN).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s hidden card returns to their hand".to_string()));
        assert!(
            !return_to_owners_hand(&mut ctx, fixtures::HAND_HIDDEN),
            "already in hand"
        );
    }

    #[test]
    fn a_hidden_card_is_offered_beside_gear_and_units_and_returns_to_hand() {
        let mut fixture = packed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        hide::hide(&mut ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        activate::activate(&mut ctx, 0, PACK, 0).unwrap();
        assert!(offered_cards(&ctx).contains(&Some(fixtures::HAND_HIDDEN)));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_HIDDEN)).unwrap();
        resolve_chain(&mut ctx);
        assert!(!hide::is_facedown(&ctx, fixtures::HAND_HIDDEN));
    }
}
