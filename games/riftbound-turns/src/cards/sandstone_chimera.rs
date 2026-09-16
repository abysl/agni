use super::prelude::{unit, with_statics};
use super::tianna_crownguard::holds_the_line;
use super::{Card, Static};
use crate::engine::ctx::Ctx;
use crate::rules;

pub const RUNES_WHILE_PRESENT: usize = 1;

pub fn throttles_the_channel(ctx: &Ctx, me: u32) -> bool {
    ctx.script(me)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
        && holds_the_line(ctx, me)
}

pub fn a_chimera_stands_at_a_battlefield(ctx: &Ctx) -> bool {
    let mut chimeras: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| throttles_the_channel(ctx, held.id))
        .map(|held| held.id)
        .collect();
    chimeras.sort_unstable();
    !chimeras.is_empty()
}

pub fn runes_this_turn(ctx: &Ctx) -> usize {
    if a_chimera_stands_at_a_battlefield(ctx) {
        RUNES_WHILE_PRESENT
    } else {
        rules::runes_this_turn(ctx.blob)
    }
}

pub fn channel_count(ctx: &Ctx, _seat: u8) -> u8 {
    u8::try_from(runes_this_turn(ctx)).unwrap_or(u8::MAX)
}

pub static CARD: Card = with_statics(
    unit("Sandstone Chimera", &[], &[]),
    &[Static::ChannelCount(channel_count)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{move_unit, Location, Moved};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::phases;
    use crate::state::{GameBlob, Mode, Phase};
    use agni_plugin_sdk::table::CardInfo;

    const CHIMERA: u32 = 90;

    fn chimera(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(7),
            power: Some(2),
            domain: vec!["Calm".into()],
            ..fixtures::unit(CHIMERA, zone, seat, "Sandstone Chimera", 8)
        }
    }

    fn dunes(zone: u16, seat: u8, turn: u16, player: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.blob.core_mut().unwrap().turn = turn;
        fixture.blob.core_mut().unwrap().player = player;
        fixture.blob.set_holder(fixtures::BF1, Some(seat));
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.cards.push(chimera(zone, seat));
        for (id, owner) in [(36, 0), (37, 0), (38, 0), (39, 1), (46, 1), (47, 1)] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::RUNE_DECK, owner));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CHIMERA).unwrap(),
            &CARD
        ));
        fixture
    }

    fn pool_of(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, seat).count()
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_channel_rule_is_a_static() {
        assert!(std::ptr::eq(script_of("Sandstone Chimera").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(CARD.statics, [Static::ChannelCount(_)]));
        assert_eq!(RUNES_WHILE_PRESENT, 1);
    }

    #[test]
    fn she_throttles_from_a_battlefield_for_both_players_and_never_from_a_base_or_a_hand() {
        let mut fixture = dunes(fixtures::BASE, 0, 2, 1);
        let mut ctx = fixture.ctx();
        assert!(!throttles_the_channel(&ctx, CHIMERA));
        assert!(!a_chimera_stands_at_a_battlefield(&ctx));
        assert_eq!(runes_this_turn(&ctx), rules::runes_this_turn(ctx.blob));
        assert_eq!(runes_this_turn(&ctx), 3, "the second player's first turn");
        assert_eq!(
            move_unit(
                &mut ctx,
                &fixtures::effect_of(0),
                CHIMERA,
                Location::Battlefield(fixtures::BF1)
            ),
            Some(Moved::Moved)
        );
        assert!(throttles_the_channel(&ctx, CHIMERA));
        assert!(a_chimera_stands_at_a_battlefield(&ctx));
        assert_eq!(
            runes_this_turn(&ctx),
            1,
            "players, not her controller alone"
        );
        assert!(
            !throttles_the_channel(&ctx, fixtures::VI),
            "another unit at a battlefield is not a Chimera"
        );
        ctx.recall(CHIMERA, true);
        assert!(!a_chimera_stands_at_a_battlefield(&ctx));
        assert_eq!(runes_this_turn(&ctx), 3);
        drop(ctx);
        let mut pocketed = dunes(fixtures::HAND, 1, 3, 0);
        let ctx = pocketed.ctx();
        assert!(!throttles_the_channel(&ctx, CHIMERA));
        assert_eq!(runes_this_turn(&ctx), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn while_she_stands_at_a_battlefield_each_channel_phase_deals_one_rune() {
        let mut fixture = dunes(fixtures::BF1, 0, 2, 1);
        let mut ctx = fixture.ctx();
        let before = pool_of(&ctx, 1);
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert_eq!(
            pool_of(&ctx, 1),
            before + RUNES_WHILE_PRESENT,
            "the opponent's first turn channels one rune, not three"
        );
        drop(ctx);
        let mut hers = dunes(fixtures::BF1, 0, 3, 0);
        let mut ctx = hers.ctx();
        let before = pool_of(&ctx, 0);
        phases::start_turn(&mut ctx);
        assert_eq!(pool_of(&ctx, 0), before + RUNES_WHILE_PRESENT);
    }
}
