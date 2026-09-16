use crate::cards::prelude::{asking, triggered, with_candidates};
use crate::cards::{Ability, Flow, Item, Replacement, Source, Stage, Trigger, WouldDie};
use crate::engine::ctx::{Cause, Ctx, Event, Killed, Trashed};
use crate::engine::triggers::{self, Match};
use crate::state::{ChainItem, Death, ItemKind, ItemStatus, Noted, Origin, Phase, TargetRef};

pub const CHOICE: u8 = u8::MAX - 1;
const ASK: u8 = 0;
const PICK: u8 = 1;

struct Planned {
    card: u32,
    noted: Noted,
    deathknells: Vec<Match>,
}

fn plan(ctx: &Ctx, card: u32) -> Planned {
    let noted = ctx.note(card);
    Planned {
        card,
        deathknells: triggers::deathknells(ctx, card, noted),
        noted,
    }
}

pub fn kill(ctx: &mut Ctx, card: u32, cause: Cause) -> Killed {
    if !ctx.on_board(card) {
        return Killed::NotOnBoard;
    }
    let planned = plan(ctx, card);
    run(ctx, planned, cause)
}

pub fn batch(ctx: &mut Ctx, cards: &[u32], cause: Cause) -> Vec<u32> {
    let planned: Vec<(Planned, Vec<Source>)> = cards
        .iter()
        .filter(|card| ctx.on_board(**card))
        .map(|card| {
            let would = WouldDie { unit: *card, cause };
            (plan(ctx, *card), applicable(ctx, &would))
        })
        .collect();
    let (replaced, unmodified): (Vec<_>, Vec<_>) = planned
        .into_iter()
        .partition(|(_, applicable)| !applicable.is_empty());
    let mut dead = Vec::new();
    for (held, mut applicable) in replaced.into_iter().chain(unmodified) {
        let card = held.card;
        applicable.retain(|source| still_applies(ctx, source.card, cards));
        if run_with(ctx, held, cause, applicable) == Killed::Yes {
            dead.push(card);
        }
    }
    dead
}

fn still_applies(ctx: &Ctx, source: u32, batch: &[u32]) -> bool {
    ctx.on_board(source) || (batch.contains(&source) && !ctx.in_trash(source))
}

fn run(ctx: &mut Ctx, planned: Planned, cause: Cause) -> Killed {
    let would = WouldDie {
        unit: planned.card,
        cause,
    };
    let applicable = applicable(ctx, &would);
    run_with(ctx, planned, cause, applicable)
}

fn run_with(ctx: &mut Ctx, planned: Planned, cause: Cause, applicable: Vec<Source>) -> Killed {
    let card = planned.card;
    if !ctx.on_board(card) {
        return Killed::NotOnBoard;
    }
    if is_parked(ctx, card) {
        return Killed::Replaced;
    }
    let would = WouldDie { unit: card, cause };
    if applicable.len() > 1 && !mid_combat(ctx, &would) {
        park(ctx, &would, &applicable);
        return Killed::Replaced;
    }
    if let Some(source) = preferred(ctx, &would, &applicable) {
        if applicable.len() > 1 {
            ctx.narrate(format!(
                "{{card {card}}} would die mid-combat · {{card {}}} is taken without asking",
                source.card
            ));
        }
        replace(ctx, &would, source);
        return Killed::Replaced;
    }
    die(ctx, planned)
}

fn mid_combat(ctx: &Ctx, would: &WouldDie) -> bool {
    ctx.in_combat(would.unit) && matches!(would.cause, Cause::Cleanup { .. })
}

fn preferred(ctx: &Ctx, would: &WouldDie, applicable: &[Source]) -> Option<Source> {
    let owner = ctx.controller(would.unit);
    applicable
        .iter()
        .find(|source| ctx.controller(source.card) == owner)
        .or_else(|| applicable.first())
        .copied()
}

fn die(ctx: &mut Ctx, planned: Planned) -> Killed {
    let Planned {
        card,
        noted,
        deathknells,
    } = planned;
    let controller = noted.controller;
    let unit = ctx.is_unit(card);
    if let Trashed::Banished { by } = ctx.trash(card) {
        ctx.narrate(format!(
            "{{card {card}}} is killed but never reaches the trash · no death for {{card {by}}}"
        ));
        return Killed::Yes;
    }
    ctx.deaths.extend(deathknells);
    ctx.blob.deaths_this_turn.push(Death {
        card,
        controller,
        unit,
        phase: ctx.blob.phase().unwrap_or(Phase::Setup),
    });
    ctx.raise(Event::Died {
        card,
        controller,
        unit,
        noted,
    });
    Killed::Yes
}

pub fn applicable(ctx: &Ctx, would: &WouldDie) -> Vec<Source> {
    if matches!(would.cause, Cause::Replacement) {
        return Vec::new();
    }
    let mut sources: Vec<u32> = ctx
        .faces_on_board()
        .map(|card| card.id)
        .filter(|card| !ctx.is_facedown(*card))
        .collect();
    sources.sort_unstable();
    sources
        .into_iter()
        .filter_map(|card| {
            let replacement = ctx.script(card).and_then(|script| script.replacement)?;
            let source = Source { card, ability: 0 };
            (replacement.applies)(ctx, would, source).then_some(source)
        })
        .collect()
}

