use super::prelude::{at_end_of_turn, battlefield, done, triggered, with_statics, ONE_ENERGY};
use super::{Card, Cost, Flow, Item, Stage, Static, Trigger, Who};
use crate::engine::ctx::Ctx;
use crate::state::{ChainItem, ItemKind, When};

pub const SURCHARGE: Cost = ONE_ENERGY;
pub const HOLD: u8 = 0;
pub const LAPSE: u8 = 1;

pub fn surcharged_this_turn(ctx: &Ctx, vaults: u32, seat: u8) -> bool {
    let turn = ctx.turn();
    ctx.blob.delayed.iter().any(|delayed| {
        delayed.source == vaults
            && delayed.ability == LAPSE
            && delayed.seat == seat
            && delayed.when == When::EndOfTurn(turn)
    })
}

pub fn a_non_token_unit_of_a_surcharged_seat(ctx: &Ctx, item: &ChainItem, vaults: u32) -> bool {
    let ItemKind::Permanent { card } = item.kind else {
        return false;
    };
    ctx.is_unit(card) && !ctx.is_token(card) && surcharged_this_turn(ctx, vaults, item.controller)
}

pub fn unit_surcharge(ctx: &Ctx, item: &ChainItem, vaults: u32) -> Cost {
    if a_non_token_unit_of_a_surcharged_seat(ctx, item, vaults) {
        SURCHARGE
    } else {
        Cost::FREE
    }
}

fn tax_the_turn(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if surcharged_this_turn(ctx, item.kind.source(), seat) {
        ctx.narrate(format!(
            "{{seat {seat}}}'s non-token units already cost {} more this turn",
            SURCHARGE.energy
        ));
        return done();
    }
    at_end_of_turn(ctx, item, LAPSE, Vec::new());
    ctx.narrate(format!(
        "{{seat {seat}}}'s non-token units cost {} more energy to play this turn",
        SURCHARGE.energy
    ));
    done()
}

fn lapse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    ctx.narrate(format!(
        "the surcharge on {{seat {}}}'s units lapses",
        item.controller
    ));
    done()
}

