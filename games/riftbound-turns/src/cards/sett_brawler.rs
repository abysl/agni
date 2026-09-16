use super::prelude::{
    activated, buff, done, friendly_units, might_this_turn, named, on_conquer_me, paying_with,
    play, unit, usable_if,
};
use super::{Ability, Card, Cost, Flow, Item, SelfCost, Source, Stage, Timing};
use crate::engine::ctx::{Ctx, COUNTER_BUFFED};
use agni_plugin_sdk::decide::Effect;
use agni_plugin_sdk::table::Target;

pub const BRAWL_MIGHT: i16 = 4;
pub const PLAYED: u8 = 0;
pub const CONQUERED: u8 = 1;
pub const BRAWL: u8 = 2;

pub fn spend_buff(ctx: &mut Ctx, unit: u32) -> bool {
    if !ctx.is_unit(unit) || !ctx.on_board(unit) || !ctx.is_buffed(unit) {
        return false;
    }
    ctx.emit(Effect::Counter {
        target: Target::Card(unit),
        counter: COUNTER_BUFFED,
        delta: -1,
    });
    ctx.narrate(format!("{{card {unit}}}'s buff is spent"));
    true
}

pub fn buffed_friendly_units(ctx: &Ctx, seat: u8) -> Vec<u32> {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| ctx.is_buffed(*unit))
        .collect()
}

pub fn can_spend_my_buff(ctx: &Ctx, source: Source) -> bool {
    ctx.is_buffed(source.card)
}

pub const fn spending_my_buff(ability: Ability) -> Ability {
    usable_if(paying_with(ability, SelfCost::Free), can_spend_my_buff)
}

pub fn buff_me(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    }
    done()
}