fn replacement_of(ctx: &Ctx, source: Source) -> Option<Replacement> {
    ctx.script(source.card)
        .and_then(|script| script.replacement)
}

fn replace(ctx: &mut Ctx, would: &WouldDie, source: Source) -> bool {
    let Some(replacement) = replacement_of(ctx, source) else {
        return false;
    };
    ctx.narrate(format!(
        "{{card {}}} replaces the death of {{card {}}}",
        source.card, would.unit
    ));
    (replacement.run)(ctx, would, source);
    true
}

pub fn is_choice(item: &ChainItem) -> bool {
    matches!(item.kind, ItemKind::Trigger { index, .. } if index == CHOICE)
}

pub fn is_parked(ctx: &Ctx, card: u32) -> bool {
    ctx.blob
        .chain
        .iter()
        .any(|item| is_choice(item) && item.kind.source() == card)
}

fn park(ctx: &mut Ctx, would: &WouldDie, applicable: &[Source]) {
    let owner = ctx.controller(would.unit);
    let id = ctx.blob.next_item_id();
    let mut item = ChainItem::new(
        id,
        ItemKind::Trigger {
            source: would.unit,
            index: CHOICE,
        },
        owner,
        Origin::Board,
    );
    item.status = ItemStatus::Resolving;
    item.stage = ASK;
    item.subject = Some(TargetRef::Card(would.unit));
    item.targets = applicable
        .iter()
        .map(|source| TargetRef::Card(source.card))
        .collect();
    item.picks = encode_cause(would.cause);
    ctx.blob.chain.push(item);
    ctx.narrate(format!(
        "{{seat {owner}}} chooses which replacement applies to {{card {}}}",
        would.unit
    ));
}

fn encode_cause(cause: Cause) -> Vec<u8> {
    match cause {
        Cause::Item(item) => {
            let mut bytes = vec![0];
            bytes.extend(item.to_le_bytes());
            bytes
        }
        Cause::Ability(source) => {
            let mut bytes = vec![1];
            bytes.extend(source.card.to_le_bytes());
            bytes.push(source.ability);
            bytes
        }
        Cause::Replacement => vec![2],
        Cause::Combat => vec![3],
        Cause::Cleanup { last_item: None } => vec![4],
        Cause::Cleanup {
            last_item: Some(item),
        } => {
            let mut bytes = vec![5];
            bytes.extend(item.to_le_bytes());
            bytes
        }
        Cause::Cost => vec![6],
        Cause::Rule => vec![7],
    }
}

fn decode_cause(bytes: &[u8]) -> Cause {
    let u16_at = |at: usize| Some(u16::from_le_bytes([*bytes.get(at)?, *bytes.get(at + 1)?]));
    match bytes.first() {
        Some(0) => u16_at(1).map(Cause::Item).unwrap_or(Cause::Rule),
        Some(1) => {
            let card = (|| {
                Some(u32::from_le_bytes([
                    *bytes.get(1)?,
                    *bytes.get(2)?,
                    *bytes.get(3)?,
                    *bytes.get(4)?,
                ]))
            })();
            match (card, bytes.get(5)) {
                (Some(card), Some(ability)) => Cause::Ability(Source {
                    card,
                    ability: *ability,
                }),
                _ => Cause::Rule,
            }
        }
        Some(2) => Cause::Replacement,
        Some(3) => Cause::Combat,
        Some(4) => Cause::Cleanup { last_item: None },
        Some(5) => Cause::Cleanup {
            last_item: u16_at(1),
        },
        Some(6) => Cause::Cost,
        _ => Cause::Rule,
    }
}

fn would_of(item: &Item) -> WouldDie {
    WouldDie {
        unit: item.kind.source(),
        cause: decode_cause(&item.picks),
    }
}

fn offered(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    applicable(ctx, &would_of(item))
        .into_iter()
        .map(|source| TargetRef::Card(source.card))
        .collect()
}

fn decide(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let would = would_of(item);
    let applicable = applicable(ctx, &would);
    if stage.0 == ASK && applicable.len() > 1 {
        return Flow::Ask(ctx.ask_resume(item, PICK, 1, 1));
    }
    let picked = ctx
        .picks()
        .first()
        .and_then(|card| applicable.iter().find(|source| source.card == *card))
        .or_else(|| applicable.first())
        .copied();
    match picked {
        Some(source) => {
            replace(ctx, &would, source);
        }
        None => {
            let unit = would.unit;
            if ctx.on_board(unit) {
                let planned = plan(ctx, unit);
                if die(ctx, planned) == Killed::Yes {
                    ctx.narrate(format!(
                        "no replacement is left for {{card {unit}}} · it dies"
                    ));
                }
            }
        }
    }
    Flow::Done
}

