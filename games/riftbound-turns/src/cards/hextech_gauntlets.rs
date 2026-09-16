use super::prelude::{
    done, draw, equip, excess_damage_assigned_in_my_attack, gear, on_conquer_me, when,
    while_attached, with_statics,
};
use super::{
    Ability, Card, Cost, Event, Flow, Grant, Item, Keyword, Power, Source, Stage, GRANTED,
};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 3,
    power: &[Power::Rainbow],
};

pub const MIGHT_BONUS: i16 = 3;
pub const EXCESS_TO_DRAW: u8 = 3;
pub const DRAWS: usize = 1;
pub const ON_CONQUER: u8 = GRANTED;

pub fn discounted(ctx: &Ctx, wearer: u32) -> Cost {
    let might = u8::try_from(ctx.current_might(wearer).max(0)).unwrap_or(u8::MAX);
    Cost {
        energy: EQUIP.energy.saturating_sub(might),
        power: EQUIP.power,
    }
}

pub fn conquered_after_an_attack_with_three_excess(ctx: &Ctx, event: &Event, _: Source) -> bool {
    let Event::Conquered { zone, seat, .. } = event else {
        return false;
    };
    excess_damage_assigned_in_my_attack(ctx, *seat, *zone)
        .is_some_and(|excess| excess >= EXCESS_TO_DRAW)
}