pub static CARD: Card = with_statics(
    battlefield(
        "Vaults of Helia",
        &[],
        &[
            triggered(Trigger::Hold(Who::You), &[], tax_the_turn),
            triggered(Trigger::Reflexive, &[], lapse),
        ],
    ),
    &[Static::Surcharge(unit_surcharge)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, cost, phases, priority, settle};
    use crate::state::Origin;

    const VAULTS: u32 = fixtures::GROUNDS;
    const JINX: u32 = 90;
    const THEIR_ROOKIE: u32 = 91;

    fn vaults_held_by(seat: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(VAULTS).unwrap().name = "Vaults of Helia".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        let mut jinx = fixtures::unit(JINX, fixtures::HAND, 0, "Jinx", 2);
        jinx.energy = Some(1);
        fixture.table.cards.push(jinx);
        let mut rookie = fixtures::unit(THEIR_ROOKIE, fixtures::HAND, 1, "Their Rookie", 1);
        rookie.energy = Some(1);
        fixture.table.cards.push(rookie);
        fixture.blob.set_holder(fixtures::BF1, seat);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(VAULTS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn vaults_items(ctx: &Ctx) -> Vec<(u8, u8)> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter_map(|item| match item.kind {
                ItemKind::Trigger { source, index } if source == VAULTS => {
                    Some((item.controller, index))
                }
                _ => None,
            })
            .collect()
    }

    fn hold_and_resolve(ctx: &mut Ctx, seat: u8) {
        assert!(cleanup::score_holds(ctx, seat).contains(&fixtures::BF1));
        settle(ctx).unwrap();
        assert_eq!(vaults_items(ctx), [(seat, HOLD)]);
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
    }

    fn play_of(card: u32, seat: u8) -> ChainItem {
        ChainItem::new(7, ItemKind::Permanent { card }, seat, Origin::Hand)
    }

    #[test]
    fn the_vaults_are_a_hold_trigger_a_lapse_trigger_and_a_one_energy_surcharge() {
        assert!(std::ptr::eq(script_of("Vaults of Helia").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::Surcharge(unit_surcharge)));
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let hold = &CARD.abilities[usize::from(HOLD)];
        assert_eq!(hold.trigger, Trigger::Hold(Who::You));
        assert!(hold.targets.is_empty());
        assert!(hold.cost.is_none());
        assert!(!hold.optional);
        assert_eq!(
            CARD.abilities[usize::from(LAPSE)].trigger,
            Trigger::Reflexive
        );
        assert_eq!(SURCHARGE.energy, 1);
        assert!(SURCHARGE.power.is_empty());
    }

    #[test]
    fn holding_records_the_surcharge_for_the_holder_until_the_turn_ends() {
        let mut fixture = vaults_held_by(Some(0));
        let mut ctx = fixture.ctx();
        assert!(!surcharged_this_turn(&ctx, VAULTS, 0));
        hold_and_resolve(&mut ctx, 0);
        assert_eq!(ctx.points(0), 1);
        assert!(surcharged_this_turn(&ctx, VAULTS, 0));
        assert!(!surcharged_this_turn(&ctx, VAULTS, 1));
        assert!(!surcharged_this_turn(&ctx, fixtures::ROCKFALL, 0));
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].when, When::EndOfTurn(ctx.turn()));
        assert!(ctx.blob.log.contains(
            &"{seat 0}'s non-token units cost 1 more energy to play this turn".to_string()
        ));
        phases::end_turn(&mut ctx).unwrap();
        while !ctx.blob.chain.is_empty() {
            let holder = priority::holder(&ctx).unwrap();
            priority::pass(&mut ctx, holder).unwrap();
        }
        assert!(
            ctx.blob.delayed.is_empty(),
            "the surcharge lapses with the turn"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"the surcharge on {seat 0}'s units lapses".to_string()));
        assert!(!surcharged_this_turn(&ctx, VAULTS, 0));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_surcharge_reads_the_holders_non_token_units_and_nothing_else() {
        let mut fixture = vaults_held_by(Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            unit_surcharge(&ctx, &play_of(JINX, 0), VAULTS),
            Cost::FREE,
            "nothing before the hold"
        );
        hold_and_resolve(&mut ctx, 0);
        assert!(a_non_token_unit_of_a_surcharged_seat(
            &ctx,
            &play_of(JINX, 0),
            VAULTS
        ));
        assert_eq!(unit_surcharge(&ctx, &play_of(JINX, 0), VAULTS), SURCHARGE);
        assert_eq!(
            unit_surcharge(&ctx, &play_of(THEIR_ROOKIE, 1), VAULTS),
            Cost::FREE,
            "the opponent did not hold"
        );
        assert_eq!(
            unit_surcharge(&ctx, &play_of(fixtures::HAND_GEAR, 0), VAULTS),
            Cost::FREE,
            "gear is not a unit"
        );
        assert_eq!(
            unit_surcharge(
                &ctx,
                &ChainItem::new(
                    8,
                    ItemKind::Spell {
                        card: fixtures::HAND_SPELL
                    },
                    0,
                    Origin::Hand
                ),
                VAULTS
            ),
            Cost::FREE,
            "a spell is not a unit"
        );
        assert_eq!(
            unit_surcharge(&ctx, &play_of(fixtures::SPRITE, 0), VAULTS),
            Cost::FREE,
            "a token is exempt"
        );
        assert_eq!(
            unit_surcharge(&ctx, &play_of(JINX, 0), fixtures::ROCKFALL),
            Cost::FREE,
            "Rockfall Path taxes nothing"
        );
    }

    #[test]
    fn a_second_hold_the_same_turn_records_nothing_more_and_the_opponents_hold_taxes_them() {
        let mut fixture = vaults_held_by(Some(0));
        let mut ctx = fixture.ctx();
        hold_and_resolve(&mut ctx, 0);
        ctx.blob.clear_scored();
        hold_and_resolve(&mut ctx, 0);
        assert_eq!(ctx.blob.delayed.len(), 1, "one lapse per turn");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0}'s non-token units already cost 1 more this turn".to_string()));
        drop(ctx);
        let mut theirs = vaults_held_by(Some(1));
        theirs.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        theirs.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        theirs.resolve();
        let mut ctx = theirs.ctx();
        hold_and_resolve(&mut ctx, 1);
        assert!(surcharged_this_turn(&ctx, VAULTS, 1));
        assert!(!surcharged_this_turn(&ctx, VAULTS, 0));
        assert_eq!(
            unit_surcharge(&ctx, &play_of(THEIR_ROOKIE, 1), VAULTS),
            SURCHARGE
        );
        assert_eq!(unit_surcharge(&ctx, &play_of(JINX, 0), VAULTS), Cost::FREE);
    }

    #[test]
    fn before_the_hold_the_holders_unit_plays_for_its_printed_energy() {
        let mut fixture = vaults_held_by(Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cost::of_item(&ctx, &play_of(JINX, 0), None).energy, 1);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, JINX).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 1);
    }

    #[test]
    fn after_the_hold_the_holders_non_token_units_cost_one_energy_more_this_turn() {
        let mut fixture = vaults_held_by(Some(0));
        let mut ctx = fixture.ctx();
        hold_and_resolve(&mut ctx, 0);
        assert_eq!(cost::of_item(&ctx, &play_of(JINX, 0), None).energy, 2);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, JINX).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 2);
        assert_eq!(
            cost::of_item(&ctx, &play_of(THEIR_ROOKIE, 1), None).energy,
            1,
            "the opponent's units are untouched"
        );
    }
}
