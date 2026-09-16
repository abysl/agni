use super::prelude::{buff, card_targets, done, play, spawn_gold, target, unit, FRIENDLY_UNIT};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const BUFFS: u8 = 4;
pub const GOLD_ARRIVES_READY: bool = false;
pub const UNITS_TO_BUFF: TargetSpec = target(
    FRIENDLY_UNIT,
    0,
    BUFFS,
    TargetKind::Card,
    "up to four friendly units to buff",
);

pub fn you_spent_a_buff(ctx: &Ctx, unit: u32, seat: u8) -> bool {
    ctx.is_unit(unit) && ctx.on_board(unit) && ctx.controller(unit) == seat
}

pub fn mint_gold(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if let Some(gold) = spawn_gold(ctx, seat, GOLD_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{card {}}} · {{card {gold}}} arrives exhausted",
            item.kind.source()
        ));
    }
    done()
}

fn blessing(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for unit in card_targets(ctx, item) {
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        }
    }
    done()
}

pub static CARD: Card = unit("Fae Dragon", &[], &[play(&[UNITS_TO_BUFF], blessing)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sett_brawler::spend_buff;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const DRAGON: u32 = 90;
    const ALLIES: [u32; 4] = [91, 92, 93, 94];

    fn dragon(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Body".into()],
            ..fixtures::unit(DRAGON, zone, 0, "Fae Dragon", 7)
        }
    }

    fn glade(dragon_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dragon(dragon_zone));
        for (index, ally) in ALLIES.iter().enumerate() {
            let zone = if index % 2 == 0 {
                fixtures::BASE
            } else {
                fixtures::BF1
            };
            fixture
                .table
                .cards
                .push(fixtures::unit(*ally, zone, 0, "Sprite", 2));
        }
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(ALLIES[0]),
            counter: COUNTER_BUFFED,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn buffs(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    fn golds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == "Gold" && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_play_trigger_over_up_to_four_friendly_units_and_the_spend_trigger_is_a_seam()
    {
        assert!(std::ptr::eq(script_of("Fae Dragon").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert_eq!(ability.targets, &[UNITS_TO_BUFF]);
        assert_eq!((UNITS_TO_BUFF.min, UNITS_TO_BUFF.max), (0, BUFFS));
        assert_eq!(UNITS_TO_BUFF.filter, FRIENDLY_UNIT);
    }

    #[test]
    fn playing_it_buffs_the_chosen_friendly_units_once_each_when_the_trigger_resolves() {
        let mut fixture = glade(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DRAGON).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        assert!(ctx.on_board(DRAGON));
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        let offered = fixtures::labels(&ctx);
        assert!(
            offered.contains(&format!("{{card {DRAGON}}}")),
            "itself included"
        );
        assert!(offered.contains(&format!("{{card {}}}", fixtures::VI)));
        for ally in ALLIES {
            assert!(offered.contains(&format!("{{card {ally}}}")));
        }
        assert!(
            !offered.contains(&format!("{{card {}}}", fixtures::THEIR_UNIT)),
            "friendly units only"
        );
        for ally in ALLIES {
            fixtures::choose(&mut ctx, 0, &format!("{{card {ally}}}")).unwrap();
        }
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DRAGON
        ));
        assert_eq!(buffs(&ctx, ALLIES[1]), 0, "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for ally in ALLIES {
            assert!(ctx.is_buffed(ally));
            assert_eq!(buffs(&ctx, ally), 1, "702.3 · one buff at a time");
            assert_eq!(ctx.current_might(ally), 3);
        }
        assert!(!ctx.is_buffed(DRAGON));
        assert!(!ctx.is_buffed(fixtures::VI));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| line.ends_with("is buffed"))
                .count(),
            3,
            "the already buffed ally gains nothing new"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn choosing_nothing_buffs_nothing_and_a_fifth_unit_cannot_be_chosen() {
        let mut fixture = glade(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DRAGON).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for ally in &ALLIES[1..] {
            assert!(!ctx.is_buffed(*ally));
        }
        drop(ctx);

        let mut fixture = glade(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DRAGON).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        for ally in ALLIES {
            fixtures::choose(&mut ctx, 0, &format!("{{card {ally}}}")).unwrap();
        }
        assert!(
            ctx.blob.prompt.is_none(),
            "four picks close the prompt: {:?}",
            fixtures::labels(&ctx)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_spend_condition_reads_a_buff_spent_off_a_unit_of_yours() {
        let mut fixture = glade(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(you_spent_a_buff(&ctx, ALLIES[0], 0));
        assert!(
            !you_spent_a_buff(&ctx, ALLIES[0], 1),
            "the opponent's spend is not yours"
        );
        assert!(!you_spent_a_buff(&ctx, fixtures::THEIR_UNIT, 0));
        assert!(
            !you_spent_a_buff(&ctx, fixtures::HAND_UNIT, 0),
            "not on the board"
        );
    }

    #[test]
    fn the_effect_plays_an_exhausted_gold_to_its_controllers_base() {
        let mut fixture = glade(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let item = Item::new(
            7,
            ItemKind::Trigger {
                source: DRAGON,
                index: 1,
            },
            0,
            Origin::Board,
        );
        assert_eq!(mint_gold(&mut ctx, &item, Stage(0)), Flow::Done);
        let golds = golds_of(&ctx, 0);
        assert_eq!(golds.len(), 1);
        let gold = ctx.card(golds[0]).unwrap();
        assert!(gold.exhausted, "played exhausted");
        assert_eq!(gold.zone, Some(fixtures::BASE));
        assert!(ctx.is_token(golds[0]));
        assert!(golds_of(&ctx, 1).is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
    }

    #[test]
    fn today_spending_a_buff_queues_no_gold() {
        let mut fixture = glade(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(spend_buff(&mut ctx, ALLIES[0]));
        settle(&mut ctx).unwrap();
        assert!(!ctx.is_buffed(ALLIES[0]));
        assert!(
            ctx.blob.chain.is_empty(),
            "no trigger reaches the chain yet"
        );
        assert!(golds_of(&ctx, 0).is_empty());
    }

    #[test]
    #[ignore = "engine gap · spend a buff: Ctx::spend_buff raises no BuffSpent event and Trigger has no BuffSpent(Who::You); with it the script adds triggered(BuffSpent(You), &[], mint_gold) and every buff you spend plays a Gold exhausted"]
    fn spending_a_buff_plays_an_exhausted_gold_when_the_trigger_resolves() {
        let mut fixture = glade(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(spend_buff(&mut ctx, ALLIES[0]));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        let golds = golds_of(&ctx, 0);
        assert_eq!(golds.len(), 1);
        assert!(ctx.card(golds[0]).unwrap().exhausted);
    }
}
