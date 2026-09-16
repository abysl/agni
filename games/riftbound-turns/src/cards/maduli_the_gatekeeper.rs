use super::prelude::{
    activated, asking, done, move_destinations, move_unit, named, paying_with, suppresses_itself,
    unit, usable_if, with_candidates, with_statics, Location, CHAOS,
};
use super::{Card, Flow, Item, SelfCost, Source, Stage, Static, Timing};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const QUESTION: &str = "an occupied enemy battlefield to move to";
pub const PICKED: u8 = 1;

pub fn enemy_might_at(ctx: &Ctx, seat: u8, zone: u16) -> i32 {
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat)
        .map(|unit| ctx.current_might(unit))
        .sum()
}

fn occupied_by_enemies(ctx: &Ctx, seat: u8, zone: u16) -> bool {
    ctx.blob.holder(zone).is_some_and(|holder| holder != seat)
        && ctx
            .units_at(Location::Battlefield(zone))
            .into_iter()
            .any(|unit| ctx.controller(unit) != seat)
}

pub fn gates_open_to(ctx: &Ctx, me: u32) -> Vec<Location> {
    if !ctx.on_board(me) {
        return Vec::new();
    }
    let seat = ctx.controller(me);
    let might = ctx.current_might(me);
    move_destinations(ctx, me)
        .into_iter()
        .filter(|to| match to {
            Location::Battlefield(zone) => {
                occupied_by_enemies(ctx, seat, *zone) && might > enemy_might_at(ctx, seat, *zone)
            }
            Location::Base(_) => false,
        })
        .collect()
}

fn a_gate_is_open(ctx: &Ctx, source: Source) -> bool {
    !gates_open_to(ctx, source.card).is_empty()
}

fn gates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != PICKED {
        return Vec::new();
    }
    gates_open_to(ctx, item.kind.source())
        .into_iter()
        .filter_map(|to| ctx.zone_of(to))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn picked_gate(ctx: &Ctx, me: u32) -> Option<Location> {
    let zone = u16::try_from(*ctx.picks().first()?).ok()?;
    let to = Location::of_zone(zone, ctx.controller(me), &ctx.zones)?;
    gates_open_to(ctx, me).contains(&to).then_some(to)
}

fn open_the_gate(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    if stage.0 == PICKED {
        match picked_gate(ctx, me) {
            Some(to) => {
                move_unit(ctx, item, me, to);
            }
            None => ctx.narrate(format!("{{card {me}}} stays where he is")),
        }
        return done();
    }
    if gates_open_to(ctx, me).is_empty() {
        ctx.narrate(format!(
            "{{card {me}}} stays where he is · no occupied enemy battlefield where his Might is greater"
        ));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, PICKED, 1, 1))
}

