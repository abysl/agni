use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power};
use crate::engine::attach;
use crate::engine::ctx::Ctx;

pub const CALM: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};

pub const MIGHT_BONUS: i16 = 1;
pub const FRESH_BONUS: i16 = 2;

fn attached_this_turn(ctx: &Ctx, _unit: u32, gear: u32) -> bool {
    attach::attached_this_turn(ctx, gear)
}

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Might(MIGHT_BONUS),
    Grant::MightIf(attached_this_turn, FRESH_BONUS),
];

pub static CARD: Card = with_statics(
    gear("Brutalizer", &[Keyword::Equip(CALM)], &[equip(CALM)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, is_attached, Attached, FRIENDLY_UNIT};
    use crate::cards::{SelfCost, Static, Timing, Trigger};
    use crate::engine::ctx::{Cause, Killed, Location, MoveCause, Moved, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, priority, prompts, resume, settle, statics};
    use crate::state::{PromptWhy, FLAG_STUNNED};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const BRUTALIZER: u32 = 90;
    const EQUIP: u8 = 0;

    fn brutalizer(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::gear(id, fixtures::BASE, seat, "Brutalizer", 2);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(brutalizer(BRUTALIZER, 0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BRUTALIZER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx.blob.prompt.as_ref().map(|held| held.id).unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, BRUTALIZER, EQUIP).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        pick(ctx, 0, 0).unwrap();
        resolve_chain(ctx);
    }

    fn might_counters(ctx: &Ctx, card: u32) -> i32 {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Counter {
                    target: Target::Card(held),
                    counter,
                    delta,
                } if *held == card && *counter == COUNTER_MIGHT => Some(*delta),
                _ => None,
            })
            .sum()
    }

    #[test]
    fn the_script_is_a_calm_equipment_with_a_stored_plus_one_and_a_projected_plus_two() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Brutalizer").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.equip_cost(), Some(CALM));
        assert!(CARD.is_equipment());
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[0];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(CALM));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(1), Grant::MightIf(_, 2)]
        ));
    }

    #[test]
    fn equipping_costs_a_calm_rune_and_the_wearer_reads_plus_three_this_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == BRUTALIZER)
            .expect("the loose Brutalizer offers its Equip");
        assert!(offer.enabled);
        assert_eq!(offer.label, "{card 90}: equip (1 Calm power)");
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, BRUTALIZER), Some(fixtures::VI));
        assert_eq!(ctx.attached_turn(BRUTALIZER), ctx.turn());
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "the Calm rune was recycled for the power"
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            6,
            "+1 stored and +2 projected on the turn it was attached"
        );
        assert_eq!(
            might_counters(&ctx, fixtures::VI),
            1,
            "kai sees only the stored bonus as a counter; the +2 is a projection fact"
        );
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::MightIf(_, 2)]
        ));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +1 Might while {card 90} is attached".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_plus_two_survives_moves_stuns_and_combat_but_lapses_when_the_turn_advances() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.current_might(fixtures::VI), 6, "a move keeps it");
        assert!(ctx.stun(fixtures::VI));
        assert!(ctx.has_flag(fixtures::VI, FLAG_STUNNED));
        assert_eq!(ctx.current_might(fixtures::VI), 6, "a stun keeps it");
        crate::engine::expiry::at_combat_end(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 6, "combat ending keeps it");
        ctx.blob.core_mut().unwrap().advance();
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "the next turn reads only the +1, with no expiry to clean"
        );
        assert!(statics::grants_on(&ctx, fixtures::VI).is_empty());
        assert_eq!(might_counters(&ctx, fixtures::VI), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn re_equipping_on_a_later_turn_reads_plus_three_again_and_a_new_wearer_takes_both_parts() {
        let mut fixture = armed();
        fixture.table.card_mut(41).unwrap().domain = vec!["Calm".into()];
        fixture.table.card_mut(41).unwrap().name = "Calm Rune".into();
        fixture
            .table
            .cards
            .push(fixtures::unit(95, fixtures::BASE, 0, "Wailer", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        ctx.blob.core_mut().unwrap().advance();
        ctx.blob.core_mut().unwrap().advance();
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(
            activate::activate(&mut ctx, 0, BRUTALIZER, EQUIP),
            Err(Refusal::Illegal(Reason::Attached)),
            "134.4 · the attached Brutalizer's own Equip is inactive"
        );
        ctx.detach(BRUTALIZER);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        activate::activate(&mut ctx, 0, BRUTALIZER, EQUIP).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "{card 95}", "cancel"]);
        pick(&mut ctx, 0, 0).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            6,
            "434.1.f · a fresh attach resets the turn"
        );
        assert_eq!(ctx.attached_turn(BRUTALIZER), ctx.turn());
        ctx.detach(BRUTALIZER);
        assert_eq!(ctx.attach(BRUTALIZER, 95), Attached::Yes);
        assert_eq!(
            ctx.current_might(95),
            5,
            "both parts move to the new wearer"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_bonus_leaves_with_the_wearer_and_the_gear_alike() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert_eq!(attached_to(&ctx, BRUTALIZER), None);
        assert!(!is_attached(&ctx, BRUTALIZER));
        assert!(statics::grants_on(&ctx, fixtures::VI).is_empty());
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(ctx.kill(BRUTALIZER, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(might_counters(&ctx, fixtures::VI), 0);
    }

    #[test]
    fn the_other_seat_a_missing_calm_rune_and_an_enemy_wearer_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, BRUTALIZER, EQUIP),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, BRUTALIZER, EQUIP).unwrap();
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "744.1.c.2 · only a unit you control"
        );
        let cancel = fixtures::labels(&ctx).len() as u16 - 1;
        pick(&mut ctx, 0, cancel).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, BRUTALIZER));
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = armed();
        let held = broke.table.card_mut(42).unwrap();
        held.domain = vec!["Fury".into()];
        held.name = "Fury Rune".into();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, BRUTALIZER, EQUIP),
            Err(Refusal::NoPowerOf),
            "the Calm power needs a Calm rune to recycle"
        );
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == BRUTALIZER && !offer.enabled));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }
}
