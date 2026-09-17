use super::prelude::{
    a_card, card_target, disempower, done, is_empowered, kill, play, unit, when, ENEMY_GEAR,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event, Killed};

pub const RUNES_REQUIRED: usize = 7;

fn controls_enough_runes(ctx: &Ctx, _: &Event, source: Source) -> bool {
    ctx.runes_of(ctx.controller(source.card)).len() >= RUNES_REQUIRED
}

fn raid(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(gear) = card_target(ctx, item, 0) else {
        return done();
    };
    if is_empowered(ctx, gear) {
        disempower(ctx, gear);
        ctx.narrate(format!("{{card {gear}}} is disempowered"));
    } else if kill(ctx, item, gear) == Killed::Yes {
        ctx.narrate(format!("{{card {gear}}} dies"));
    }
    done()
}

pub static CARD: Card = unit(
    "Tomb-Raider Barbara",
    &[],
    &[when(
        play(&[a_card(ENEMY_GEAR, "an enemy gear")], raid),
        controls_enough_runes,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::ctx::{Location, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const BARBARA: u32 = 90;
    const RELIC: u32 = 92;
    const AMULET: u32 = 93;
    const THEIR_GOLD: u32 = 94;
    const SPARE_RUNES: [u32; 3] = [46, 47, 48];

    fn barbara(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            might: Some(4),
            domain: vec!["Calm".into()],
            ..fixtures::card(BARBARA, zone, seat, "Tomb-Raider Barbara", "Unit")
        }
    }

    fn digging() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(barbara(fixtures::HAND, 0));
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture
            .table
            .cards
            .push(fixtures::gear(RELIC, fixtures::BASE, 1, "Relic", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(AMULET, fixtures::BASE, 0, "Amulet", 2));
        fixture.resolve();
        fixture
    }

    fn with_a_gold() -> Fixture {
        let mut fixture = digging();
        fixture
            .table
            .cards
            .push(fixtures::gold(THEIR_GOLD, 1, false));
        fixture.table.tokens.push(THEIR_GOLD);
        fixture.resolve();
        fixture
    }

    fn empowered() -> Fixture {
        let mut fixture = digging();
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(RELIC),
            counter: COUNTER_EMPOWERED,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.resolve();
        fixture
    }

    fn dig(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, BARBARA, Origin::Hand, Some(Location::Base(0)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn open_item(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            why => panic!("Barbara opens a target prompt, got {why:?}"),
        }
    }

    fn choose(ctx: &mut Ctx, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        let prompt = ctx.blob.prompt.as_ref().map(|held| held.id).unwrap_or(0);
        let answered = prompts::answer(ctx, 0, Pick { prompt, option })?;
        if let Some(answered) = answered {
            let PromptWhy::Target { item, spec } = answered.why else {
                panic!("Barbara opens a target prompt, got {:?}", answered.why);
            };
            play_engine::choose_targets(ctx, item, spec, &answered.prompt.picked)?;
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
    }

    fn resolve(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn died(ctx: &Ctx, card: u32) -> bool {
        ctx.events
            .iter()
            .any(|event| matches!(event, Event::Died { card: dead, .. } if *dead == card))
    }

    #[test]
    fn the_script_is_one_conditional_play_trigger_that_chooses_an_enemy_gear() {
        assert_eq!(CARD.name, "Tomb-Raider Barbara");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(
            ability.condition.is_some(),
            "if you control 7 or more runes"
        );
        assert!(!ability.optional, "the choice is not a may");
        assert!(ability.cost.is_none());
        assert_eq!(ability.targets.len(), 1);
        let spec = ability.targets[0];
        assert_eq!((spec.min, spec.max), (1, 1));
        assert_eq!(spec.kind, TargetKind::Card);
        assert_eq!(spec.filter, ENEMY_GEAR);
        assert_eq!(spec.label, "an enemy gear");
        assert_eq!(RUNES_REQUIRED, 7);
        let fixture = digging();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BARBARA).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn seven_runes_open_a_prompt_over_every_enemy_gear_and_the_chosen_one_dies() {
        let mut fixture = with_a_gold();
        let action = fixtures::move_action(BARBARA, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.runes_of(0).len(), RUNES_REQUIRED);
        dig(&mut ctx).unwrap();
        assert_eq!(ctx.location(BARBARA), Some(Location::Base(0)));
        assert!(ctx.card(BARBARA).unwrap().exhausted);
        assert_eq!(ctx.current_might(BARBARA), 4);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "four of the six ready runes paid the energy"
        );
        let item = open_item(&ctx);
        let prompt = ctx.blob.prompt.clone().expect("the choice is open");
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert!(!prompt.cancel, "a trigger's choice cannot be taken back");
        let offered = labels(&ctx);
        assert!(
            offered.contains(&format!("{{card {RELIC}}}")),
            "the enemy gear: {offered:?}"
        );
        assert!(
            offered.contains(&format!("{{card {THEIR_GOLD}}}")),
            "a Gold token is a gear too: {offered:?}"
        );
        assert!(
            !offered.contains(&format!("{{card {AMULET}}}")),
            "their own gear is out of reach: {offered:?}"
        );
        assert!(ctx.on_board(RELIC), "nothing until the item resolves");
        choose(&mut ctx, &format!("{{card {RELIC}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BARBARA
        ));
        assert_eq!(ctx.blob.chain[0].id, item);
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(RELIC)]);
        assert!(ctx.on_board(RELIC));
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(RELIC).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.effects.contains(&Effect::Move {
            card: RELIC,
            zone: fixtures::TRASH,
            seat: 1,
            index: TOP
        }));
        assert!(died(&ctx, RELIC));
        assert!(ctx.blob.log.contains(&format!("{{card {RELIC}}} dies")));
        assert!(ctx.on_board(THEIR_GOLD), "only the chosen gear");
        assert!(ctx.on_board(AMULET));
    }

    #[test]
    fn an_empowered_gear_is_disempowered_and_stays_on_the_board() {
        let mut fixture = empowered();
        let action = fixtures::move_action(BARBARA, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(ctx.is_empowered(RELIC));
        dig(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one candidate answers its own prompt"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(RELIC)]);
        resolve(&mut ctx);
        assert!(!ctx.is_empowered(RELIC));
        assert!(ctx.on_board(RELIC), "disempowered, not killed");
        assert_eq!(ctx.card(RELIC).unwrap().zone, Some(fixtures::BASE));
        assert!(!died(&ctx, RELIC));
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(RELIC),
            counter: COUNTER_EMPOWERED,
            delta: -1
        }));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {RELIC}}} is disempowered")));
    }

    #[test]
    fn six_runes_never_collect_the_trigger_at_all() {
        let mut fixture = digging();
        fixture.table.cards.retain(|card| card.id != SPARE_RUNES[2]);
        fixture.resolve();
        let action = fixtures::move_action(BARBARA, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.runes_of(0).len(), RUNES_REQUIRED - 1);
        dig(&mut ctx).unwrap();
        assert_eq!(ctx.location(BARBARA), Some(Location::Base(0)));
        assert!(ctx.blob.prompt.is_none(), "the condition failed");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.on_board(RELIC));
        assert!(!died(&ctx, RELIC));
    }

    #[test]
    fn a_board_with_no_enemy_gear_fizzles_the_trigger() {
        let mut fixture = digging();
        fixture.table.cards.retain(|card| card.id != RELIC);
        fixture.resolve();
        let action = fixtures::move_action(BARBARA, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        dig(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BARBARA}}} trigger fizzles · no legal target"
        )));
        assert!(ctx.on_board(AMULET), "her own side is never a target");
    }

    #[test]
    fn a_friendly_gear_is_refused_and_a_spent_rune_pool_refuses_the_play_itself() {
        let mut fixture = with_a_gold();
        let action = fixtures::move_action(BARBARA, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        dig(&mut ctx).unwrap();
        let item = open_item(&ctx);
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[AMULET]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "her own gear is no enemy gear"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.on_board(AMULET));
        assert!(ctx.on_board(RELIC));
        drop(ctx);
        let mut broke = digging();
        for rune in broke.table.cards.iter_mut() {
            if rune.owner == 0 && rune.is_kind("Rune") {
                rune.exhausted = true;
            }
        }
        broke.resolve();
        let action = fixtures::move_action(BARBARA, fixtures::BASE, 0);
        let mut ctx = broke.ctx_for(0, &action);
        assert_eq!(
            dig(&mut ctx),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 0
            })
        );
        assert!(ctx.blob.chain.is_empty(), "nothing triggered");
        assert!(ctx.on_board(RELIC));
        assert!(!died(&ctx, RELIC));
    }
}