pub static CARD: Card = with_statics(
    unit(
        "Maduli the Gatekeeper",
        &[],
        &[named(
            asking(
                with_candidates(
                    usable_if(
                        paying_with(
                            activated(Timing::Sorcery, CHAOS, &[], open_the_gate),
                            SelfCost::Free,
                        ),
                        a_gate_is_open,
                    ),
                    gates,
                ),
                QUESTION,
            ),
            "move to an occupied enemy battlefield",
        )],
    ),
    &[Static::ReadySuppressed {
        by_effects: suppresses_itself,
        by_awaken: suppresses_itself,
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::might_this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, phases, priority, prompts};
    use crate::state::{GameBlob, ItemKind, Mode, Phase, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const MADULI: u32 = 90;
    const GUARD: u32 = 91;
    const WALL: u32 = 92;
    const PLAIN: u32 = 54;
    const CHAOS_RUNE: u32 = 46;

    fn maduli(zone: u16, exhausted: bool) -> CardInfo {
        CardInfo {
            energy: Some(7),
            power: Some(1),
            domain: vec!["Chaos".into()],
            exhausted,
            ..fixtures::unit(MADULI, zone, 0, "Maduli the Gatekeeper", 6)
        }
    }

    fn his_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == MADULI)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn gates_of_bandle(zone: u16, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(maduli(zone, exhausted));
        fixture
            .table
            .cards
            .push(fixtures::unit(GUARD, fixtures::BF1, 1, "Guard", 2));
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF3,
            1,
            "Plain Field",
            "Battlefield",
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(WALL, fixtures::BF3, 1, "Wall", 7));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_holder(fixtures::BF3, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MADULI).unwrap(),
            &CARD
        ));
        fixture
    }

    fn chaos_rune_recycled(ctx: &Ctx) -> bool {
        ctx.effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::Move {
                    card: CHAOS_RUNE,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    ..
                }
            )
        })
    }

    #[test]
    fn the_script_is_a_unit_with_one_chaos_ability_that_does_not_exhaust_him_and_the_ready_veto() {
        assert!(std::ptr::eq(
            script_of("Maduli the Gatekeeper").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(matches!(CARD.statics, [Static::ReadySuppressed { .. }]));
        assert_eq!(CARD.abilities.len(), 1);
        let gate = &CARD.abilities[0];
        assert_eq!(gate.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(gate.self_cost, SelfCost::Free);
        assert_eq!(gate.cost, Some(CHAOS));
        assert!(gate.targets.is_empty());
        assert!(gate.usable.is_some());
        assert!(gate.candidates.is_some());
        assert_eq!(gate.question, Some(QUESTION));
        assert_eq!(gate.label, Some("move to an occupied enemy battlefield"));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = gates_of_bandle(fixtures::BASE, true);
        let ctx = fixture.ctx();
        assert!(ctx.ready_suppressed(MADULI));
        assert!(ctx.awaken_suppressed(MADULI));
        assert!(
            !ctx.ready_suppressed(fixtures::VI),
            "only he can't be readied"
        );
        assert!(!ctx.ready_suppressed(GUARD));
        assert!(!ctx.awaken_suppressed(GUARD));
    }

    #[test]
    fn the_gates_are_the_enemy_held_battlefields_with_enemy_units_whose_total_might_is_below_his() {
        let mut fixture = gates_of_bandle(fixtures::BASE, true);
        let mut ctx = fixture.ctx();
        assert_eq!(enemy_might_at(&ctx, 0, fixtures::BF1), 2);
        assert_eq!(enemy_might_at(&ctx, 0, fixtures::BF2), 3, "the Sprite");
        assert_eq!(enemy_might_at(&ctx, 0, fixtures::BF3), 7);
        assert_eq!(
            gates_open_to(&ctx, MADULI),
            [
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ],
            "6 beats 2 and 3, not 7"
        );
        let item = Item::new(
            5,
            ItemKind::Ability {
                source: MADULI,
                index: 0,
            },
            0,
            crate::state::Origin::Board,
        );
        assert_eq!(
            gates(&ctx, &item, Stage(PICKED)),
            [
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Zone(fixtures::BF2)
            ]
        );
        assert!(gates(&ctx, &item, Stage(0)).is_empty());
        might_this_turn(&mut ctx, &item, MADULI, -4, None);
        assert_eq!(ctx.current_might(MADULI), 2);
        assert_eq!(
            gates_open_to(&ctx, MADULI),
            [],
            "2 is not greater than 2 · the comparison is strict"
        );
        drop(ctx);

        let mut empty = gates_of_bandle(fixtures::BASE, true);
        empty.table.card_mut(GUARD).unwrap().zone = Some(fixtures::BASE);
        empty.blob.set_holder(fixtures::BF2, Some(0));
        empty.resolve();
        let ctx = empty.ctx();
        assert_eq!(
            gates_open_to(&ctx, MADULI),
            [],
            "an enemy-held battlefield without units is not occupied, a battlefield you hold is not enemy"
        );
    }

    #[test]
    fn the_chaos_moves_him_through_the_chosen_gate_without_exhausting_him_and_starts_the_contest() {
        let mut fixture = gates_of_bandle(fixtures::BASE, true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            his_offers(&ctx),
            [(
                format!("{{card {MADULI}}}: move to an occupied enemy battlefield (1 Chaos power)"),
                true
            )],
            "an exhausted Maduli still opens the gate · no exhaust in the cost"
        );
        activate::activate(&mut ctx, 0, MADULI, 0).unwrap();
        assert!(chaos_rune_recycled(&ctx), "{:?}", ctx.effects);
        assert!(ctx.card(MADULI).unwrap().exhausted, "still exhausted");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == MADULI
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICKED
            }),
            "the gate is picked as the ability resolves"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BF1),
                format!("{{zone {}}}", fixtures::BF2)
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(MADULI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, to: Location::Battlefield(fixtures::BF1), cause: MoveCause::Effect, .. } if *card == MADULI
        )));
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(0),
            "moving onto the enemy's battlefield contests it"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_gate_that_closed_before_the_ability_resolves_leaves_him_where_he_is() {
        let mut fixture = gates_of_bandle(fixtures::BASE, true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, MADULI, 0).unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, MADULI, -4, None);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing to pick from");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(MADULI), Some(Location::Base(0)));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MADULI}}} stays where he is · no occupied enemy battlefield where his Might is greater"
        )));
        assert!(chaos_rune_recycled(&ctx), "the Chaos is spent all the same");
    }

    #[test]
    fn with_no_open_gate_or_no_chaos_the_ability_is_refused_and_an_opponent_cannot_use_it() {
        let mut walled = gates_of_bandle(fixtures::BASE, true);
        walled.table.card_mut(GUARD).unwrap().might = Some(6);
        walled.table.card_mut(fixtures::SPRITE).unwrap().might = Some(9);
        walled.resolve();
        let mut ctx = walled.ctx();
        assert!(his_offers(&ctx).is_empty(), "no gate is open");
        assert_eq!(
            activate::activate(&mut ctx, 0, MADULI, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, MADULI, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
        drop(ctx);

        let mut poor = gates_of_bandle(fixtures::BASE, true);
        poor.table.cards.retain(|card| card.id != CHAOS_RUNE);
        poor.resolve();
        let mut ctx = poor.ctx();
        assert_eq!(
            his_offers(&ctx),
            [(
                format!("{{card {MADULI}}}: move to an occupied enemy battlefield (1 Chaos power)"),
                false
            )],
            "listed, greyed"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, MADULI, 0),
            Err(Refusal::NoPowerOf)
        );
    }

    #[test]
    fn he_is_never_readied_by_awaken_or_by_an_effect() {
        let mut fixture = gates_of_bandle(fixtures::BASE, true);
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.set_phase(Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert!(
            !ctx.effects.contains(&Effect::ready(MADULI)),
            "I can't be readied · the Awaken step skips him"
        );
        assert!(
            ctx.effects.contains(&Effect::ready(fixtures::RUNE_A)),
            "everything else readies"
        );
        assert!(!ctx.ready(MADULI), "an effect cannot ready him either");
        assert!(!ctx.awaken(MADULI, 0));
        assert!(ctx.card(MADULI).unwrap().exhausted);
        assert_eq!(
            priority::holder(&ctx),
            None,
            "nothing on the chain after the Beginning phase"
        );
    }
}