fn overcharge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [when(
    on_conquer_me(&[], overcharge),
    conquered_after_an_attack_with_three_excess,
)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear(
        "Hextech Gauntlets",
        &[Keyword::Equip(EQUIP)],
        &[equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{conquered, equipment, held, queue_granted, GEAR};
    use crate::cards::prelude::{attach_gear, attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, play as play_engine, priority, settle, triggers};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;

    const RUNT: u32 = 95;
    const EQUIP_INDEX: u8 = 0;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Hextech Gauntlets", 3, "Fury"));
        fixture
            .table
            .cards
            .push(fixtures::unit(RUNT, fixtures::BASE, 0, "Runt", 1));
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
            ability: ON_CONQUER,
        }
    }

    #[test]
    fn the_script_is_a_three_energy_rainbow_equipment_with_plus_three_and_a_wearer_conquer_listener(
    ) {
        assert!(std::ptr::eq(script_of("Hextech Gauntlets").unwrap(), &CARD));
        assert_eq!(CARD.name, "Hextech Gauntlets");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!((EQUIP.energy, EQUIP.power), (3, &[Power::Rainbow][..]));
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert!(
            equip.extra.is_none(),
            "the seam: the discount waits on the chosen unit"
        );
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        let listener = &WEARER_TEXT[0];
        assert_eq!(
            listener.trigger,
            Trigger::Conquer(Who::Me),
            "136.2.c · I am the wearer"
        );
        assert!(
            listener.condition.is_some(),
            "383.2.a.1 · the if is the condition"
        );
        assert!(listener.targets.is_empty());
        assert!(!listener.optional);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(3), Grant::Ability(_)]
        ));
        assert_eq!(ON_CONQUER, GRANTED);
        assert_eq!((MIGHT_BONUS, EXCESS_TO_DRAW, DRAWS), (3, 3, 1));
    }

    #[test]
    fn the_discount_reads_the_chosen_units_current_might_and_never_touches_the_rainbow() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            discounted(&ctx, fixtures::VI),
            Cost {
                energy: 0,
                power: &[Power::Rainbow]
            },
            "three Might pays the three energy"
        );
        assert_eq!(discounted(&ctx, RUNT).energy, 2);
        assert_eq!(discounted(&ctx, fixtures::SPRITE).energy, 0);
        attach_gear(&mut ctx, GEAR, RUNT);
        assert_eq!(
            discounted(&ctx, RUNT).energy,
            0,
            "current Might, buffs and bonuses included"
        );
        ctx.detach(GEAR);
        ctx.might(RUNT, -3, crate::state::Expiry::Permanent, None, 0);
        assert_eq!(
            discounted(&ctx, RUNT).energy,
            3,
            "a Might below zero reduces nothing"
        );
    }

    #[test]
    fn equipping_at_the_printed_price_pays_three_energy_and_any_rune_and_gives_plus_three() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.runes_of(0)
                .iter()
                .filter(|rune| !rune.exhausted)
                .count(),
            3
        );
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, GEAR), Some(fixtures::VI));
        assert_eq!(ctx.runes_of(0).len(), 3, "one rune of any domain recycled");
        assert!(
            ctx.runes_of(0).iter().all(|rune| rune.exhausted),
            "every ready rune exhausted for the energy"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert_eq!(ctx.location(GEAR), Some(Location::Base(0)));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +3 Might while {card 90} is attached".to_string()));
        ctx.detach(GEAR);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_condition_reads_three_excess_in_the_conquering_seats_attack_and_nothing_else() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            None,
            "no attack, no record"
        );
        assert!(!conquered_after_an_attack_with_three_excess(
            &ctx,
            &conquered(&[fixtures::VI]),
            source()
        ));
        ctx.record_excess_damage(0, fixtures::BF1, 2);
        assert!(
            !conquered_after_an_attack_with_three_excess(
                &ctx,
                &conquered(&[fixtures::VI]),
                source()
            ),
            "two excess is not three"
        );
        ctx.record_excess_damage(0, fixtures::BF1, 3);
        assert!(conquered_after_an_attack_with_three_excess(
            &ctx,
            &conquered(&[fixtures::VI]),
            source()
        ));
        assert!(!conquered_after_an_attack_with_three_excess(
            &ctx,
            &Event::Conquered {
                zone: fixtures::BF2,
                seat: 0,
                units: vec![fixtures::VI]
            },
            source()
        ));
        assert!(!conquered_after_an_attack_with_three_excess(
            &ctx,
            &held(&[fixtures::VI]),
            source()
        ));
    }

    #[test]
    fn the_run_draws_one_for_the_wearers_controller() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let hand = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.hand_of(1).len(), theirs, "the other seat draws nothing");
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_too_few_runes_and_attached_gauntlets_are_refused() {
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

        let mut broke = Fixture::enforced();
        broke
            .table
            .cards
            .push(equipment(GEAR, 0, "Hextech Gauntlets", 3, "Fury"));
        for rune in [41, 42] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 1
            }),
            "one ready rune cannot pay three energy at the printed price"
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
    fn a_conquer_without_three_excess_another_units_conquer_and_loose_gauntlets_draw_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.record_excess_damage(0, fixtures::BF1, 2);
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 0, "two excess is not three");
        ctx.record_excess_damage(0, fixtures::BF1, 3);
        ctx.raise(conquered(&[RUNT]));
        assert_eq!(triggers::collect(&mut ctx), 0, "another unit's conquer");
        ctx.detach(GEAR);
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "loose gear grants nothing to anyone"
        );
    }

    #[test]
    #[ignore = "engine gap · target-dependent costs: ExtraCost sees the source only, so the Equip's energy cannot read the chosen unit's Might; with an ExtraCost evaluated on the item at choose_targets and STAGE_PAY (the any_affordable path deciding legality), discounted(ctx, wearer) is the price and one ready rune equips a 3-Might unit"]
    fn one_ready_rune_equips_a_three_might_unit_for_the_rainbow_alone() {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Hextech Gauntlets", 3, "Fury"));
        for rune in [41, 42] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.runes_of(0)
                .iter()
                .filter(|rune| !rune.exhausted)
                .count(),
            1
        );
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, GEAR), Some(fixtures::VI));
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "the one ready rune paid the rainbow"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 6);
    }

    #[test]
    fn a_conquer_after_an_attack_with_three_excess_draws_one_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let hand = ctx.hand_of(0).len();
        ctx.record_excess_damage(0, fixtures::BF1, 3);
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(
            excess_damage_assigned_in_my_attack(&ctx, 0, fixtures::BF1),
            Some(3)
        );
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
    }
}
