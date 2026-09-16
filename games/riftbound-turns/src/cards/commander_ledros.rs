use super::prelude::{friendly_units, unit};
use super::{Card, Domain, Keyword};
use crate::engine::cost::{Cost, Need};
use crate::engine::ctx::{Cause, Ctx, Killed};

pub const PER_KILL: Need = Need::Domain(Domain::Order);

pub fn kill_candidates(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut units = friendly_units(ctx, seat);
    units.sort_unstable();
    units
}

pub fn discount_for(killed: usize) -> Cost {
    Cost {
        power: vec![PER_KILL; killed],
        ..Cost::free()
    }
}

pub fn pay_kills(ctx: &mut Ctx, units: &[u32]) -> usize {
    let mut killed = 0;
    for unit in units {
        if ctx.kill(*unit, Cause::Cost) == Killed::NotOnBoard {
            continue;
        }
        ctx.narrate(format!("{{card {unit}}} is killed as an additional cost"));
        killed += 1;
    }
    killed
}

pub static CARD: Card = unit(
    "Commander Ledros",
    &[Keyword::Deflect(1), Keyword::Ganking],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::PromptWhy;
    use agni_plugin_sdk::table::CardInfo;

    const LEDROS: u32 = 90;
    const SCOUT: u32 = 91;
    const GUARD: u32 = 92;

    fn ledros() -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(4),
            domain: vec!["Order".into()],
            ..fixtures::unit(LEDROS, fixtures::HAND, 0, "Commander Ledros", 8)
        }
    }

    fn barracks() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ledros());
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(GUARD, fixtures::BASE, 0, "Guard", 3));
        for id in [46, 47, 48, 49] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Order", false));
        }
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn ledros_prints_deflect_one_and_ganking_and_no_optional_rune_cost() {
        assert!(std::ptr::eq(script_of("Commander Ledros").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deflect(1), Keyword::Ganking]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(
            CARD.additional.is_none(),
            "Card.additional is a rune cost paid once; his is any number of kills"
        );
        let mut fixture = barracks();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(LEDROS, Keyword::Ganking));
        assert_eq!(ctx.deflect_of(LEDROS), 1);
    }

    #[test]
    fn each_kill_strikes_one_order_need_from_his_printed_cost_and_never_the_energy() {
        let mut fixture = barracks();
        let ctx = fixture.ctx();
        let printed = cost::printed_of(&ctx, LEDROS, false);
        assert_eq!(printed.energy, 6);
        assert_eq!(printed.power, vec![PER_KILL; 4]);
        assert_eq!(discount_for(0), Cost::free());
        let two = printed.clone().less(&discount_for(2));
        assert_eq!(two.energy, 6);
        assert_eq!(two.power, vec![PER_KILL; 2]);
        let all = printed.clone().less(&discount_for(4));
        assert_eq!(all.power, Vec::<Need>::new());
        assert_eq!(all.energy, 6, "the energy is never reduced");
        let more = printed.less(&discount_for(6));
        assert_eq!(
            more.power,
            Vec::<Need>::new(),
            "356.6 · a fifth kill buys nothing"
        );
    }

    #[test]
    fn the_candidates_are_his_controllers_units_and_the_kills_are_paid_together() {
        let mut fixture = barracks();
        let mut ctx = fixture.ctx();
        assert_eq!(kill_candidates(&ctx, 0), [fixtures::VI, SCOUT, GUARD]);
        assert_eq!(
            kill_candidates(&ctx, 1),
            [fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
        assert_eq!(pay_kills(&mut ctx, &[SCOUT, GUARD]), 2);
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(GUARD).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(kill_candidates(&ctx, 0), [fixtures::VI]);
        assert_eq!(
            pay_kills(&mut ctx, &[SCOUT, fixtures::THEIR_UNIT]),
            1,
            "a dead unit pays nothing; an enemy unit is not a candidate but the kill itself is not refused here"
        );
        assert_eq!(pay_kills(&mut ctx, &[]), 0, "you may kill none");
    }

    #[test]
    #[ignore = "engine gap · an any-number kill additional cost with a per-kill power discount at the pay stage; Card.additional is a fixed optional rune cost and cannot hold a kill list"]
    fn playing_him_offers_any_number_of_friendly_units_and_each_one_killed_takes_an_order_off() {
        let mut fixture = barracks();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 7);
        fixtures::play_from_hand(&mut ctx, 0, LEDROS).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { .. })),
            "which friendly units die, zero or more: {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {GUARD}}}")).unwrap();
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(GUARD).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.location(LEDROS), Some(Location::Base(0)));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "six runes for the energy, two of them Order recycled for the power"
        );
    }
}
