use super::prelude::{equip, gear, while_attached, with_statics, CHAOS};
use super::{Card, Grant, Keyword};

pub const MIGHT_BONUS: i16 = 2;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Keyword(Keyword::Ganking), Grant::Might(MIGHT_BONUS)];

pub static CARD: Card = with_statics(
    gear(
        "Boots of Swiftness",
        &[Keyword::Equip(CHAOS)],
        &[equip(CHAOS)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attach_gear, attached_to, is_attached, Attached, FRIENDLY_UNIT};
    use crate::cards::{SelfCost, Static, TargetKind, Timing, Trigger};
    use crate::engine::ctx::{Cause, Ctx, Event, Killed, Location, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, march, play as play_engine, priority, prompts, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const BOOTS: u32 = 90;
    const THEIR_BOOTS: u32 = 91;
    const EQUIP: u8 = 0;
    const ENERGY: u8 = 3;

    fn boots(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::gear(id, fixtures::BASE, seat, "Boots of Swiftness", ENERGY);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn shod() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(boots(BOOTS, 0));
        fixture.table.cards.push(boots(THEIR_BOOTS, 1));
        for rune in [fixtures::RUNE_A, 41] {
            let held = fixture.table.card_mut(rune).unwrap();
            held.domain = vec!["Chaos".into()];
            held.name = "Chaos Rune".into();
        }
        fixture.resolve();
        fixture
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? else {
            return settle(ctx);
        };
        match (answered.why, answered.answer) {
            (PromptWhy::Target { item, .. }, Answer::Cancel) => play_engine::cancel(ctx, item),
            (PromptWhy::Target { item, spec }, _) => {
                play_engine::choose_targets(ctx, item, spec, &answered.prompt.picked)?
            }
            (why, answer) => panic!("Boots opens only target prompts: {why:?} {answer:?}"),
        }
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, BOOTS, EQUIP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
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
    fn the_script_is_an_equipment_with_ganking_as_its_effect_text_and_a_might_bonus_of_two() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Boots of Swiftness").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Boots of Swiftness");
        assert!(CARD.has_keyword(Keyword::Equip(CHAOS)));
        assert_eq!(CARD.equip_cost(), Some(CHAOS));
        assert!(!CARD.has_keyword(Keyword::Hidden));
        assert!(
            !CARD.has_keyword(Keyword::Ganking),
            "Ganking is the wearer's, not the boots'"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[0];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(CHAOS));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets.len(), 1);
        assert_eq!((equip.targets[0].min, equip.targets[0].max), (1, 1));
        assert_eq!(equip.targets[0].kind, TargetKind::Card);
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Keyword(Keyword::Ganking), Grant::Might(2)]
        ));
        assert_eq!(MIGHT_BONUS, 2);
    }

    #[test]
    fn equipping_costs_a_chaos_rune_chooses_a_friendly_unit_and_gives_it_ganking_and_two_might() {
        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        let offers = activate::offers(&ctx, 0);
        let offer = offers
            .iter()
            .find(|offer| offer.source == BOOTS)
            .unwrap_or_else(|| panic!("the loose boots offer their Equip: {offers:?}"));
        assert!(offer.enabled);
        assert_eq!(offer.label, "{card 90}: equip (1 Chaos power)");
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        activate::activate(&mut ctx, 0, BOOTS, EQUIP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "cancel"],
            "744.1.c.2 · a unit you control; the other seat's Jinx is not offered"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "744.1 · Equip is an activated ability"
        );
        assert!(
            ctx.events.contains(&Event::Chosen {
                card: fixtures::VI,
                by: 0,
                item: 1
            }),
            "744.1.b.1 · Equip's choice is a target"
        );
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "one Chaos rune recycled for the power"
        );
        assert!(!is_attached(&ctx, BOOTS), "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, BOOTS), Some(fixtures::VI));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "136.3 · the +2 Might Bonus modulates the wearer"
        );
        assert_eq!(
            might_counters(&ctx, fixtures::VI),
            2,
            "kai sees the bonus as a Might counter"
        );
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Base(0)),
            "421.4 · the boots stand where the wearer stands"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +2 Might while {card 90} is attached".to_string()));
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                crate::engine::ctx::MoveCause::Effect
            ),
            crate::engine::ctx::Moved::Moved
        );
        cleanup::run(&mut ctx, None);
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Battlefield(fixtures::BF1)),
            "719.3.a · the boots follow the wearer"
        );
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Ok(()),
            "736 · the wearer marches battlefield to battlefield"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_bonus_and_ganking_leave_with_the_boots_when_the_wearer_dies_and_the_boots_come_home() {
        let mut fixture = shod();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert_eq!(attached_to(&ctx, BOOTS), None);
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Base(0)),
            "435.1 · the boots the dead wearer left behind are recalled at cleanup"
        );
        assert!(
            ctx.state_of(fixtures::VI)
                .is_none_or(|row| row.might.is_empty() && row.granted.is_empty()),
            "136.3.a · the bonus stops applying as soon as the boots are no longer attached"
        );
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(ctx.kill(BOOTS, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the bonus dies with the boots"
        );
        assert_eq!(might_counters(&ctx, fixtures::VI), 0);
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Base(0),
                Location::Battlefield(fixtures::BF1)
            ),
            Ok(())
        );
    }

    #[test]
    fn re_equipping_moves_the_boots_and_the_bonus_to_the_new_wearer() {
        let mut fixture = shod();
        fixture
            .table
            .cards
            .push(fixtures::unit(95, fixtures::BASE, 0, "Wailer", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(
            activate::activate(&mut ctx, 0, BOOTS, EQUIP),
            Err(Refusal::Illegal(Reason::Attached)),
            "134.4 · the attached boots' own Equip is inactive"
        );
        assert!(attached_to(&ctx, BOOTS).is_some());
        ctx.detach(BOOTS);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        activate::activate(&mut ctx, 0, BOOTS, EQUIP).unwrap();
        assert_eq!(labels(&ctx), ["{card 50}", "{card 95}", "cancel"]);
        pick(&mut ctx, 0, 1).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, BOOTS), Some(95));
        assert_eq!(ctx.current_might(95), 4);
        assert!(ctx.has_keyword(95, Keyword::Ganking));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(
            ctx.runes_of(0).len(),
            2,
            "a second Chaos rune was recycled for the second Equip"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_a_missing_chaos_rune_and_attached_boots_are_refused() {
        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, BOOTS, EQUIP),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, THEIR_BOOTS, EQUIP),
            Err(Refusal::NotYourTurn),
            "744.1 · a sorcery-speed activation waits for its own turn"
        );
        activate::activate(&mut ctx, 0, BOOTS, EQUIP).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "744.1.c.2 · only a unit you control"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[BOOTS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a gear is not a unit"
        );
        assert!(ctx.blob.prompt.is_some());
        let cancel = labels(&ctx).len() as u16 - 1;
        pick(&mut ctx, 0, cancel).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, BOOTS));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "a cancelled Equip pays nothing"
        );
        drop(ctx);

        let mut broke = shod();
        for rune in [fixtures::RUNE_A, 41] {
            let held = broke.table.card_mut(rune).unwrap();
            held.domain = vec!["Calm".into()];
            held.name = "Calm Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, BOOTS, EQUIP),
            Err(Refusal::NoPowerOf),
            "744.1.c.3 · the Chaos power needs a Chaos rune to recycle"
        );
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == BOOTS && !offer.enabled));
        drop(ctx);

        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, BOOTS, EQUIP),
            Err(Refusal::Illegal(Reason::Attached))
        );
        assert!(activate::offers(&ctx, 0)
            .iter()
            .all(|offer| offer.source != BOOTS));
        assert_eq!(
            attach_gear(&mut ctx, BOOTS, fixtures::VI),
            Attached::Already,
            "the same wearer twice is a no-op, so the bonus never doubles"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 5);
    }
}
