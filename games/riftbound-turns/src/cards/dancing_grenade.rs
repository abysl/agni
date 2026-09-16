use super::prelude::{a_unit, card_target, deal, done, play, spell, RAINBOW};
use super::{Card, Cost, Flow, Item, Paying, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::{cost, pay};
use crate::state::{Leave, Origin};

pub const DAMAGE: u8 = 2;
pub const BONUS_PER_PRIOR_DEAL: u8 = 1;
pub const REPLAY: Cost = RAINBOW;

pub fn damage_on_replay(prior_deals_this_turn: u8) -> u8 {
    DAMAGE.saturating_add(prior_deals_this_turn.saturating_mul(BONUS_PER_PRIOR_DEAL))
}

pub fn may_play_again_from_the_trash(ctx: &mut Ctx, item: &Item, seat: u8, price: Cost) -> bool {
    let me = item.kind.source();
    let total = cost::of_script(&price, &[]);
    let again = cost::play_item(
        ctx,
        seat,
        me,
        Origin::Trash {
            leave: Leave::Recycle,
        },
    );
    if !pay::affordable_for(ctx, seat, &total, Paying::Item(&again)) {
        ctx.narrate(format!(
            "{{seat {seat}}} cannot pay to play {{card {me}}} again from the trash"
        ));
        return false;
    }
    ctx.narrate(format!(
        "{{seat {seat}}} may play {{card {me}}} again from the trash for 1 any power · the play waits for the engine"
    ));
    false
}

fn lob(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if deal(ctx, item, unit, DAMAGE) {
        ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
    }
    let controller = ctx.controller(unit);
    may_play_again_from_the_trash(ctx, item, controller, REPLAY);
    done()
}

pub static CARD: Card = spell("Dancing Grenade", &[], &[play(&[a_unit("a unit")], lob)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{Cause, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const GRENADE: u32 = 90;
    const THEIR_GRENADE: u32 = 91;
    const BRUTE: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            GRENADE,
            fixtures::HAND,
            0,
            "Dancing Grenade",
            2,
            1,
        ));
        fixture.table.cards.push(fixtures::spell(
            THEIR_GRENADE,
            fixtures::HAND,
            1,
            "Dancing Grenade",
            2,
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, GRENADE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_sorcery_over_any_unit_and_the_replay_prices_one_bonus_per_prior_deal() {
        assert!(std::ptr::eq(script_of("Dancing Grenade").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(REPLAY, RAINBOW);
        assert_eq!(damage_on_replay(0), 2);
        assert_eq!(damage_on_replay(1), 3, "one prior deal · one Bonus Damage");
        assert_eq!(damage_on_replay(3), 5);
    }

    #[test]
    fn two_lands_on_a_unit_anywhere_and_the_replay_is_offered_to_that_units_controller() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GRENADE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "{card 92}", "cancel"],
            "any unit, in a base or at a battlefield"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::THEIR_UNIT,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(
            !ctx.on_board(fixtures::THEIR_UNIT),
            "two kills the 2-Might Jinx"
        );
        assert!(ctx.blob.log.contains(&"{card 81} takes 2".to_string()));
        assert!(ctx.blob.log.contains(
            &"{seat 1} may play {card 90} again from the trash for 1 any power · the play waits for the engine".to_string()
        ));
        assert_eq!(ctx.card(GRENADE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn hitting_my_own_unit_offers_the_replay_to_me_and_a_seat_without_a_rune_cannot_pay() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, fixtures::VI);
        assert_eq!(ctx.damage_on(fixtures::VI), 2);
        assert!(ctx.blob.log.contains(
            &"{seat 0} may play {card 90} again from the trash for 1 any power · the play waits for the engine".to_string()
        ));
        drop(ctx);
        let mut broke = armed();
        broke
            .table
            .cards
            .retain(|card| ![44, 45].contains(&card.id));
        broke.resolve();
        let mut ctx = broke.ctx();
        cast_at(&mut ctx, BRUTE);
        assert_eq!(ctx.damage_on(BRUTE), 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} cannot pay to play {card 90} again from the trash".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · a play as an effect mid-resolution · the spell is still on the chain as it resolves and play::begin cannot play it again from the trash, nor is the count of deals per spell per turn kept; may_play_again_from_the_trash narrates until the engine offers the unit's controller the paid replay"]
    fn the_units_controller_pays_a_rainbow_to_replay_it_and_the_replay_deals_two_plus_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "seat 1 may pay 1 any power"
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        fixtures::choose(&mut ctx, 1, "yes").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the grenade is on the chain again");
        assert_eq!(ctx.blob.chain[0].controller, 1);
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.damage_on(fixtures::VI),
            3,
            "two plus one Bonus Damage for the one deal before it"
        );
    }

    #[test]
    fn non_units_are_refused_a_gone_target_is_not_hit_and_the_other_seat_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_GRENADE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, GRENADE).unwrap();
        for wrong in [fixtures::GROUNDS, fixtures::HAND_UNIT, fixtures::RUNE_A] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BRUTE, fixtures::HAND, 1), 1)
            .unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("again from the trash")));
        assert_eq!(ctx.card(GRENADE).unwrap().zone, Some(fixtures::TRASH));
    }
}
