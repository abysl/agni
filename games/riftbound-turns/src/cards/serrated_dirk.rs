use super::prelude::{equip, gear, while_attached, with_statics};
use super::{Card, Cost, Domain, Grant, Keyword, Power};

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};

pub const ASSAULT: u8 = 2;
pub const MIGHT_BONUS: i16 = 0;

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Keyword(Keyword::Assault(ASSAULT)),
    Grant::Might(MIGHT_BONUS),
];

pub static CARD: Card = with_statics(
    gear("Serrated Dirk", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
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
    use crate::state::{PromptWhy, FLAG_ATTACKER};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const DIRK: u32 = 90;
    const EQUIP_INDEX: u8 = 0;

    fn dirk(seat: u8) -> CardInfo {
        let mut card = fixtures::gear(DIRK, fixtures::BASE, seat, "Serrated Dirk", 1);
        card.domain = vec!["Fury".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dirk(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(DIRK).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, DIRK, EQUIP_INDEX).unwrap();
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
    fn the_script_is_a_fury_equipment_whose_effect_text_is_assault_two_with_no_might_bonus() {
        assert!(std::ptr::eq(script_of("Serrated Dirk").unwrap(), &CARD));
        assert_eq!(CARD.name, "Serrated Dirk");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert!(CARD.is_equipment());
        assert!(
            !CARD.has_keyword(Keyword::Assault(ASSAULT)),
            "Assault is the wearer's, not the dirk's"
        );
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[0];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets.len(), 1);
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Keyword(Keyword::Assault(2)), Grant::Might(0)]
        ));
        assert_eq!((ASSAULT, MIGHT_BONUS), (2, 0));
    }

    #[test]
    fn equipping_costs_a_fury_rune_and_the_wearer_has_assault_two_while_it_attacks() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let offers = activate::offers(&ctx, 0);
        assert!(offers
            .iter()
            .any(|offer| offer.source == DIRK && offer.enabled));
        activate::activate(&mut ctx, 0, DIRK, EQUIP_INDEX).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "818.1.c.2 · a unit you control"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "one Fury rune recycled for the power"
        );
        assert!(!is_attached(&ctx, DIRK), "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, DIRK), Some(fixtures::VI));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Assault(ASSAULT)));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "+0 · the badge adds nothing outside an attack"
        );
        assert_eq!(
            might_counters(&ctx, fixtures::VI),
            0,
            "a zero bonus writes no Might counter"
        );
        ctx.set_flag(fixtures::VI, FLAG_ATTACKER, true);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "803 · Assault 2 while the wearer is an attacker"
        );
        ctx.set_flag(fixtures::VI, FLAG_ATTACKER, false);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.location(DIRK), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn assault_leaves_with_the_dirk_when_it_is_detached_or_killed() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        ctx.set_flag(fixtures::VI, FLAG_ATTACKER, true);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        ctx.detach(DIRK);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Assault(ASSAULT)));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(ctx.kill(DIRK, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Assault(ASSAULT)));
        assert_eq!(attached_to(&ctx, DIRK), None);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_a_missing_fury_rune_and_an_attached_dirk_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, DIRK, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, DIRK, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "818.1.c.2 · only a unit you control"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[DIRK]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a gear is not a unit"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "a cancelled Equip pays nothing"
        );
        drop(ctx);

        let mut broke = armed();
        for rune in [fixtures::RUNE_A, 41, 43] {
            let held = broke.table.card_mut(rune).unwrap();
            held.domain = vec!["Calm".into()];
            held.name = "Calm Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, DIRK, EQUIP_INDEX),
            Err(Refusal::NoPowerOf),
            "the Fury power needs a Fury rune to recycle"
        );
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, DIRK, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached)),
            "434.1.e · the attached dirk's own Equip is inactive"
        );
    }
}
