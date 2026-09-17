use super::prelude::unit;
use super::{Card, Keyword};

pub const ASSAULT: u8 = 2;

pub static CARD: Card = unit("Laurent Duelist", &[Keyword::Assault(ASSAULT)], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Ctx, EntryMove, Location, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, combat, legal, play as play_engine, settle, showdown};
    use crate::state::{GameBlob, Mode, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const DUELIST: u32 = 90;
    const NOBODY: u32 = 91;
    const ENERGY: u8 = 4;
    const MIGHT: u8 = 3;

    fn duelist(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Order".into()],
            ..fixtures::unit(DUELIST, zone, seat, "Laurent Duelist", MIGHT)
        }
    }

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture
    }

    fn attacking_into(defender_might: u8) -> Fixture {
        let mut fixture = arena();
        fixture.table.cards.push(duelist(fixtures::BF1, 0));
        fixture.table.cards.push(fixtures::unit(
            NOBODY,
            fixtures::BF1,
            1,
            "Nobody",
            defender_might,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn defending_against(attacker_might: u8) -> Fixture {
        let mut fixture = arena();
        fixture.table.cards.push(duelist(fixtures::BF1, 1));
        fixture.table.cards.push(fixtures::unit(
            NOBODY,
            fixtures::BF1,
            0,
            "Nobody",
            attacker_might,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn open_showdown(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        assert!(
            ctx.blob.showdown.is_some(),
            "the contested battlefield opens a showdown"
        );
    }

    fn fight(ctx: &mut Ctx) {
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(ctx.blob.showdown.is_none());
    }

    fn damage_dealt(ctx: &Ctx, card: u32) -> i32 {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Counter {
                    target: Target::Card(held),
                    counter,
                    delta,
                } if *held == card && *counter == COUNTER_DAMAGE && *delta > 0 => Some(*delta),
                _ => None,
            })
            .sum()
    }

    fn play_to_base(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        let entry = EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: Some(fixtures::BASE),
            to_seat: seat,
            index: TOP,
            hidden: false,
        };
        legal::classify(ctx, seat, &entry)?;
        play_engine::begin(ctx, seat, card, Origin::Hand, Some(Location::Base(seat)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    #[test]
    fn the_script_is_an_assault_two_unit_with_no_abilities() {
        assert!(std::ptr::eq(script_of("Laurent Duelist").unwrap(), &CARD));
        assert_eq!(CARD.name, "Laurent Duelist");
        assert_eq!(CARD.keywords, &[Keyword::Assault(2)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = attacking_into(MIGHT);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DUELIST).unwrap(),
            &CARD
        ));
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(DUELIST, Keyword::Assault(2)));
        assert_eq!(
            ctx.current_might(DUELIST),
            i32::from(MIGHT),
            "no bonus outside combat"
        );
    }

    #[test]
    fn as_an_attacker_it_has_two_more_might_and_kills_a_defender_two_bigger_than_itself() {
        let mut fixture = attacking_into(MIGHT + ASSAULT);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_attacker(DUELIST));
        assert_eq!(
            ctx.current_might(DUELIST),
            i32::from(MIGHT + ASSAULT),
            "732 · +2 might while attacking"
        );
        assert_eq!(combat::lethal(&ctx, DUELIST), MIGHT + ASSAULT);
        assert_eq!(combat::might_sum(&ctx, &[DUELIST]), MIGHT + ASSAULT);
        fight(&mut ctx);
        assert_eq!(damage_dealt(&ctx, NOBODY), i32::from(MIGHT + ASSAULT));
        assert_eq!(
            ctx.card(NOBODY).unwrap().zone,
            Some(fixtures::TRASH),
            "five damage is lethal on five might"
        );
        assert_eq!(damage_dealt(&ctx, DUELIST), i32::from(MIGHT + ASSAULT));
        assert_eq!(ctx.card(DUELIST).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn as_a_defender_the_assault_adds_nothing_and_it_trades_with_an_equal_attacker() {
        let mut fixture = defending_against(MIGHT);
        let mut ctx = fixture.ctx();
        open_showdown(&mut ctx);
        assert!(ctx.is_defender(DUELIST));
        assert_eq!(ctx.current_might(DUELIST), i32::from(MIGHT));
        assert_eq!(combat::lethal(&ctx, DUELIST), MIGHT);
        fight(&mut ctx);
        assert_eq!(damage_dealt(&ctx, DUELIST), i32::from(MIGHT));
        assert_eq!(damage_dealt(&ctx, NOBODY), i32::from(MIGHT));
        assert_eq!(ctx.card(DUELIST).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(NOBODY).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn played_from_hand_it_enters_exhausted_and_the_play_is_refused_when_short() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(duelist(fixtures::HAND, 0));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        let action = fixtures::move_action(DUELIST, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to_base(&mut ctx, 0, DUELIST).unwrap();
        assert_eq!(ctx.location(DUELIST), Some(Location::Base(0)));
        assert!(ctx.card(DUELIST).unwrap().exhausted);
        assert!(ctx.ready_runes_of(0).is_empty());
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert!(ctx.blob.prompt.is_none());
        drop(ctx);
        let mut short = Fixture::enforced();
        short.table.cards.push(duelist(fixtures::HAND, 0));
        short.resolve();
        let mut ctx = short.ctx_for(0, &action);
        assert_eq!(
            play_to_base(&mut ctx, 0, DUELIST),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: ENERGY - 1
            })
        );
        assert!(ctx.blob.queue.is_empty(), "the play never became pending");
        assert!(ctx.effects.is_empty(), "nothing was paid");
    }
}
