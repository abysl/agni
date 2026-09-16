use super::prelude::{attached_to, equip, gear, while_attached, with_statics};
use super::{Ability, Card, Cost, Domain, Grant, Keyword, Power};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Calm)],
};

pub const MIGHT_BONUS: i16 = 0;

pub fn copied_text(ctx: &Ctx, gear: u32) -> Option<&'static Card> {
    let wearer = attached_to(ctx, gear)?;
    ctx.script(wearer)
}

fn copied_abilities(ctx: &Ctx, gear: u32) -> &'static [Ability] {
    copied_text(ctx, gear)
        .map(|script| script.abilities)
        .unwrap_or(&[])
}

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Copied(copied_abilities)];

pub static CARD: Card = with_statics(
    gear("Svellsongur", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{is_attached, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger, GRANTED};
    use crate::engine::ctx::{Location, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, play as play_engine, priority, triggers};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const SVELLSONGUR: u32 = 90;
    const AHRI: u32 = 95;
    const BLITZCRANK: u32 = 96;
    const EQUIP_INDEX: u8 = 0;

    fn svellsongur(seat: u8) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::gear(SVELLSONGUR, fixtures::BASE, seat, "Svellsongur", 3)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(svellsongur(0));
        fixture.table.cards.push(fixtures::unit(
            AHRI,
            fixtures::BASE,
            0,
            "Ahri - Alluring",
            4,
        ));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SVELLSONGUR).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip(ctx: &mut Ctx, wearer: u32) {
        activate::activate(ctx, 0, SVELLSONGUR, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, &format!("{{card {wearer}}}")).unwrap();
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
    fn the_script_is_a_one_energy_calm_equipment_with_no_bonus_that_copies_the_wearers_text() {
        assert!(std::ptr::eq(script_of("Svellsongur").unwrap(), &CARD));
        assert_eq!(CARD.name, "Svellsongur");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(EQUIP.energy, 1);
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[0];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(0), Grant::Copied(_)]
        ));
        assert_eq!(MIGHT_BONUS, 0);
    }

    #[test]
    fn equipping_costs_one_energy_and_a_calm_rune_and_the_copied_text_is_the_wearers_script() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(
            copied_text(&ctx, SVELLSONGUR).is_none(),
            "unattached, nothing is copied"
        );
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == SVELLSONGUR)
            .unwrap();
        assert!(offer.enabled);
        assert_eq!(offer.label, "{card 90}: equip (1 energy and 1 Calm power)");
        equip(&mut ctx, AHRI);
        assert_eq!(attached_to(&ctx, SVELLSONGUR), Some(AHRI));
        assert_eq!(ctx.runes_of(0).len(), 3, "the Calm rune recycled");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "one rune exhausted for the energy"
        );
        assert_eq!(ctx.current_might(AHRI), 4, "+0 · no bonus");
        assert_eq!(might_counters(&ctx, AHRI), 0);
        assert!(std::ptr::eq(
            copied_text(&ctx, SVELLSONGUR).unwrap(),
            script_of("Ahri - Alluring").unwrap()
        ));
        assert_eq!(ctx.location(SVELLSONGUR), Some(Location::Base(0)));
        ctx.detach(SVELLSONGUR);
        assert!(copied_text(&ctx, SVELLSONGUR).is_none());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_a_missing_calm_rune_and_an_attached_svellsongur_are_refused()
    {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, SVELLSONGUR, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, SVELLSONGUR, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, SVELLSONGUR));
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "a cancelled Equip pays nothing"
        );
        drop(ctx);

        let mut broke = armed();
        {
            let held = broke.table.card_mut(42).unwrap();
            held.domain = vec!["Fury".into()];
            held.name = "Fury Rune".into();
        }
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SVELLSONGUR, EQUIP_INDEX),
            Err(Refusal::NoPowerOf)
        );
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        equip(&mut ctx, AHRI);
        assert_eq!(
            activate::activate(&mut ctx, 0, SVELLSONGUR, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }

    fn held_by(unit: u32) -> crate::engine::ctx::Event {
        crate::engine::ctx::Event::Held {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![unit],
        }
    }

    #[test]
    fn while_attached_the_wearer_has_its_own_text_twice() {
        let mut fixture = armed();
        fixture.table.card_mut(AHRI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        equip(&mut ctx, AHRI);
        let found = triggers::find(&ctx, &held_by(AHRI));
        assert_eq!(
            found.len(),
            2,
            "Ahri's own hold and the copy Svellsongur appends"
        );
        assert_eq!(
            found[1].kind(),
            ItemKind::Granted {
                holder: AHRI,
                lender: SVELLSONGUR,
                index: GRANTED
            },
            "136.2.c · the copy of her first ability is appended to Ahri, lent by the gear"
        );
        ctx.raise(held_by(AHRI));
        crate::engine::settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OrderTriggers { seat: 0 })
        ));
        let first = crate::engine::fixtures::labels(&ctx)[0].clone();
        crate::engine::fixtures::choose(&mut ctx, 0, &first).unwrap();
    }

    #[test]
    fn the_copied_text_resolves_as_the_wearer_so_blitzcranks_hold_returns_him_and_not_the_gear() {
        let mut fixture = armed();
        fixture.table.cards.push(fixtures::unit(
            BLITZCRANK,
            fixtures::BF1,
            0,
            "Blitzcrank - Impassive",
            5,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        equip(&mut ctx, BLITZCRANK);
        assert_eq!(
            ctx.location(SVELLSONGUR),
            Some(Location::Battlefield(fixtures::BF1))
        );
        ctx.raise(held_by(BLITZCRANK));
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.queue.len(), 2, "his own hold and the copy");
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OrderTriggers { seat: 0 })),
            "two copies remain ordered even when their callbacks are identical"
        );
        let first = crate::engine::fixtures::labels(&ctx)[0].clone();
        crate::engine::fixtures::choose(&mut ctx, 0, &first).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.in_hand(BLITZCRANK),
            "136.2.c · \"me\" in the copied text is Blitzcrank"
        );
        assert!(
            ctx.on_board(SVELLSONGUR),
            "136.2.d · the gear is not \"me\""
        );
        assert!(!is_attached(&ctx, SVELLSONGUR));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| *line == "{card 96} returns to his owner's hand")
                .count(),
            1,
            "the second hold finds him already home"
        );
        assert!(ctx.fault.is_none());
    }
}
