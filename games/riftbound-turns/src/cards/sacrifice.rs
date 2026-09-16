use super::cruel_patron::pay_kill_cost;
use super::prelude::{
    a_card, card_target, channel_exhausted, done, draw, friendly_units, is_mighty, play, spell,
    MIGHTY,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::{Ctx, Killed};

pub const DRAWS: usize = 2;
pub const RUNES: usize = 1;
pub const KILLS: usize = 1;
pub const NOT_MIGHTY: Filter = Filter::MightAtMost((MIGHTY - 1) as u8);
pub const FRIENDLY_MIGHTY_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::Not(&NOT_MIGHTY)]);
pub const KILL_COST: TargetSpec = a_card(
    FRIENDLY_MIGHTY_UNIT,
    "a friendly Mighty unit to kill as an additional cost",
);

pub fn kill_candidates(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut units: Vec<u32> = friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| is_mighty(ctx, *unit))
        .collect();
    units.sort_unstable();
    units
}

pub fn kill_cost_payable(ctx: &Ctx, seat: u8) -> bool {
    kill_candidates(ctx, seat).len() >= KILLS
}

pub fn killed_as_the_additional_cost_until_play_pays_it_at_the_pay_stage(
    ctx: &mut Ctx,
    item: &Item,
) -> bool {
    let Some(unit) = card_target(ctx, item, 0) else {
        return false;
    };
    if ctx.controller(unit) != item.controller || !is_mighty(ctx, unit) {
        ctx.narrate(format!(
            "{{card {unit}}} is no longer a friendly Mighty unit · the cost can't be paid"
        ));
        return false;
    }
    pay_kill_cost(ctx, unit) == Killed::Yes
}

fn sacrifice(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if !killed_as_the_additional_cost_until_play_pays_it_at_the_pay_stage(ctx, item) {
        ctx.narrate(format!(
            "{{card {}}} does nothing · its additional cost was never paid",
            item.kind.source()
        ));
        return done();
    }
    draw(ctx, seat, DRAWS);
    channel_exhausted(ctx, seat, RUNES);
    done()
}

