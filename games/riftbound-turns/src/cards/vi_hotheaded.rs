use super::prelude::{activated, done, might_this_turn, named, paying_with, unit};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, SelfCost, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const COST: Cost = Cost {
    energy: 2,
    power: &[Power::Domain(Domain::Fury)],
};

pub fn doubling_of(ctx: &Ctx, unit: u32) -> i16 {
    i16::try_from(ctx.current_might(unit).max(0)).unwrap_or(i16::MAX)
}

fn vault_breaker(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    let current = ctx.current_might(me);
    let delta = doubling_of(ctx, me);
    might_this_turn(ctx, item, me, delta, None);
    ctx.narrate(format!(
        "{{card {me}}} doubles to {} might this turn",
        current + i32::from(delta)
    ));
    done()
}

pub static CARD: Card = unit(
    "Vi - Hotheaded",
    &[Keyword::Deflect(1)],
    &[named(
        paying_with(
            activated(Timing::Sorcery, COST, &[], vault_breaker),
            SelfCost::Free,
        ),
        "double my Might this turn",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{Expiry, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const VI: u32 = 90;

    fn vi(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Fury".into()],
            ..fixtures::unit(VI, zone, 0, "Vi - Hotheaded", 3)
        }
    }

    fn her_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == VI)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn fury_runes_recycled(ctx: &Ctx) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Move {
                        card,
                        zone: fixtures::RUNE_DECK,
                        seat: 0,
                        ..
                    } if [fixtures::RUNE_A, 41, 43].contains(card)
                )
            })
            .count()
    }

    fn gym(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(vi(zone));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(VI).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_prints_deflect_and_one_paid_ability_that_does_not_exhaust_her() {
        assert!(std::ptr::eq(script_of("Vi - Hotheaded").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Deflect(1)]);
        assert_eq!(CARD.abilities.len(), 1);
        let punch = &CARD.abilities[0];
        assert_eq!(punch.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(punch.self_cost, SelfCost::Free);
        assert_eq!(punch.cost, Some(COST));
        assert!(punch.targets.is_empty());
        assert_eq!(punch.label, Some("double my Might this turn"));
        let mut fixture = gym(fixtures::BF1);
        let ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(VI), 1);
        assert_eq!(doubling_of(&ctx, VI), 3);
    }

    #[test]
    fn the_punch_pays_two_and_a_fury_leaves_her_ready_and_adds_her_current_might_once_for_the_turn()
    {
        let mut fixture = gym(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            her_offers(&ctx),
            [(
                format!("{{card {VI}}}: double my Might this turn (2 energy and 1 Fury power)"),
                true
            )]
        );
        ctx.grant(VI, Keyword::Assault(2), Expiry::Permanent);
        ctx.mark_attacker(VI);
        assert_eq!(ctx.current_might(VI), 5, "3 + Assault 2 while attacking");
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, VI, 0).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target to ask for");
        assert!(!ctx.card(VI).unwrap().exhausted, "no exhaust in the cost");
        assert_eq!(fury_runes_recycled(&ctx), 1, "{:?}", ctx.effects);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 2,
            "two runes exhaust for the energy, the spent Fury recycles for the power"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == VI
        ));
        assert_eq!(ctx.current_might(VI), 5, "nothing until it resolves");
        let pump = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &pump, VI, 1, None);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.current_might(VI),
            12,
            "432.1.a · the current 6 as it resolves is added once, the reaction's +1 included"
        );
        assert_eq!(might_counter(&ctx, VI), 7);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {VI}}} doubles to 12 might this turn")));
        ctx.clear_designation(VI);
        assert_eq!(
            ctx.current_might(VI),
            10,
            "432.1.a · after combat the Assault falls away but the +6 stays"
        );
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(VI), 3, "the doubling ends with the turn");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_second_punch_doubles_again_and_a_vi_shrunk_to_nothing_doubles_by_zero() {
        let mut fixture = gym(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(48, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, VI, 0).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(VI), 6);
        activate::activate(&mut ctx, 0, VI, 0).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.current_might(VI),
            12,
            "each double reads the current value"
        );
        assert_eq!(might_counter(&ctx, VI), 9);
        drop(ctx);

        let mut shrunk = gym(fixtures::BASE);
        let mut ctx = shrunk.ctx();
        activate::activate(&mut ctx, 0, VI, 0).unwrap();
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        let eclipse = Item::new(
            9,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            Origin::Hand,
        );
        might_this_turn(&mut ctx, &eclipse, VI, -5, None);
        assert_eq!(ctx.current_might(VI), 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.current_might(VI),
            0,
            "477.3.c · a negative current Might is increased by 0"
        );
        assert_eq!(might_counter(&ctx, VI), -5);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {VI}}} doubles to 0 might this turn")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn short_of_energy_or_fury_the_offer_is_greyed_and_the_activation_refused() {
        let mut poor = gym(fixtures::BASE);
        for rune in [fixtures::RUNE_A, 41, 43] {
            poor.table.card_mut(rune).unwrap().domain = vec!["Calm".into()];
        }
        poor.resolve();
        let mut ctx = poor.ctx();
        assert_eq!(
            her_offers(&ctx),
            [(
                format!("{{card {VI}}}: double my Might this turn (2 energy and 1 Fury power)"),
                false
            )]
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, VI, 0),
            Err(Refusal::NoPowerOf)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, VI, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
        drop(ctx);

        let mut short = gym(fixtures::BASE);
        short.table.card_mut(42).unwrap().exhausted = true;
        short.table.card_mut(43).unwrap().exhausted = true;
        short.resolve();
        let mut ctx = short.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, VI, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 2,
                ready: 1
            })
        );
        assert_eq!(ctx.current_might(VI), 3);
    }
}
