use super::prelude::{
    a_battlefield, asking, deal, done, enemy_units, play, spell, with_candidates, zone_target,
    Location,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::pay;
use crate::state::TargetRef;

pub fn rainbow_sources(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut sources: Vec<u32> = ctx.runes_of(seat).into_iter().map(|rune| rune.id).collect();
    sources.extend(pay::ready_golds(ctx, seat));
    sources
}

fn one_more_rainbow(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 == 0 {
        return Vec::new();
    }
    rainbow_sources(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn pay_one_rainbow(ctx: &mut Ctx, seat: u8, source: u32) -> bool {
    if !rainbow_sources(ctx, seat).contains(&source) {
        return false;
    }
    if ctx.is_rune(source) {
        ctx.recycle_to_bottom(source);
        ctx.narrate(format!("{{card {source}}} is recycled for 1 power"));
    } else {
        ctx.exhaust(source);
        ctx.kill(source, Cause::Cost);
        ctx.narrate(format!("{{card {source}}} pays 1 power"));
    }
    true
}

pub fn enemy_units_at(ctx: &Ctx, seat: u8, zone: u16) -> Vec<u32> {
    let at = Location::Battlefield(zone);
    enemy_units(ctx, seat)
        .into_iter()
        .filter(|unit| ctx.location(*unit) == Some(at))
        .collect()
}

fn fire(ctx: &mut Ctx, item: &Item, zone: u16, amount: u8) -> Flow {
    let me = item.kind.source();
    if amount == 0 {
        ctx.narrate(format!("{{card {me}}} · nothing paid, nothing dealt"));
        return done();
    }
    ctx.narrate(format!(
        "{{card {me}}} deals {amount} to every enemy unit at {{zone {zone}}}"
    ));
    for unit in enemy_units_at(ctx, item.controller, zone) {
        if deal(ctx, item, unit, amount) {
            ctx.narrate(format!("{{card {unit}}} takes {amount}"));
        }
    }
    done()
}

fn bullet_time(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(zone) = zone_target(item, 0) else {
        return done();
    };
    let seat = item.controller;
    let answered = stage.0;
    let mut paid = answered.saturating_sub(1);
    if answered > 0 {
        let picked = ctx.picks().first().copied();
        match picked {
            Some(source) if pay_one_rainbow(ctx, seat, source) => paid = answered,
            _ => return fire(ctx, item, zone, paid),
        }
    }
    if rainbow_sources(ctx, seat).is_empty() {
        return fire(ctx, item, zone, paid);
    }
    ctx.narrate(format!(
        "{{card {}}} · {paid} paid so far",
        item.kind.source()
    ));
    Flow::Ask(ctx.ask_resume(item, paid + 1, 0, 1))
}

pub static CARD: Card = spell(
    "Bullet Time",
    &[Keyword::Action],
    &[asking(
        with_candidates(
            play(&[a_battlefield("a battlefield")], bullet_time),
            one_more_rainbow,
        ),
        "a rune to recycle, or a Gold to kill, for one more damage",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts};
    use crate::state::PromptWhy;
    use agni_plugin_sdk::table::CardInfo;

    const BULLET_TIME: u32 = 90;
    const RAIDER: u32 = 91;
    const GOLD: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Body".into(), "Chaos".into()],
            ..fixtures::spell(BULLET_TIME, fixtures::HAND, 0, "Bullet Time", 1, 0)
        });
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF2, 1, "Raider", 2));
        fixture.table.cards.push(fixtures::gold(GOLD, 0, false));
        fixture.resolve();
        fixture
    }

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt {
                    card,
                    n,
                    source: Cause::Item(1),
                } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    fn cast_at(ctx: &mut Ctx, zone: u16) {
        fixtures::play_from_hand(ctx, 0, BULLET_TIME).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{zone {zone}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn bullet_time_is_an_action_that_chooses_a_battlefield_and_pays_as_it_resolves() {
        assert!(std::ptr::eq(script_of("Bullet Time").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(CARD.abilities[0].targets[0].kind, TargetKind::Zone);
        assert!(CARD.abilities[0].candidates.is_some());
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BULLET_TIME).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "the battlefields, not the bases"
        );
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "one energy for the play itself, the rest is paid on resolution"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 1 }));
        let prompt = ctx.blob.prompt.as_ref().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 40}",
                "{card 41}",
                "{card 42}",
                "{card 43}",
                "{card 92}",
                "skip"
            ],
            "204.3.b · every rune in the pool, spent or ready, and the ready Gold"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item: 1, stage: 1 }),
            "{card 90}: choose a rune to recycle, or a Gold to kill, for one more damage (0 of 1)"
        );
    }

    #[test]
    fn three_runes_and_a_gold_deal_four_to_every_enemy_unit_at_the_battlefield() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, fixtures::BF2);
        fixtures::choose(&mut ctx, 0, "{card 40}").unwrap();
        assert_eq!(ctx.card(40).unwrap().zone, Some(fixtures::RUNE_DECK));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 2 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 41}", "{card 42}", "{card 43}", "{card 92}", "skip"],
            "the recycled rune is gone from the offer"
        );
        fixtures::choose(&mut ctx, 0, "{card 42}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(!ctx.on_board(GOLD), "the Gold is killed for its power");
        fixtures::choose(&mut ctx, 0, "{card 43}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 1, stage: 5 }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} · 4 paid so far".to_string()));
        assert!(
            damage_events(&ctx).is_empty(),
            "nothing lands before the payment ends"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            damage_events(&ctx),
            [(fixtures::SPRITE, 4), (RAIDER, 4)],
            "every enemy unit at the chosen battlefield"
        );
        assert_eq!(
            ctx.damage_on(fixtures::VI),
            0,
            "the friendly unit there is spared"
        );
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(!ctx.on_board(RAIDER));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BULLET_TIME).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn paying_nothing_deals_nothing_and_an_empty_pool_asks_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, fixtures::BF2);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(damage_events(&ctx).is_empty());
        assert!(ctx.on_board(fixtures::SPRITE));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} · nothing paid, nothing dealt".to_string()));
        assert_eq!(ctx.card(BULLET_TIME).unwrap().zone, Some(fixtures::TRASH));
        let mut bare = armed();
        bare.table
            .cards
            .retain(|card| ![40, 42, 43, GOLD].contains(&card.id));
        bare.resolve();
        let mut ctx = bare.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BULLET_TIME).unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        assert!(
            ctx.card(41).unwrap().exhausted,
            "the last rune paid the play"
        );
        ctx.recycle_to_bottom(41);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "with nothing in the pool there is nothing to offer"
        );
        assert!(damage_events(&ctx).is_empty());
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_battlefield_with_no_enemy_still_takes_the_payment_and_the_pool_is_read_live() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, fixtures::BF1);
        assert!(
            ctx.card(41).unwrap().exhausted,
            "the play exhausted it, and a spent rune still recycles"
        );
        fixtures::choose(&mut ctx, 0, "{card 41}").unwrap();
        for rune in [42, 43] {
            assert_eq!(ctx.card(rune).unwrap().zone, Some(fixtures::RUNE_POOL));
        }
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(
            damage_events(&ctx).is_empty(),
            "no enemy unit at that battlefield"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} deals 1 to every enemy unit at {zone 9}".to_string()));
        assert_eq!(ctx.card(41).unwrap().zone, Some(fixtures::RUNE_DECK));
    }
}

#[cfg(test)]
mod fidelity_probe {
    use super::*;
    use crate::engine::fixtures::{self, Fixture};
    use agni_plugin_sdk::table::CardInfo;

    const BULLET_TIME: u32 = 90;
    const RAIDER: u32 = 91;

    #[test]
    fn a_possessed_unit_is_no_longer_an_enemy_of_its_new_controller() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Body".into(), "Chaos".into()],
            ..fixtures::spell(BULLET_TIME, fixtures::HAND, 0, "Bullet Time", 1, 0)
        });
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF2, 1, "Raider", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(enemy_units(&ctx, 0).contains(&RAIDER));
        assert!(ctx.set_controller(RAIDER, 0, BULLET_TIME));
        assert_eq!(ctx.controller(RAIDER), 0);
        assert!(
            !enemy_units(&ctx, 0).contains(&RAIDER),
            "the possessed unit is friendly to seat 0 now"
        );
        assert!(
            enemy_units(&ctx, 1).contains(&RAIDER),
            "and an enemy of the seat that owns it"
        );
    }
}
