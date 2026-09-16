use crate::engine::ctx::Ctx;
use crate::state::{
    Expiry, FLAG_ENTERED_THIS_TURN, FLAG_NO_MOVE_BY_OWNER, FLAG_ONCE_BY_SEAT, FLAG_ONCE_USED,
    FLAG_SHROUDED, FLAG_STUNNED,
};

pub fn this_turn(ctx: &Ctx) -> Expiry {
    Expiry::EndOfTurn(ctx.turn())
}

pub fn clear_stuns(ctx: &mut Ctx) {
    let stunned: Vec<u32> = ctx
        .blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_STUNNED))
        .map(|row| row.id)
        .collect();
    for card in stunned {
        ctx.unstun(card);
    }
    for row in &mut ctx.blob.cards {
        row.set(FLAG_SHROUDED, false);
    }
}

pub fn at_expiration(ctx: &mut Ctx) {
    let until = this_turn(ctx);
    ctx.expire(until);
    ctx.empty_pools();
    for row in &mut ctx.blob.seats {
        row.reset_turn();
    }
    ctx.blob.clear_scored();
    ctx.blob.deaths_this_turn.clear();
    ctx.blob.clear_excess();
    for row in &mut ctx.blob.cards {
        row.set(FLAG_ENTERED_THIS_TURN, false);
        row.set(FLAG_ONCE_USED, false);
        row.set(FLAG_NO_MOVE_BY_OWNER, false);
        row.set(FLAG_ONCE_BY_SEAT, false);
    }
    ctx.blob
        .preventions
        .retain(|prevention| prevention.until != until);
    ctx.blob.cards.retain(|row| !row.is_default());
}

pub fn at_combat_end(ctx: &mut Ctx) {
    ctx.expire(Expiry::CombatEnd);
    ctx.blob
        .preventions
        .retain(|prevention| prevention.until != Expiry::CombatEnd);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{once_by_seat, Amount, DamageSource};

    #[test]
    fn the_ending_step_clears_shrouds_and_expiration_clears_the_seat_onces_and_preventions() {
        let mut fixture = Fixture::enforced();
        let mut ctx = fixture.ctx();
        ctx.shroud(fixtures::VI);
        ctx.stun(fixtures::VI);
        ctx.set_flag(fixtures::GROUNDS, once_by_seat(1), true);
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::All,
            Expiry::EndOfTurn(1),
        );
        ctx.prevent(DamageSource::Combat, Amount::N(2), Expiry::EndOfTurn(2));
        ctx.prevent(DamageSource::Any, Amount::All, Expiry::CombatEnd);
        ctx.record_excess_damage(0, fixtures::BF1, 4);
        clear_stuns(&mut ctx);
        assert!(!ctx.is_shrouded(fixtures::VI));
        assert!(!ctx.is_stunned(fixtures::VI));
        assert!(ctx.has_flag(fixtures::GROUNDS, once_by_seat(1)));
        assert_eq!(ctx.blob.preventions.len(), 3);
        at_combat_end(&mut ctx);
        assert_eq!(ctx.blob.preventions.len(), 2);
        assert_eq!(
            ctx.excess_damage_in_attack(0, fixtures::BF1),
            Some(4),
            "the excess of an attack outlives the combat"
        );
        at_expiration(&mut ctx);
        assert_eq!(ctx.excess_damage_in_attack(0, fixtures::BF1), None);
        assert!(!ctx.has_flag(fixtures::GROUNDS, once_by_seat(1)));
        assert_eq!(ctx.blob.preventions.len(), 1);
        assert_eq!(ctx.blob.preventions[0].until, Expiry::EndOfTurn(2));
        assert!(
            ctx.state_of(fixtures::GROUNDS).is_none(),
            "an empty row is pruned"
        );
    }
}
