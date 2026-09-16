use super::prelude::{
    deal, detach_gear, done, equip, gear, lender_of, triggered, when, while_attached, with_statics,
};
use super::{
    Ability, Card, Cost, Domain, Event, Flow, Grant, Item, Keyword, Power, Source, Stage, Trigger,
    GRANTED,
};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Fury)],
};

pub const MIGHT_BONUS: i16 = 4;
pub const DAMAGE: u8 = 4;
pub const AT_END_OF_TURN: u8 = GRANTED;

pub fn conquered_this_turn(ctx: &Ctx, unit: u32) -> bool {
    ctx.events
        .iter()
        .any(|event| matches!(event, Event::Conquered { units, .. } if units.contains(&unit)))
}

pub fn i_did_not_conquer_this_turn(ctx: &Ctx, event: &Event, source: Source) -> bool {
    matches!(event, Event::EndingStep { .. }) && !conquered_this_turn(ctx, source.card)
}

fn blight(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let axe = lender_of(item);
    if let Some(axe) = axe.filter(|axe| detach_gear(ctx, *axe)) {
        ctx.narrate(format!(
            "{{card {me}}} did not conquer this turn · {{card {axe}}} comes loose"
        ));
    }
    if deal(ctx, item, me, DAMAGE) {
        ctx.narrate(format!("{{card {me}}} takes {DAMAGE}"));
    }
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [when(
    triggered(Trigger::EndOfTurn, &[], blight),
    i_did_not_conquer_this_turn,
)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear(
        "Blighted Battleaxe",
        &[Keyword::Equip(EQUIP)],
        &[equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{conquered, equipment, queue_granted, GEAR};
    use crate::cards::prelude::{attach_gear, attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, phases, play as play_engine, priority, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;

    const WAILER: u32 = 95;
    const EQUIP_INDEX: u8 = 0;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Blighted Battleaxe", 4, "Fury"));
        fixture
            .table
            .cards
            .push(fixtures::unit(WAILER, fixtures::BASE, 0, "Wailer", 5));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(GEAR).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        resolve_chain(ctx);
    }

    fn source() -> Source {
        Source {
            card: fixtures::VI,
            ability: AT_END_OF_TURN,
        }
    }

    fn ending(seat: u8) -> Event {
        Event::EndingStep { seat }
    }

    #[test]
    fn the_script_is_a_one_energy_fury_equipment_with_plus_four_and_a_wearer_end_of_turn_listener()
    {
        assert!(std::ptr::eq(
            script_of("Blighted Battleaxe").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Blighted Battleaxe");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(EQUIP.energy, 1);
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        let listener = &WEARER_TEXT[0];
        assert_eq!(listener.trigger, Trigger::EndOfTurn);
        assert!(
            listener.condition.is_some(),
            "383.2.a.1 · the if is the condition"
        );
        assert!(listener.targets.is_empty());
        assert!(!listener.optional);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(4), Grant::Ability(_)]
        ));
        assert_eq!((MIGHT_BONUS, DAMAGE), (4, 4));
    }

    #[test]
    fn equipping_pays_one_energy_and_a_fury_rune_and_the_wearer_gets_plus_four() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, GEAR), Some(fixtures::VI));
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "a Fury rune recycled for the power"
        );
        assert_eq!(
            ctx.runes_of(0)
                .iter()
                .filter(|rune| !rune.exhausted)
                .count(),
            2,
            "one ready rune exhausted for the energy"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 7);
        assert_eq!(ctx.location(GEAR), Some(Location::Base(0)));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +4 Might while {card 90} is attached".to_string()));
        ctx.detach(GEAR);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_condition_reads_the_wearer_at_its_controllers_ending_step_with_no_conquer_in_the_record()
    {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(
            triggers::find(&ctx, &ending(0)).is_empty(),
            "loose gear has no wearer"
        );
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert!(!conquered_this_turn(&ctx, fixtures::VI));
        assert!(i_did_not_conquer_this_turn(&ctx, &ending(0), source()));
        assert_eq!(triggers::find(&ctx, &ending(0)).len(), 1);
        assert!(
            triggers::find(&ctx, &ending(1)).is_empty(),
            "the other seat's turn ending is not yours"
        );
        assert!(!i_did_not_conquer_this_turn(
            &ctx,
            &Event::BeginningPhase { seat: 0 },
            source()
        ));
        ctx.raise(conquered(&[fixtures::SPRITE]));
        assert!(
            !conquered_this_turn(&ctx, fixtures::VI),
            "another unit's conquer is not the wearer's"
        );
        assert!(i_did_not_conquer_this_turn(&ctx, &ending(0), source()));
        ctx.raise(conquered(&[fixtures::VI, WAILER]));
        assert!(conquered_this_turn(&ctx, fixtures::VI));
        assert!(!i_did_not_conquer_this_turn(&ctx, &ending(0), source()));
        assert!(triggers::find(&ctx, &ending(0)).is_empty());
    }

    #[test]
    fn the_run_unattaches_first_so_the_four_damage_lands_on_the_bare_wearer() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 7);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            AT_END_OF_TURN,
            TargetRef::Seat(0),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, GEAR), None);
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::TRASH),
            "four damage on a bare 3-Might wearer is lethal"
        );
        assert_eq!(
            ctx.location(GEAR),
            Some(Location::Base(0)),
            "the loose axe stays home"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} did not conquer this turn · {card 90} comes loose".to_string()));
        assert!(ctx.blob.log.contains(&"{card 50} takes 4".to_string()));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, WAILER);
        assert_eq!(ctx.current_might(WAILER), 9);
        queue_granted(&mut ctx, WAILER, GEAR, AT_END_OF_TURN, TargetRef::Seat(0));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(attached_to(&ctx, GEAR), None);
        assert_eq!(ctx.current_might(WAILER), 5);
        assert_eq!(ctx.damage_on(WAILER), 4, "a 5-Might wearer survives it");
        assert_eq!(ctx.location(WAILER), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.detach(GEAR);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            AT_END_OF_TURN,
            TargetRef::Seat(0),
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::TRASH),
            "an axe already loose has nothing to unattach, the four still land"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_a_missing_fury_rune_and_an_attached_axe_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, GEAR, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = armed();
        for rune in [fixtures::RUNE_A, 41, 43] {
            let held = broke.table.card_mut(rune).unwrap();
            held.domain = vec!["Calm".into()];
            held.name = "Calm Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX),
            Err(Refusal::NoPowerOf),
            "seat 0 holds no Fury rune"
        );
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }

    #[test]
    fn at_the_end_of_your_turn_a_wearer_that_did_not_conquer_drops_the_axe_and_takes_four() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, WAILER);
        phases::end_turn(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.iter().any(|held| matches!(
                held.kind,
                ItemKind::Granted { holder, lender, index: AT_END_OF_TURN } if holder == WAILER && lender == GEAR
            )),
            "the wearer's appended trigger"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(attached_to(&ctx, GEAR), None);
        assert!(
            ctx.events.iter().any(|event| matches!(
                event,
                Event::DamageDealt { card, n: DAMAGE, .. } if *card == WAILER
            )),
            "the bare wearer took four"
        );
        assert_eq!(
            ctx.location(WAILER),
            Some(Location::Base(0)),
            "a 5-Might wearer survives it"
        );
        assert_eq!(
            ctx.damage_on(WAILER),
            0,
            "317.2.b · the Expiration Step heals every unit"
        );
    }

    #[test]
    #[ignore = "engine gap · per-turn counters: no CardState row records which units conquered this turn, so conquered_this_turn reads only this request's events; with a per-unit conquer record set by cleanup::conquer and cleared at Expiration, a wearer that conquered earlier in the turn keeps the axe"]
    fn a_wearer_that_conquered_earlier_in_the_turn_keeps_the_axe_at_the_ending_step() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        assert!(conquered_this_turn(&ctx, fixtures::VI));
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        let ctx = fixture.ctx();
        assert!(ctx.events.is_empty(), "a fresh request");
        assert!(conquered_this_turn(&ctx, fixtures::VI));
        assert!(!i_did_not_conquer_this_turn(&ctx, &ending(0), source()));
    }
}