pub static CHOICE_ABILITY: Ability = asking(
    with_candidates(triggered(Trigger::Reflexive, &[], decide), offered),
    "which replacement applies",
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, deathknell, unit};
    use crate::cards::{Card, Flow, Keyword};
    use crate::engine::ctx::{Location, Token, COUNTER_BUFFED, COUNTER_DAMAGE, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, cleanup, combat, priority, prompts, settle, showdown};
    use crate::rules::COUNTER_TEMPORARY;
    use crate::state::{Expiry, ItemKind, Noted, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const HERO: u32 = 90;
    const GUARDED: u32 = 91;
    const HOURGLASS: u32 = 92;
    const KILLER: u32 = 93;
    const SECOND_GLASS: u32 = 94;

    static HERO_CARD: Card = unit(
        "Hero",
        &[Keyword::Deathknell],
        &[deathknell(&[], |ctx, item, _| {
            if prelude::was_mighty(item) {
                prelude::draw(ctx, item.controller, 2);
            }
            Flow::Done
        })],
    );

    static HOURGLASS_CARD: Card = prelude::with_replacement(
        prelude::gear("Hourglass", &[], &[]),
        prelude::replaces(
            |ctx, would, source| {
                ctx.is_unit(would.unit) && ctx.controller(would.unit) == ctx.controller(source.card)
            },
            |ctx, would, source| {
                ctx.kill(source.card, Cause::Replacement);
                ctx.recall(would.unit, true);
            },
        ),
    );

    fn body(id: u32, zone: u16, seat: u8, name: &str, might: u8) -> CardInfo {
        fixtures::unit(id, zone, seat, name, might)
    }

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
    }

    fn two_hourglasses() -> Fixture {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(body(GUARDED, fixtures::BF1, 0, "Hero", 3));
        fixture
            .table
            .cards
            .push(fixtures::gear(HOURGLASS, fixtures::BASE, 0, "Hourglass", 2));
        fixture.table.cards.push(fixtures::gear(
            SECOND_GLASS,
            fixtures::BASE,
            0,
            "Hourglass",
            2,
        ));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GUARDED, &HERO_CARD)
            .with_script(HOURGLASS, &HOURGLASS_CARD)
            .with_script(SECOND_GLASS, &HOURGLASS_CARD);
        fixture
    }

    #[test]
    fn a_replacement_sitting_facedown_at_a_battlefield_does_not_apply() {
        let mut fixture = two_hourglasses();
        fixture.table.cards.retain(|card| card.id != SECOND_GLASS);
        fixture.table.card_mut(HOURGLASS).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GUARDED, &HERO_CARD)
            .with_script(HOURGLASS, &HOURGLASS_CARD);
        fixture.blob.card_state_mut(HOURGLASS).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert!(ctx.is_facedown(HOURGLASS));
        assert!(
            applicable(
                &ctx,
                &WouldDie {
                    unit: GUARDED,
                    cause: Cause::Rule,
                }
            )
            .is_empty(),
            "408.3 · a facedown card's only properties are the ones the hiding effect grants"
        );
        assert_eq!(ctx.kill(GUARDED, Cause::Rule), Killed::Yes);
        assert_eq!(ctx.card(GUARDED).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.card(HOURGLASS).unwrap().zone,
            Some(fixtures::BF1),
            "and the gear is untouched in its facedown zone"
        );
        let facedown = crate::engine::hide::drop_facedown(&mut ctx, HOURGLASS);
        assert_eq!(facedown, Some(fixtures::BF1));
        assert_eq!(
            applicable(
                &ctx,
                &WouldDie {
                    unit: fixtures::VI,
                    cause: Cause::Rule,
                }
            ),
            [Source {
                card: HOURGLASS,
                ability: 0
            }],
            "revealed, the printed replacement is back"
        );
    }

    #[test]
    fn a_deathknell_reads_the_noted_snapshot_of_a_unit_whose_might_came_from_a_counter() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(body(HERO, fixtures::BASE, 0, "Hero", 2));
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(HERO),
            counter: COUNTER_BUFFED,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(HERO, &HERO_CARD);
        let mut ctx = fixture.ctx();
        ctx.might(HERO, 2, Expiry::Permanent, None, 0);
        assert_eq!(
            ctx.current_might(HERO),
            5,
            "2 printed + 1 buff + 2 counters"
        );
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(HERO, Cause::Rule), Killed::Yes);
        assert_eq!(
            ctx.card(HERO).unwrap().zone,
            Some(fixtures::TRASH),
            "the body is in the trash"
        );
        assert_eq!(
            ctx.table
                .counter(Target::Card(HERO), COUNTER_MIGHT)
                .unwrap_or(0),
            0,
            "the mirrors go with the card as it leaves play"
        );
        assert_eq!(
            ctx.table
                .counter(Target::Card(HERO), COUNTER_BUFFED)
                .unwrap_or(0),
            0
        );
        assert_eq!(
            ctx.deaths.len(),
            1,
            "the Deathknell is queued before the move"
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].noted,
            Some(Noted {
                zone: fixtures::BASE,
                might: 5,
                controller: 0,
                alone: false,
                buffed: true
            })
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + 2,
            "Mighty at death, read off the snapshot"
        );
    }

    #[test]
    fn a_deathknell_of_a_small_unit_sees_a_snapshot_that_is_not_mighty() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(body(HERO, fixtures::BASE, 0, "Hero", 2));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(HERO, &HERO_CARD);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(HERO, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand, "2 might is not Mighty");
    }

    #[test]
    fn a_replacement_stops_the_death_and_every_trigger_that_keys_on_it() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(body(GUARDED, fixtures::BF1, 0, "Hero", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(HOURGLASS, fixtures::BASE, 0, "Hourglass", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GUARDED, &HERO_CARD)
            .with_script(HOURGLASS, &HOURGLASS_CARD);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(GUARDED, Cause::Combat), Killed::Replaced);
        assert_eq!(
            ctx.location(GUARDED),
            Some(Location::Base(0)),
            "the unit is recalled instead"
        );
        assert!(ctx.card(GUARDED).unwrap().exhausted);
        assert_eq!(
            ctx.card(HOURGLASS).unwrap().zone,
            Some(fixtures::TRASH),
            "the gear kills itself"
        );
        assert!(ctx.deaths.is_empty(), "no Deathknell was queued");
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Died { card, .. } if *card == GUARDED)),
            "nothing keys on a death that never happened"
        );
        assert!(
            ctx.events
                .iter()
                .any(|event| matches!(event, Event::Died { card, .. } if *card == HOURGLASS)),
            "the gear itself really died"
        );
    }

    #[test]
    fn zhonyas_replaces_a_combat_death_and_the_deathknell_never_fires() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(body(GUARDED, fixtures::BF1, 0, "Hero", 3));
        fixture
            .table
            .cards
            .push(body(KILLER, fixtures::BF1, 1, "Jinx", 3));
        fixture
            .table
            .cards
            .push(fixtures::gear(HOURGLASS, fixtures::BASE, 0, "Hourglass", 2));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GUARDED, &HERO_CARD)
            .with_script(HOURGLASS, &HOURGLASS_CARD);
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cleanup::run(&mut ctx, None);
        assert!(
            ctx.blob.showdown.as_ref().is_some_and(|held| held.combat),
            "two seats' units at a contested battlefield stage a combat"
        );
        showdown::pass(&mut ctx, 1).unwrap();
        showdown::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.showdown.is_none(), "the combat resolved");
        assert_eq!(
            ctx.location(GUARDED),
            Some(Location::Base(0)),
            "the defender was recalled by the replacement"
        );
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.hand_of(0).len(), hand, "no Deathknell draw");
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn a_token_despawns_with_its_owner_and_a_card_leaves_its_counters_behind() {
        let mut fixture = arena();
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(fixtures::SPRITE),
            counter: COUNTER_DAMAGE,
            value: 2,
        });
        fixture.table.counters.sort();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.controller(fixtures::SPRITE), 1);
        assert_eq!(ctx.kill(fixtures::SPRITE, Cause::Rule), Killed::Yes);
        assert_eq!(
            ctx.effects,
            [Effect::Despawn {
                card: fixtures::SPRITE
            }],
            "a token vanishes: the host sheds its counters with it"
        );
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert_eq!(ctx.kill(fixtures::SPRITE, Cause::Rule), Killed::NotOnBoard);
        let mut plain = arena();
        plain.table.counters.push(CounterInfo {
            target: Target::Card(fixtures::VI),
            counter: COUNTER_DAMAGE,
            value: 3,
        });
        plain.table.counters.push(CounterInfo {
            target: Target::Card(fixtures::VI),
            counter: COUNTER_TEMPORARY,
            value: 1,
        });
        plain.table.counters.sort();
        let mut ctx = plain.ctx();
        assert_eq!(ctx.kill(fixtures::VI, Cause::Cost), Killed::Yes);
        assert_eq!(
            ctx.effects,
            [Effect::Move {
                card: fixtures::VI,
                zone: fixtures::TRASH,
                seat: 0,
                index: TOP
            }]
        );
        assert_eq!(
            ctx.damage_on(fixtures::VI),
            0,
            "the trash sheds every mirror the card carried"
        );
        assert_eq!(
            ctx.table
                .counter(Target::Card(fixtures::VI), COUNTER_TEMPORARY)
                .unwrap_or(0),
            0
        );
        assert!(ctx.blob.card_state(fixtures::VI).is_none());
    }

    #[test]
    fn a_spawned_token_keeps_its_owner_through_a_conquer_and_a_despawn() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.actor, 0, "the entry belongs to seat 0");
        let sprite = ctx
            .spawn(1, Token::Sprite, Location::Battlefield(fixtures::BF1), true)
            .unwrap();
        assert_eq!(ctx.controller(sprite), 1, "the spawn names its owner");
        assert!(ctx.is_token(sprite));
        assert!(
            ctx.is_temporary(sprite),
            "a Sprite carries the Temporary mirror"
        );
        assert_eq!(ctx.current_might(sprite), 3);
        assert!(!ctx.card(sprite).unwrap().exhausted);
        ctx.blob.set_holder(fixtures::BF1, None);
        ctx.blob.set_contested(fixtures::BF1, Some(1));
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(1)
        );
        assert_eq!(
            ctx.controller(sprite),
            1,
            "the conquer leaves the owner alone"
        );
        assert_eq!(ctx.points(1), 1);
        assert_eq!(ctx.kill(sprite, Cause::Rule), Killed::Yes);
        assert!(ctx.effects.contains(&Effect::Despawn { card: sprite }));
        assert!(
            ctx.card(sprite).is_none(),
            "a token despawns, it does not trash"
        );
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, controller: 1, .. } if *card == sprite)
        ));
    }

    #[test]
    fn a_kill_in_a_cleanup_and_a_kill_in_combat_walk_the_same_path() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(body(HERO, fixtures::BF1, 0, "Hero", 2));
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(HERO),
            counter: COUNTER_DAMAGE,
            value: 2,
        });
        fixture.table.counters.sort();
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(HERO, &HERO_CARD);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::dying(&ctx), [HERO]);
        cleanup::lethal_kills(&mut ctx);
        assert_eq!(ctx.card(HERO).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.deaths.len(),
            1,
            "the cleanup kill queues the Deathknell"
        );
        assert!(ctx.blob.log.iter().any(|line| line == "{card 90} dies"));
        chain::proceed(&mut ctx);
        assert!(matches!(
            ctx.blob.chain.first().map(|item| item.kind),
            Some(ItemKind::Trigger { source, .. }) if source == HERO
        ));
        assert!(combat::attackers(&ctx, fixtures::BF1).is_empty());
    }

    fn damaged(fixture: &mut Fixture, card: u32, damage: i32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(card),
            counter: COUNTER_DAMAGE,
            value: damage,
        });
        fixture.table.counters.sort();
    }

    static PAIRED: Card = unit(
        "Paired",
        &[Keyword::Deathknell],
        &[prelude::when(
            deathknell(&[], |ctx, item, _| {
                prelude::draw(ctx, item.controller, 1);
                Flow::Done
            }),
            |ctx, _, source| prelude::friendly_units(ctx, ctx.controller(source.card)).len() > 1,
        )],
    );

    #[test]
    fn a_batch_of_simultaneous_deaths_reads_its_deathknells_off_the_board_they_all_still_stand_on()
    {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(body(KILLER, fixtures::BF1, 0, "Paired", 3));
        fixture
            .table
            .cards
            .push(body(GUARDED, fixtures::BF1, 0, "Paired", 3));
        damaged(&mut fixture, GUARDED, 3);
        damaged(&mut fixture, KILLER, 3);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GUARDED, &PAIRED)
            .with_script(KILLER, &PAIRED);
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::dying(&ctx),
            [GUARDED, KILLER],
            "the batch is ordered by card id, never by the table's move history"
        );
        cleanup::lethal_kills(&mut ctx);
        assert_eq!(
            ctx.deaths.len(),
            2,
            "322.3 · both conditions are read while both are still on the board"
        );
    }

    #[test]
    fn one_replacement_in_a_simultaneous_batch_answers_the_lowest_id_and_the_rest_die() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(body(KILLER, fixtures::BF1, 0, "Hero", 3));
        fixture
            .table
            .cards
            .push(body(GUARDED, fixtures::BF1, 0, "Hero", 3));
        fixture
            .table
            .cards
            .push(fixtures::gear(HOURGLASS, fixtures::BASE, 0, "Hourglass", 2));
        damaged(&mut fixture, GUARDED, 3);
        damaged(&mut fixture, KILLER, 3);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GUARDED, &HERO_CARD)
            .with_script(KILLER, &HERO_CARD)
            .with_script(HOURGLASS, &HOURGLASS_CARD);
        let mut ctx = fixture.ctx();
        cleanup::lethal_kills(&mut ctx);
        assert!(
            ctx.blob.log.iter().any(|line| line
                == &format!("{{card {HOURGLASS}}} replaces the death of {{card {GUARDED}}}")),
            "the one-shot replacement answers the lowest id of the batch"
        );
        assert_eq!(ctx.card(KILLER).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.location(GUARDED), Some(Location::Base(0)));
        assert_eq!(
            cleanup::dying(&ctx),
            [GUARDED],
            "436.1 · the recall keeps the damage, so 321 kills it in the next cleanup"
        );
    }

    #[test]
    fn a_showdown_waits_until_the_deaths_of_the_cleanup_have_reached_the_chain() {
        let mut fixture = arena();
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture
            .table
            .cards
            .push(body(HERO, fixtures::BASE, 0, "Hero", 6));
        damaged(&mut fixture, HERO, 6);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(HERO, &HERO_CARD);
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.card(HERO).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.deaths.len(), 1, "the Deathknell is owed");
        assert_eq!(
            ctx.blob.showdown, None,
            "322.12 · pending items finalize before a showdown may open"
        );
        assert_eq!(ctx.blob.staged.len(), 1, "it is staged and waits");
        chain::proceed(&mut ctx);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.blob.showdown.is_some(),
            "and opens once the chain is empty again"
        );
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            crate::engine::resume(ctx, &answered)?;
        }
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        pick(ctx, seat, option)
    }

    fn choice_item(ctx: &Ctx) -> u16 {
        ctx.blob
            .chain
            .iter()
            .find(|item| is_choice(item))
            .map(|item| item.id)
            .expect("a replacement choice is parked on the chain")
    }

    #[test]
    fn two_applicable_replacements_park_the_kill_and_the_death_reads_as_replaced() {
        let mut fixture = two_hourglasses();
        let mut ctx = fixture.ctx();
        assert_eq!(
            applicable(
                &ctx,
                &WouldDie {
                    unit: GUARDED,
                    cause: Cause::Rule
                }
            )
            .len(),
            2,
            "both gears answer the same death"
        );
        assert_eq!(
            ctx.kill(GUARDED, Cause::Rule),
            Killed::Replaced,
            "one of them will run, so an 'if you do' reads as 'you did not'"
        );
        assert!(
            ctx.on_board(GUARDED),
            "nothing happens until the owner picks"
        );
        assert!(is_parked(&ctx, GUARDED));
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::BASE));
        assert_eq!(ctx.card(SECOND_GLASS).unwrap().zone, Some(fixtures::BASE));
        assert!(ctx.deaths.is_empty(), "no Deathknell for a replaced death");
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        let parked = &ctx.blob.chain[0];
        assert!(is_choice(parked));
        assert_eq!(parked.status, ItemStatus::Resolving);
        assert_eq!(parked.controller, 0);
        assert_eq!(parked.subject, Some(TargetRef::Card(GUARDED)));
        assert_eq!(
            parked.targets,
            [TargetRef::Card(HOURGLASS), TargetRef::Card(SECOND_GLASS)]
        );
        assert_eq!(decode_cause(&parked.picks), Cause::Rule);
        assert_eq!(
            ctx.kill(GUARDED, Cause::Combat),
            Killed::Replaced,
            "a second kill while the choice waits parks nothing new"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.log.iter().any(|line| line
            == &format!("{{seat 0}} chooses which replacement applies to {{card {GUARDED}}}")));
    }

    #[test]
    fn two_applicable_replacements_ask_the_dying_units_controller_which_one_runs() {
        let mut fixture = two_hourglasses();
        let mut ctx = fixture.ctx();
        ctx.kill(GUARDED, Cause::Rule);
        settle(&mut ctx).unwrap();
        let item = choice_item(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {HOURGLASS}}}"),
                format!("{{card {SECOND_GLASS}}}")
            ],
            "368 · the owner of the dying unit orders the replacements"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 1 }),
            format!("{{card {GUARDED}}}: choose which replacement applies (0 of 1)")
        );
        assert!(
            !ctx.blob.is_neutral_open(),
            "the parked kill closes the state like any chain item"
        );
        choose(&mut ctx, 0, &format!("{{card {SECOND_GLASS}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(SECOND_GLASS).unwrap().zone,
            Some(fixtures::TRASH),
            "the chosen gear kills itself"
        );
        assert_eq!(
            ctx.card(HOURGLASS).unwrap().zone,
            Some(fixtures::BASE),
            "the other is untouched"
        );
        assert_eq!(ctx.location(GUARDED), Some(Location::Base(0)));
        assert!(ctx.card(GUARDED).unwrap().exhausted, "recalled exhausted");
        assert!(ctx.blob.log.iter().any(|line| line
            == &format!("{{card {SECOND_GLASS}}} replaces the death of {{card {GUARDED}}}")));
        assert!(
            !ctx.blob
                .log
                .iter()
                .any(|line| line.ends_with("ability resolves")),
            "a replacement choice is not an ability"
        );
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(
            ctx.kill(GUARDED, Cause::Rule),
            Killed::Replaced,
            "the one gear left answers alone"
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "one applicable replacement never asks"
        );
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn the_replacement_choice_is_refused_to_the_other_seat_and_off_the_offered_list() {
        let mut fixture = two_hourglasses();
        let mut ctx = fixture.ctx();
        ctx.kill(GUARDED, Cause::Rule);
        settle(&mut ctx).unwrap();
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 2 }),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 2,
                count: 2
            }))
        );
        assert_eq!(
            priority::pass(&mut ctx, 0),
            Err(Refusal::PromptOpen),
            "nothing else moves while the choice waits"
        );
        assert!(ctx.on_board(GUARDED));
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::BASE));
        assert_eq!(ctx.card(SECOND_GLASS).unwrap().zone, Some(fixtures::BASE));
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
    }

    #[test]
    fn a_lethal_cleanup_with_two_replacements_waits_for_the_choice_then_finishes_the_cleanup() {
        let mut fixture = two_hourglasses();
        damaged(&mut fixture, GUARDED, 3);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GUARDED, &HERO_CARD)
            .with_script(HOURGLASS, &HOURGLASS_CARD)
            .with_script(SECOND_GLASS, &HOURGLASS_CARD);
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(is_parked(&ctx, GUARDED));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the cleanup loop re-reads the parked unit without parking it twice"
        );
        assert!(ctx.on_board(GUARDED));
        settle(&mut ctx).unwrap();
        let item = choice_item(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        choose(&mut ctx, 0, &format!("{{card {HOURGLASS}}}")).unwrap();
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.card(SECOND_GLASS).unwrap().zone,
            Some(fixtures::TRASH),
            "436.1 · the recall keeps the damage, so the next cleanup asks the other glass, alone"
        );
        assert_eq!(
            ctx.card(GUARDED).unwrap().zone,
            Some(fixtures::TRASH),
            "and with no glass left the third cleanup kills it for real"
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.blob
                .log
                .iter()
                .any(|line| line == &format!("{{card {GUARDED}}} dies")),
            "{:?}",
            ctx.blob.log
        );
    }

    #[test]
    fn a_combat_death_with_two_replacements_takes_the_owners_lowest_id_so_the_combat_can_finish() {
        let mut fixture = two_hourglasses();
        fixture
            .table
            .cards
            .push(body(KILLER, fixtures::BF1, 1, "Jinx", 5));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GUARDED, &HERO_CARD)
            .with_script(HOURGLASS, &HOURGLASS_CARD)
            .with_script(SECOND_GLASS, &HOURGLASS_CARD);
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        showdown::pass(&mut ctx, 1).unwrap();
        showdown::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.showdown.is_none(), "the combat resolved");
        assert!(
            ctx.blob.prompt.is_none(),
            "444 · the damage step cannot wait for a choice, so none is asked"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(GUARDED), Some(Location::Base(0)));
        assert_eq!(
            ctx.card(HOURGLASS).unwrap().zone,
            Some(fixtures::TRASH),
            "the owner's lowest id answers"
        );
        assert_eq!(ctx.card(SECOND_GLASS).unwrap().zone, Some(fixtures::BASE));
        assert_eq!(
            ctx.location(KILLER),
            Some(Location::Battlefield(fixtures::BF1)),
            "the survivor stands alone"
        );
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(1),
            "the attacker conquers the emptied battlefield as usual"
        );
        assert_eq!(ctx.points(1), 1);
        assert!(ctx.blob.log.iter().any(|line| line
            == &format!(
                "{{card {GUARDED}}} would die mid-combat · {{card {HOURGLASS}}} is taken without asking"
            )));
    }

    static SLAY: Card = prelude::spell(
        "Slay",
        &[],
        &[prelude::play(
            &[prelude::a_unit("a unit")],
            |ctx, item, _| {
                if let Some(unit) = prelude::card_target(ctx, item, 0) {
                    if ctx.kill(unit, Cause::Item(item.id)) == Killed::Yes {
                        prelude::draw(ctx, item.controller, 1);
                    }
                }
                Flow::Done
            },
        )],
    );

    #[test]
    fn a_spells_kill_with_two_replacements_finishes_the_spell_then_asks_and_if_you_do_is_false() {
        let mut fixture = two_hourglasses();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &SLAY);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let entry = crate::engine::ctx::EntryMove {
            card: fixtures::HAND_SPELL,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        crate::engine::legal::classify(&ctx, 0, &entry).unwrap();
        let chain_zone = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::HAND_SPELL, chain_zone, 0),
                0,
            )
            .unwrap();
        crate::engine::play::begin(
            &mut ctx,
            0,
            fixtures::HAND_SPELL,
            crate::state::Origin::Hand,
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        choose(&mut ctx, 0, &format!("{{card {GUARDED}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH),
            "the spell resolved and left the chain"
        );
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1,
            "the death was replaced, so 'if you do' drew nothing"
        );
        let item = choice_item(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        assert_eq!(decode_cause(&ctx.blob.chain[0].picks), Cause::Item(1));
        choose(&mut ctx, 0, &format!("{{card {HOURGLASS}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.location(GUARDED), Some(Location::Base(0)));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn the_cause_survives_the_park_as_bytes() {
        for cause in [
            Cause::Item(300),
            Cause::Ability(crate::cards::Source {
                card: 70000,
                ability: 2,
            }),
            Cause::Replacement,
            Cause::Combat,
            Cause::Cleanup { last_item: None },
            Cause::Cleanup { last_item: Some(9) },
            Cause::Cost,
            Cause::Rule,
        ] {
            assert_eq!(decode_cause(&encode_cause(cause)), cause);
        }
        assert_eq!(decode_cause(&[]), Cause::Rule);
        assert_eq!(decode_cause(&[0, 1]), Cause::Rule, "a truncated item id");
        assert_eq!(decode_cause(&[1, 1, 2]), Cause::Rule);
        assert_eq!(decode_cause(&[9]), Cause::Rule);
    }

    fn lonely(id: u32, zone: u16) -> CardInfo {
        fixtures::unit(id, zone, 0, "Lonely Poro", 2)
    }

    fn poro_pair() -> Fixture {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.cards.push(lonely(HERO, fixtures::BF1));
        fixture.table.cards.push(lonely(GUARDED, fixtures::BF1));
        damaged(&mut fixture, HERO, 2);
        fixture.resolve();
        fixture
    }

    fn resolve_deaths(ctx: &mut Ctx) {
        settle(ctx).unwrap();
        while let Some(PromptWhy::OrderTriggers { seat }) = ctx.blob.why {
            let labels = prompts::offered(ctx)
                .iter()
                .map(|opt| opt.label.clone())
                .collect::<Vec<_>>();
            let next = labels
                .iter()
                .find(|label| *label == "done")
                .or_else(|| labels.first())
                .cloned()
                .unwrap();
            choose(ctx, seat, &next).unwrap();
        }
        while !ctx.blob.chain.is_empty() {
            let holder = priority::holder(ctx).unwrap();
            priority::pass(ctx, holder).unwrap();
        }
    }

    #[test]
    fn the_alone_snapshot_is_taken_while_the_whole_dying_batch_still_stands() {
        let mut fixture = poro_pair();
        damaged(&mut fixture, GUARDED, 2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(!ctx.alone_at(HERO) && !ctx.alone_at(GUARDED));
        assert_eq!(cleanup::lethal_kills(&mut ctx), [HERO, GUARDED]);
        let noted: Vec<Noted> = ctx
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Died { noted, .. } => Some(*noted),
                _ => None,
            })
            .collect();
        assert_eq!(noted.len(), 2);
        assert!(
            noted.iter().all(|noted| !noted.alone),
            "808.1.d.3 · two Poros dying in one batch did not die alone: {noted:?}"
        );
        resolve_deaths(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand, "neither draws");
        let mut beside = poro_pair();
        beside.table.card_mut(GUARDED).unwrap().name = "Sprite".into();
        beside.table.tokens.push(GUARDED);
        beside.resolve();
        let mut ctx = beside.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(cleanup::lethal_kills(&mut ctx), [HERO]);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, noted: Noted { alone: false, .. }, .. } if *card == HERO
        )));
        resolve_deaths(&mut ctx);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "740.2.a · a surviving token kept it company"
        );
        let mut alone = poro_pair();
        alone.table.cards.retain(|card| card.id != GUARDED);
        alone
            .table
            .cards
            .push(body(KILLER, fixtures::BF1, 1, "Jinx", 5));
        alone.resolve();
        let mut ctx = alone.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(cleanup::lethal_kills(&mut ctx), [HERO]);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, noted: Noted { alone: true, .. }, .. } if *card == HERO
        )));
        resolve_deaths(&mut ctx);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + 1,
            "the last friendly unit standing died alone: an enemy is not company"
        );
    }

    static HORROR: Card = unit(
        "Horror",
        &[],
        &[prelude::when(
            prelude::on_attack(&[], |ctx, item, _| {
                let me = item.kind.source();
                prelude::might_this_turn(ctx, item, me, 2, None);
                prelude::gain_xp(ctx, item.controller, 2);
                Flow::Done
            }),
            |ctx, _, source| prelude::enemy_alone_at(ctx, source.card),
        )],
    );

    fn xp_of(ctx: &Ctx, seat: u8) -> i32 {
        ctx.table
            .counter(Target::Seat(seat), crate::rules::COUNTER_XP)
            .unwrap_or(0)
    }

    fn horror_marches_into(enemies: &[u32]) -> Fixture {
        let mut fixture = arena();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture
            .table
            .cards
            .retain(|card| ![fixtures::SPRITE, fixtures::VI].contains(&card.id));
        fixture
            .table
            .cards
            .push(body(HERO, fixtures::BASE, 0, "Horror", 4));
        for enemy in enemies {
            fixture
                .table
                .cards
                .push(body(*enemy, fixtures::BF1, 1, "Jinx", 1));
        }
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(HERO, &HORROR);
        fixture
    }

    fn march_and_open(ctx: &mut Ctx) {
        assert_eq!(
            ctx.location(HERO),
            Some(Location::Battlefield(fixtures::BF1))
        );
        crate::engine::march::standard_move(
            ctx,
            0,
            HERO,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        cleanup::run(ctx, None);
        settle(ctx).unwrap();
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        assert!(ctx.is_attacker(HERO));
    }

    #[test]
    fn an_attacker_facing_one_enemy_alone_mutates_once_and_facing_two_does_not() {
        let mut fixture = horror_marches_into(&[KILLER]);
        let action = fixtures::move_action(HERO, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march_and_open(&mut ctx);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the attack trigger fired at designation"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == HERO
        ));
        assert!(ctx.blob.showdown.as_ref().unwrap().initial_chain);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(HERO), 6, "+2 this turn");
        assert_eq!(xp_of(&ctx, 0), 2);
        assert!(
            ctx.blob.showdown.is_some(),
            "the combat itself is still open"
        );
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(ctx.card(KILLER).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(xp_of(&ctx, 0), 2, "383.4.e.2.b · once per designation");
        let mut two = horror_marches_into(&[KILLER, SECOND_GLASS]);
        let action = fixtures::move_action(HERO, fixtures::BF1, 0);
        let mut ctx = two.ctx_for(0, &action);
        march_and_open(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "two enemies here: the condition read at designation fails"
        );
        assert_eq!(ctx.current_might(HERO), 4);
        assert_eq!(xp_of(&ctx, 0), 0);
    }
}
