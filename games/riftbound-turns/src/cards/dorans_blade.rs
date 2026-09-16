use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power};

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body)],
};

pub const MIGHT_BONUS: i16 = 2;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

pub static CARD: Card = with_statics(
    gear("Doran's Blade", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, is_attached, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger};
    use crate::engine::ctx::{Cause, Ctx, Killed, Location, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, play as play_engine, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const GEAR: u32 = 90;
    const WAILER: u32 = 95;
    const EQUIP_INDEX: u8 = 0;

    fn the_gear(seat: u8) -> CardInfo {
        let mut card = fixtures::gear(GEAR, fixtures::BASE, seat, "Doran's Blade", 2);
        card.domain = vec!["Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(the_gear(0));
        {
            let held = fixture.table.card_mut(42).unwrap();
            held.domain = vec!["Body".into()];
            held.name = "Body Rune".into();
        }
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
    fn the_script_is_a_plain_equipment_whose_whole_effect_text_is_a_might_bonus_of_2() {
        assert!(std::ptr::eq(script_of("Doran's Blade").unwrap(), &CARD));
        assert_eq!(CARD.name, "Doran's Blade");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert!(CARD.is_equipment());
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[0];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets.len(), 1);
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(CARD.attached_grants(), [Grant::Might(2)]));
        assert_eq!(MIGHT_BONUS, 2);
    }

    #[test]
    fn equipping_costs_the_rune_chooses_a_friendly_unit_and_gives_it_the_bonus() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == GEAR && offer.enabled));
        activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.runes_of(0).len(), 3, "one rune recycled for the power");
        assert!(!is_attached(&ctx, GEAR), "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, GEAR), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 3 + MIGHT_BONUS as i32);
        assert_eq!(might_counters(&ctx, fixtures::VI), MIGHT_BONUS as i32);
        assert!(ctx.blob.log.contains(&format!(
            "{{card 50}} gets +{MIGHT_BONUS} Might while {{card {GEAR}}} is attached"
        )));
        assert_eq!(ctx.location(GEAR), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_bonus_moves_with_the_gear_and_dies_with_the_wearer() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::unit(WAILER, fixtures::BASE, 0, "Wailer", 2));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Body", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        ctx.detach(GEAR);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "{card 95}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 95}").unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, GEAR), Some(WAILER));
        assert_eq!(ctx.current_might(WAILER), 2 + MIGHT_BONUS as i32);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.kill(WAILER, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert_eq!(attached_to(&ctx, GEAR), None);
        assert_eq!(ctx.location(GEAR), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_a_missing_rune_and_an_attached_gear_are_refused() {
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
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[GEAR]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = Fixture::enforced();
        broke.table.cards.push(the_gear(0));
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, GEAR, EQUIP_INDEX),
            Err(Refusal::NoPowerOf),
            "seat 0 holds no Body rune"
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
}