fn brawl(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if spend_buff(ctx, me) {
        might_this_turn(ctx, item, me, BRAWL_MIGHT, None);
        ctx.narrate(format!("{{card {me}}} gets +{BRAWL_MIGHT} might this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Sett - Brawler",
    &[],
    &[
        play(&[], buff_me),
        on_conquer_me(&[], buff_me),
        named(
            spending_my_buff(activated(Timing::Sorcery, Cost::FREE, &[], brawl)),
            "spend my buff: +4 might this turn",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, priority, settle};
    use crate::state::{Expiry, ItemKind};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo};

    const SETT: u32 = 90;

    fn sett(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(5),
            power: Some(1),
            domain: vec!["Body".into()],
            ..fixtures::unit(SETT, zone, 0, "Sett - Brawler", 4)
        }
    }

    fn buffed(fixture: &mut Fixture, unit: u32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(unit),
            counter: COUNTER_BUFFED,
            value: 1,
        });
        fixture.table.counters.sort();
    }

    fn pit(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sett(zone));
        for id in [fixtures::RUNE_A, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Body", false));
        fixture.resolve();
        fixture
    }

    fn buffs(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn his_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == SETT)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    #[test]
    fn the_script_buffs_him_on_play_and_on_conquer_and_spends_the_buff_by_a_free_activation() {
        assert!(std::ptr::eq(script_of("Sett - Brawler").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 3);
        assert_eq!(CARD.abilities[usize::from(PLAYED)].trigger, Trigger::Play);
        assert_eq!(
            CARD.abilities[usize::from(CONQUERED)].trigger,
            Trigger::Conquer(Who::Me)
        );
        let brawl = &CARD.abilities[usize::from(BRAWL)];
        assert_eq!(brawl.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(brawl.cost, Some(Cost::FREE));
        assert_eq!(
            brawl.self_cost,
            SelfCost::Free,
            "the buff, not an exhaust, is the cost"
        );
        assert!(brawl.usable.is_some());
        assert!(brawl.targets.is_empty());
        assert_eq!(brawl.label, Some("spend my buff: +4 might this turn"));
        assert_eq!(BRAWL_MIGHT, 4);
    }

    #[test]
    fn spend_buff_takes_the_counter_off_a_buffed_unit_on_the_board_and_refuses_everything_else() {
        let mut fixture = pit(fixtures::BASE);
        buffed(&mut fixture, SETT);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(SETT), 5);
        assert!(
            !spend_buff(&mut ctx, fixtures::VI),
            "702.2.b.1 · no buff to spend"
        );
        assert!(
            !spend_buff(&mut ctx, fixtures::HAND_UNIT),
            "a unit in hand has no buff"
        );
        assert!(spend_buff(&mut ctx, SETT));
        assert!(!ctx.is_buffed(SETT));
        assert_eq!(buffs(&ctx, SETT), 0);
        assert_eq!(
            ctx.current_might(SETT),
            4,
            "703 · the +1 leaves with the buff"
        );
        assert_eq!(
            ctx.effects,
            [Effect::Counter {
                target: Target::Card(SETT),
                counter: COUNTER_BUFFED,
                delta: -1
            }]
        );
        assert!(
            !spend_buff(&mut ctx, SETT),
            "702.2.b · one counter per spend"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SETT}}}'s buff is spent")));
        assert_eq!(buffed_friendly_units(&ctx, 0), Vec::<u32>::new());
        ctx.buff(fixtures::VI);
        ctx.buff(fixtures::THEIR_UNIT);
        assert_eq!(buffed_friendly_units(&ctx, 0), [fixtures::VI]);
        assert_eq!(buffed_friendly_units(&ctx, 1), [fixtures::THEIR_UNIT]);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_from_hand_his_trigger_buffs_him_when_it_resolves() {
        let mut fixture = pit(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SETT).unwrap();
        assert!(ctx.on_board(SETT));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == SETT && index == PLAYED
        ));
        assert!(!ctx.is_buffed(SETT), "the buff waits for the chain");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(SETT));
        assert_eq!(ctx.current_might(SETT), 5);
        assert!(ctx.blob.log.contains(&format!("{{card {SETT}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn conquering_with_him_buffs_him_and_a_conquer_without_him_does_not() {
        let mut fixture = pit(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == SETT && index == CONQUERED
        ));
        resolve_chain(&mut ctx);
        assert!(ctx.is_buffed(SETT));
        drop(ctx);
        let mut fixture = pit(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vi conquered; he stayed home");
        assert!(!ctx.is_buffed(SETT));
    }

    #[test]
    fn while_buffed_the_brawl_is_offered_free_and_spends_the_buff_for_four_might_this_turn() {
        let mut fixture = pit(fixtures::BASE);
        buffed(&mut fixture, SETT);
        let mut ctx = fixture.ctx();
        assert_eq!(
            his_offers(&ctx),
            [(
                format!("{{card {SETT}}}: spend my buff: +4 might this turn"),
                true
            )],
            "no rune, no exhaust: the buff is the price"
        );
        activate::activate(&mut ctx, 0, SETT, BRAWL).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.card(SETT).unwrap().exhausted);
        assert!(
            ctx.is_buffed(SETT),
            "the buff leaves as the ability resolves"
        );
        assert_eq!(ctx.current_might(SETT), 5);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(SETT));
        assert_eq!(ctx.current_might(SETT), 8, "4 printed + 4, the buff gone");
        assert_eq!(might_counter(&ctx, SETT), 4);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SETT}}} gets +4 might this turn")));
        assert_eq!(
            activate::activate(&mut ctx, 0, SETT, BRAWL),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "702.2.b.1 · no buff left to spend"
        );
        assert!(his_offers(&ctx).is_empty());
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(SETT), 4);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_second_brawl_in_response_finds_no_buff_and_gives_nothing() {
        let mut fixture = pit(fixtures::BASE);
        buffed(&mut fixture, SETT);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, SETT, BRAWL).unwrap();
        assert_eq!(
            activate::activate(&mut ctx, 0, SETT, BRAWL),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "381 · a sorcery-speed ability needs an open state"
        );
        let item = ctx.blob.chain[0].clone();
        assert_eq!(brawl(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(ctx.current_might(SETT), 8);
        assert_eq!(
            brawl(&mut ctx, &item, Stage(0)),
            Flow::Done,
            "the same resolution again spends nothing"
        );
        assert_eq!(ctx.current_might(SETT), 8, "one buff, one +4");
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Readied { .. }))
                .count(),
            0
        );
    }

    #[test]
    fn an_unbuffed_or_enemy_sett_cannot_brawl() {
        let mut fixture = pit(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(his_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, SETT, BRAWL),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(!can_spend_my_buff(
            &ctx,
            Source {
                card: SETT,
                ability: BRAWL
            }
        ));
        assert_eq!(
            activate::activate(&mut ctx, 1, SETT, BRAWL),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
    }

    #[test]
    #[ignore = "engine gap · spend a buff: SelfCost::SpendBuff pays the buff at activation (204.1.b), not as the ability resolves; until then usable_if gates the activation and the run spends it"]
    fn the_buff_leaves_at_activation_before_anyone_can_respond() {
        let mut fixture = pit(fixtures::BASE);
        buffed(&mut fixture, SETT);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, SETT, BRAWL).unwrap();
        assert!(!ctx.is_buffed(SETT), "paid on the way onto the chain");
        assert_eq!(ctx.current_might(SETT), 4);
    }
}
