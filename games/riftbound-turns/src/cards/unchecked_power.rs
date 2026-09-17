use super::prelude::{deal, friendly_units, play, spell, units_at_battlefields};
use super::{Card, Flow};

const DAMAGE: u8 = 12;

pub static CARD: Card = spell(
    "Unchecked Power",
    &[],
    &[play(&[], |ctx, item, _| {
        let exhausted = friendly_units(ctx, item.controller)
            .into_iter()
            .filter(|unit| ctx.exhaust(*unit))
            .count();
        let struck = units_at_battlefields(ctx)
            .into_iter()
            .filter(|unit| deal(ctx, item, *unit, DAMAGE))
            .count();
        ctx.narrate(format!(
            "{{card {}}} exhausts {exhausted} friendly units and deals {DAMAGE} to {struck} units at battlefields",
            item.kind.source()
        ));
        Flow::Done
    })],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Keyword;
    use crate::engine::ctx::COUNTER_DAMAGE;
    use crate::engine::ctx::{Cause, Ctx, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play, priority, settle};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::Target;

    const POWER: u32 = 90;
    const THEIR_POWER: u32 = 91;
    const GUARD: u32 = 92;
    const SLEEPER: u32 = 93;
    const MIND_A: u32 = 94;
    const MIND_B: u32 = 95;
    const EXTRA_RUNES: [u32; 4] = [96, 97, 98, 99];

    fn power_card(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Unchecked Power", 7, 2);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(power_card(POWER, 0));
        fixture.table.cards.push(power_card(THEIR_POWER, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(GUARD, fixtures::BF1, 0, "Guard", 4));
        let mut sleeper = fixtures::unit(SLEEPER, fixtures::BASE, 0, "Sleeper", 1);
        sleeper.exhausted = true;
        fixture.table.cards.push(sleeper);
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_A, 0, "Mind", true));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_B, 0, "Mind", true));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play::begin(ctx, seat, card, crate::state::Origin::Hand, None)?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn damage_on(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_sorcery_speed_spell_with_one_untargeted_play_ability() {
        assert_eq!(CARD.name, "Unchecked Power");
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(std::ptr::eq(
            crate::cards::script_of("Unchecked Power").unwrap(),
            &CARD
        ));
    }

    #[test]
    fn it_exhausts_every_friendly_unit_then_sweeps_the_battlefields_for_twelve() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, POWER).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| matches!(effect, Effect::Move { card, .. } if [MIND_A, MIND_B].contains(card)))
                .count(),
            2,
            "both Mind runes are recycled for power: {:?}",
            ctx.effects
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "seven energy spent every ready rune"
        );
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(!ctx.card(GUARD).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(ctx.card(POWER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.effects.contains(&Effect::exhaust(fixtures::VI)));
        assert!(ctx.effects.contains(&Effect::exhaust(GUARD)));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| **effect == Effect::exhaust(SLEEPER))
                .count(),
            0,
            "an already exhausted unit is left alone"
        );
        assert!(!ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: GUARD,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::SPRITE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(
            !ctx.events.iter().any(|event| matches!(
                event,
                Event::DamageDealt { card, .. } if *card == fixtures::VI || *card == fixtures::THEIR_UNIT || *card == SLEEPER
            )),
            "units in bases are untouched"
        );
        assert_eq!(damage_on(&ctx, fixtures::VI), 0);
        assert_eq!(damage_on(&ctx, fixtures::THEIR_UNIT), 0);
        assert_eq!(ctx.card(GUARD).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert!(ctx.effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::BASE));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::BASE)
        );
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
        assert!(ctx.blob.log.contains(
            &"{card 90} exhausts 2 friendly units and deals 12 to 2 units at battlefields"
                .to_string()
        ));
        let line = |wanted: &str| {
            ctx.blob
                .log
                .iter()
                .position(|held| held == wanted)
                .unwrap_or_else(|| panic!("{wanted} missing from {:?}", ctx.blob.log))
        };
        assert!(line("{card 90} resolves") < line("{card 92} dies"));
        assert!(line("{card 90} resolves") < line("{card 60} dies"));
        assert!(ctx.blob.seat(0).played_main);
    }

    #[test]
    fn it_is_refused_on_a_closed_chain_on_the_other_seats_turn_and_without_seven_runes() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_POWER)),
            Err(Refusal::NotYourTurn)
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, POWER)),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "no Reaction keyword, so it cannot join a chain"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_POWER)),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.is_neutral_open());
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert_eq!(damage_on(&ctx, fixtures::SPRITE), 0);
        let mut poor = Fixture::enforced();
        poor.table.cards.push(power_card(POWER, 0));
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, POWER)),
            Err(Refusal::NotEnoughRunes {
                needed: 7,
                ready: 3
            })
        );
    }
}
