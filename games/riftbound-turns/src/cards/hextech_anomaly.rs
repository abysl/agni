use super::prelude::{adding, gear};
use super::{Adds, Card, Cost, Paying, Power};
use crate::engine::ctx::Ctx;

pub const MOST: u8 = 12;
const RAINBOWS: [Power; MOST as usize] = [Power::Rainbow; MOST as usize];

pub fn pays(rainbow: u8) -> Cost {
    Cost {
        energy: 0,
        power: &RAINBOWS[..usize::from(rainbow.min(MOST))],
    }
}

pub fn adds(rainbow: u8) -> Cost {
    Cost {
        energy: rainbow.min(MOST),
        power: &[],
    }
}

pub fn most_it_can_add(ctx: &Ctx, seat: u8, anomaly: u32) -> u8 {
    let ready = ctx.card(anomaly).is_some_and(|held| !held.exhausted);
    if !(ready
        && ctx.on_board(anomaly)
        && !ctx.is_facedown(anomaly)
        && ctx.controller(anomaly) == seat)
    {
        return 0;
    }
    ctx.runes_of(seat).len().min(usize::from(MOST)) as u8
}

pub fn adds_while_paying(ctx: &Ctx, seat: u8, anomaly: u32, rainbow: u8) -> Option<Cost> {
    (rainbow > 0 && rainbow <= most_it_can_add(ctx, seat, anomaly)).then(|| adds(rainbow))
}

fn converts(ctx: &Ctx, seat: u8, anomaly: u32, _: Paying) -> Option<Adds> {
    let most = most_it_can_add(ctx, seat, anomaly);
    (most > 0).then(|| Adds::exhausting(adds(1)).paying(pays(1)).up_to(most))
}

pub static CARD: Card = adding(gear("Hextech Anomaly", &[], &[]), converts);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost::Cost as Total;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, pay};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ANOMALY: u32 = 90;

    fn anomaly(zone: u16, seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Mind".into()],
            exhausted,
            ..fixtures::gear(ANOMALY, zone, seat, CARD.name, 3)
        }
    }

    fn with_anomaly(zone: u16, seat: u8, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(anomaly(zone, seat, exhausted));
        fixture.resolve();
        fixture
    }

    fn energy(amount: u8) -> Total {
        Total {
            energy: amount,
            power: Vec::new(),
            ..Total::default()
        }
    }

    #[test]
    fn the_anomaly_is_gear_whose_add_is_paid_with_and_never_activated() {
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
        let mut fixture = with_anomaly(fixtures::BASE, 0, false);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(ANOMALY).unwrap(), &CARD));
        assert!(ctx.is_gear(ANOMALY));
        assert_eq!(
            activate::legal(&ctx, 0, ANOMALY, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "429.3 · the exhaust is paid with, not activated"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == ANOMALY));
    }

    #[test]
    fn it_converts_up_to_one_rainbow_per_rune_in_the_pool_into_that_much_energy() {
        let mut fixture = with_anomaly(fixtures::BASE, 0, false);
        let ctx = fixture.ctx();
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(
            most_it_can_add(&ctx, 0, ANOMALY),
            4,
            "a spent rune still recycles for a rainbow"
        );
        for rainbow in 1..=4u8 {
            let added = adds_while_paying(&ctx, 0, ANOMALY, rainbow).expect("within reach");
            assert_eq!(added.energy, rainbow);
            assert!(added.power.is_empty());
            assert_eq!(pays(rainbow).energy, 0);
            assert_eq!(pays(rainbow).power.len(), usize::from(rainbow));
            assert!(pays(rainbow)
                .power
                .iter()
                .all(|power| *power == Power::Rainbow));
        }
        assert_eq!(
            adds_while_paying(&ctx, 0, ANOMALY, 0),
            None,
            "any amount above none"
        );
        assert_eq!(
            adds_while_paying(&ctx, 0, ANOMALY, 5),
            None,
            "a fifth rainbow has no rune to pay it"
        );
        assert_eq!(pays(MOST + 1).power.len(), usize::from(MOST));
        assert_eq!(adds(MOST + 1).energy, MOST);
    }

    #[test]
    fn an_exhausted_anomaly_an_opponents_one_in_hand_or_without_runes_adds_nothing() {
        let mut spent = with_anomaly(fixtures::BASE, 0, true);
        let ctx = spent.ctx();
        assert_eq!(most_it_can_add(&ctx, 0, ANOMALY), 0);
        assert_eq!(adds_while_paying(&ctx, 0, ANOMALY, 1), None);
        drop(ctx);
        let mut theirs = with_anomaly(fixtures::BASE, 1, false);
        let ctx = theirs.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, ANOMALY, 1), None);
        assert_eq!(
            most_it_can_add(&ctx, 1, ANOMALY),
            2,
            "the other seat's two runes"
        );
        drop(ctx);
        let mut held = with_anomaly(fixtures::HAND, 0, false);
        let ctx = held.ctx();
        assert_eq!(adds_while_paying(&ctx, 0, ANOMALY, 1), None);
        drop(ctx);
        let mut dry = with_anomaly(fixtures::BASE, 0, false);
        dry.table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner != 0);
        dry.resolve();
        let ctx = dry.ctx();
        assert_eq!(most_it_can_add(&ctx, 0, ANOMALY), 0);
        assert_eq!(adds_while_paying(&ctx, 0, ANOMALY, 1), None);
    }

    #[test]
    fn a_ready_anomaly_turns_rainbow_into_energy_at_the_pay_stage_and_pays_by_exhausting() {
        let cost = energy(7);
        let mut spent = with_anomaly(fixtures::BASE, 0, true);
        let ctx = spent.ctx();
        assert!(
            pay::plan(&ctx, 0, &cost).is_err(),
            "three ready runes exhaust for three energy alone"
        );
        drop(ctx);
        let mut fixture = with_anomaly(fixtures::BASE, 0, false);
        let mut ctx = fixture.ctx();
        let planned = pay::plan(&ctx, 0, &cost)
            .expect("all four runes recycled for rainbow through the Anomaly add four more");
        pay::pay(&mut ctx, 0, &planned);
        assert!(
            ctx.card(ANOMALY).unwrap().exhausted,
            "the exhaust is the cost"
        );
        assert!(ctx.on_board(ANOMALY), "unlike a Gold, the Anomaly stays");
        assert_eq!(adds_while_paying(&ctx, 0, ANOMALY, 1), None);
    }
}
