use super::prelude::{adding, gear};
use super::{Adds, Card, Cost, Paying, Power};
use crate::engine::ctx::Ctx;

pub const MOST: u8 = 12;
const RAINBOWS: [Power; MOST as usize] = [Power::Rainbow; MOST as usize];

pub fn pays(energy: u8) -> Cost {
    Cost {
        energy: energy.min(MOST),
        power: &[],
    }
}

pub fn adds(energy: u8) -> Cost {
    Cost {
        energy: 0,
        power: &RAINBOWS[..usize::from(energy.min(MOST))],
    }
}

pub fn most_it_can_add(ctx: &Ctx, seat: u8, henge: u32) -> u8 {
    let ready = ctx.card(henge).is_some_and(|held| !held.exhausted);
    if !(ready && ctx.on_board(henge) && !ctx.is_facedown(henge) && ctx.controller(henge) == seat) {
        return 0;
    }
    ctx.ready_runes_of(seat).len().min(usize::from(MOST)) as u8
}

pub fn adds_while_paying(ctx: &Ctx, seat: u8, henge: u32, energy: u8) -> Option<Cost> {
    (energy > 0 && energy <= most_it_can_add(ctx, seat, henge)).then(|| adds(energy))
}

fn converts(ctx: &Ctx, seat: u8, henge: u32, _: Paying) -> Option<Adds> {
    let most = most_it_can_add(ctx, seat, henge);
    (most > 0).then(|| Adds::exhausting(adds(1)).paying(pays(1)).up_to(most))
}

pub static CARD: Card = adding(gear("Ancient Henge", &[], &[]), converts);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::{Cost as Total, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const HENGE: u32 = 90;

    fn henge(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Body".into()],
            exhausted,
            ..fixtures::gear(HENGE, zone, seat, CARD.name, 2)
        }
    }

    fn with_henge(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(henge(zone, seat, exhausted));
        fixture.resolve();
        fixture
    }

    fn rainbows(count: usize) -> Total {
        Total {
            energy: 0,
            power: vec![Need::Rainbow; count],
            ..Total::default()
        }
    }

    #[test]
    fn the_henge_is_gear_whose_add_is_paid_with_and_never_activated() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(
            CARD.abilities.is_empty(),
            "429.2 · an [Add] resolves at once and never sits on the chain"
        );
        assert!(
            CARD.keywords.is_empty(),
            "the [Reaction] is the ability's timing"
        );
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        let mut fixture = with_henge(fixtures::BASE, 0, false);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(HENGE).unwrap(), &CARD));
        assert!(ctx.is_gear(HENGE));
        assert_eq!(
            activate::legal(&ctx, 0, HENGE, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == HENGE));
    }

    #[test]
    fn it_converts_up_to_one_energy_per_ready_rune_into_that_many_rainbows() {
        let mut fixture = with_henge(fixtures::BASE, 0, false);
        let ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(
            most_it_can_add(&ctx, 0, HENGE),
            3,
            "a spent rune exhausts for no energy"
        );
        for energy in 1..=3u8 {
            let added = adds_while_paying(&ctx, 0, HENGE, energy).expect("within reach");
            assert_eq!(added.energy, 0);
            assert_eq!(added.power.len(), usize::from(energy));
            assert!(added.power.iter().all(|power| *power == Power::Rainbow));
            assert_eq!(pays(energy).energy, energy);
            assert!(pays(energy).power.is_empty());
        }
        assert_eq!(
            adds_while_paying(&ctx, 0, HENGE, 0),
            None,
            "any amount above none"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, HENGE, 4),
            None,
            "a fourth energy has no ready rune to pay it"
        );
        assert_eq!(pays(MOST + 1).energy, MOST);
        assert_eq!(adds(MOST + 1).power.len(), usize::from(MOST));
    }

    #[test]
    fn an_exhausted_henge_an_opponents_one_in_hand_or_without_ready_runes_adds_nothing() {
        let mut spent = with_henge(fixtures::BASE, 0, true);
        let ctx = spent.ctx();
        assert_eq!(most_it_can_add(&ctx, 0, HENGE), 0);
        assert_eq!(adds_while_paying(&ctx, 0, HENGE, 1), None);
        drop(ctx);
        let mut theirs = with_henge(fixtures::BASE, 1, false);
        let ctx = theirs.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, HENGE, 1), None);
        assert_eq!(
            most_it_can_add(&ctx, 1, HENGE),
            2,
            "the other seat's two ready runes"
        );
        drop(ctx);
        let mut held = with_henge(fixtures::HAND, 0, false);
        let ctx = held.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, HENGE, 1), None);
        drop(ctx);
        let mut dry = with_henge(fixtures::BASE, 0, false);
        for rune in [41, 42, 43] {
            dry.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = dry.ctx();
        assert_eq!(most_it_can_add(&ctx, 0, HENGE), 0);
        assert_eq!(adds_while_paying(&ctx, 0, HENGE, 1), None);
    }

    #[test]
    fn a_ready_henge_turns_energy_into_rainbow_at_the_pay_stage_and_pays_by_exhausting() {
        let cost = rainbows(7);
        let mut spent = with_henge(fixtures::BASE, 0, true);
        let ctx = spent.ctx();
        assert!(
            pay::plan(&ctx, 0, &cost).is_err(),
            "four runes recycle for four rainbow alone"
        );
        drop(ctx);
        let mut fixture = with_henge(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        let planned = pay::plan(&ctx, 0, &cost)
            .expect("the three ready runes exhausted for energy through the Henge add three more");
        pay::pay(&mut ctx, 0, &planned);
        assert!(
            ctx.card(HENGE).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert!(ctx.on_board(HENGE), "unlike a Gold, the Henge stays");
        assert_eq!(adds_while_paying(&ctx, 0, HENGE, 1), None);
    }
}