pub static CARD: Card = spell(
    "Sacrifice",
    &[Keyword::Reaction],
    &[play(&[KILL_COST], sacrifice)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::priority;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SACRIFICE: u32 = 90;
    const THEIR_SACRIFICE: u32 = 91;
    const TITAN: u32 = 92;
    const THEIR_TITAN: u32 = 93;
    const ORDER_RUNE: u32 = 46;

    fn sacrifice_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Sacrifice", 1, 0);
        card.domain = vec!["Order".into()];
        card
    }

    fn altar() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sacrifice_card(SACRIFICE, 0));
        fixture.table.cards.push(sacrifice_card(THEIR_SACRIFICE, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(TITAN, fixtures::BF1, 0, "Titan", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_TITAN, fixtures::BF2, 1, "Colossus", 6));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 1, "Order", false));
        fixture.resolve();
        fixture
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_reaction_whose_one_pick_is_the_friendly_mighty_unit_that_pays() {
        assert!(std::ptr::eq(script_of("Sacrifice").unwrap(), &CARD));
        assert_eq!(CARD.name, "Sacrifice");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert!(
            CARD.additional.is_none(),
            "Card.additional is an optional rune cost; this kills a Mighty unit"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, &[KILL_COST]);
        assert_eq!((KILL_COST.min, KILL_COST.max), (1, 1));
        assert_eq!(KILL_COST.kind, TargetKind::Card);
        assert_eq!(NOT_MIGHTY, Filter::MightAtMost(4));
        assert_eq!((DRAWS, RUNES, KILLS), (2, 1, 1));
    }

    #[test]
    fn the_candidates_are_the_controllers_mighty_units_and_paying_kills_one() {
        let mut fixture = altar();
        let mut ctx = fixture.ctx();
        assert_eq!(kill_candidates(&ctx, 0), [TITAN], "Vi at 3 is not Mighty");
        assert_eq!(kill_candidates(&ctx, 1), [THEIR_TITAN]);
        assert!(kill_cost_payable(&ctx, 0));
        assert_eq!(pay_kill_cost(&mut ctx, TITAN), Killed::Yes);
        assert!(ctx.in_trash(TITAN));
        assert!(
            !kill_cost_payable(&ctx, 0),
            "356.2.a.1 · nothing left to pay with"
        );
    }

    #[test]
    fn the_titan_dies_as_the_cost_then_two_are_drawn_and_one_rune_arrives_exhausted() {
        let mut fixture = altar();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let pool = ctx.table.held(fixtures::RUNE_POOL, 0).count();
        fixtures::play_from_hand(&mut ctx, 0, SACRIFICE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {TITAN}}}"), "cancel".to_string()],
            "only my Mighty unit: not Vi, not their Colossus"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {TITAN}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(TITAN)]);
        assert!(ctx.on_board(TITAN), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_trash(TITAN));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: true, .. } if *card == TITAN
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TITAN}}} is killed as an additional cost")));
        assert_eq!(drew(&ctx, 0), 2);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "one played, two drawn");
        assert_eq!(ctx.table.held(fixtures::RUNE_POOL, 0).count(), pool + 1);
        let arrived: Vec<&CardInfo> = ctx
            .table
            .held(fixtures::RUNE_POOL, 0)
            .filter(|rune| rune.zone == Some(fixtures::RUNE_POOL) && rune.id >= 30 && rune.id < 40)
            .collect();
        assert_eq!(arrived.len(), 1, "one rune off the rune deck");
        assert!(
            arrived[0].exhausted,
            "channelled exhausted: {:?}",
            arrived[0]
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert_eq!(ctx.card(SACRIFICE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_reacts_to_my_spell_with_its_own_colossus() {
        let mut fixture = altar();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_SACRIFICE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {THEIR_TITAN}}}"), "cancel".to_string()]
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIR_TITAN}}}")).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert!(ctx.in_trash(THEIR_TITAN));
        assert_eq!(drew(&ctx, 1), 2);
        assert_eq!(drew(&ctx, 0), 0);
        assert!(ctx.on_board(TITAN), "mine is untouched");
    }

    #[test]
    fn a_unit_that_stopped_being_mighty_cannot_pay_and_the_spell_does_nothing() {
        let mut fixture = altar();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SACRIFICE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TITAN}}}")).unwrap();
        let shrink = crate::state::ChainItem::new(
            9,
            crate::state::ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            crate::state::Origin::Hand,
        );
        crate::cards::prelude::might_this_turn(&mut ctx, &shrink, TITAN, -1, None);
        assert!(!is_mighty(&ctx, TITAN));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(TITAN), "no kill");
        assert_eq!(drew(&ctx, 0), 0, "no cost, no effect");
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} does nothing · its additional cost was never paid".to_string()));
        assert_eq!(ctx.card(SACRIFICE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn vi_a_gear_and_their_colossus_are_refused_and_without_a_mighty_unit_only_cancel_is_offered() {
        let mut fixture = altar();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SACRIFICE).unwrap();
        for wrong in [
            fixtures::VI,
            THEIR_TITAN,
            fixtures::HAND_GEAR,
            fixtures::GROUNDS,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly Mighty unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(SACRIFICE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut bare = altar();
        bare.table.cards.retain(|card| card.id != TITAN);
        bare.resolve();
        let mut ctx = bare.ctx();
        assert!(!kill_cost_payable(&ctx, 0));
        fixtures::play_from_hand(&mut ctx, 0, SACRIFICE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "356.2.a.1 · the cost cannot be paid, the play can only be taken back"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(SACRIFICE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    #[ignore = "engine gap · a mandatory non-resource additional cost (kill a friendly Mighty unit) at the pay stage; play::advance knows optional rune costs only, so the kill is a play-time pick paid as the spell resolves, where a response can shrink the unit and void the cost"]
    fn the_kill_is_paid_before_the_spell_is_on_the_chain_and_no_response_can_undo_it() {
        let mut fixture = altar();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SACRIFICE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TITAN}}}")).unwrap();
        assert!(
            ctx.in_trash(TITAN),
            "357.2 · the kill is paid at the pay stage, before priority"
        );
        assert!(ctx.blob.chain[0].paid_additional());
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(drew(&ctx, 0), 2);
    }
}
