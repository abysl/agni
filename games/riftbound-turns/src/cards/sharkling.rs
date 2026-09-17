use super::prelude::unit;
use super::{Card, Keyword};

pub const ASSAULT: u8 = 4;

pub static CARD: Card = unit(
    "Sharkling",
    &[Keyword::Accelerate, Keyword::Assault(ASSAULT)],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::cost;
    use crate::engine::ctx::{Ctx, EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, play as play_engine, prompts, settle};
    use crate::state::{Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const SHARKLING: u32 = 90;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 1;
    const ACCELERATED: usize = ENERGY as usize + 1;
    const EXTRA_RUNES: [u32; 2] = [100, 101];

    fn sharkling(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            ..fixtures::unit(SHARKLING, zone, seat, "Sharkling", MIGHT)
        }
    }

    fn in_hand(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sharkling(fixtures::HAND, 0));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        let runes: Vec<u32> = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.is_kind("Rune") && card.owner == 0)
            .map(|card| card.id)
            .collect();
        for (index, rune) in runes.into_iter().enumerate() {
            fixture.table.card_mut(rune).unwrap().exhausted = index >= ready;
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SHARKLING).unwrap(),
            &CARD
        ));
        fixture
    }

    fn entry(ctx: &Ctx) -> EntryMove {
        EntryMove {
            card: SHARKLING,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(fixtures::BASE),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn play_to_base(ctx: &mut Ctx) -> Result<(), Refusal> {
        legal::classify(ctx, 0, &entry(ctx))?;
        play_engine::begin(ctx, 0, SHARKLING, Origin::Hand, Some(Location::Base(0)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
    }

    #[test]
    fn the_script_is_an_accelerate_assault_four_unit_with_no_abilities() {
        assert!(std::ptr::eq(script_of("Sharkling").unwrap(), &CARD));
        assert_eq!(CARD.name, "Sharkling");
        assert_eq!(CARD.keywords, &[Keyword::Accelerate, Keyword::Assault(4)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = in_hand(ACCELERATED);
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(SHARKLING, Keyword::Accelerate));
        assert!(ctx.has_keyword(SHARKLING, Keyword::Assault(4)));
        assert!(cost::can_accelerate(&ctx, SHARKLING));
    }

    #[test]
    fn accelerated_it_enters_ready_and_swings_for_five_as_an_attacker() {
        let mut fixture = in_hand(ACCELERATED);
        let action = fixtures::move_action(SHARKLING, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to_base(&mut ctx).unwrap();
        let why = ctx.blob.why;
        assert!(
            matches!(why, Some(PromptWhy::OptionalCost { .. })),
            "731.2 · Accelerate is an optional additional cost: {why:?}"
        );
        assert_eq!(
            prompts::status(&ctx, why.unwrap()),
            format!("accelerate {{card {SHARKLING}}} for 1 energy and 1 Fury power?")
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.location(SHARKLING), Some(Location::Base(0)));
        assert!(
            !ctx.card(SHARKLING).unwrap().exhausted,
            "731.6 · it enters ready"
        );
        assert!(ctx.ready_runes_of(0).is_empty());
        assert!(ctx.blob.chain.is_empty(), "no play trigger");
        assert_eq!(ctx.current_might(SHARKLING), i32::from(MIGHT));
        assert!(ctx.mark_attacker(SHARKLING));
        assert_eq!(
            ctx.current_might(SHARKLING),
            i32::from(MIGHT + ASSAULT),
            "732 · +4 while an attacker"
        );
        ctx.clear_designation(SHARKLING);
        assert!(ctx.mark_defender(SHARKLING));
        assert_eq!(ctx.current_might(SHARKLING), i32::from(MIGHT));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn declining_the_accelerate_enters_exhausted_and_a_short_seat_is_refused() {
        let mut fixture = in_hand(ACCELERATED);
        let action = fixtures::move_action(SHARKLING, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_to_base(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.card(SHARKLING).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "one rune was spared");
        drop(ctx);
        let mut short = in_hand(usize::from(ENERGY) - 1);
        let mut ctx = short.ctx_for(0, &action);
        assert_eq!(
            play_to_base(&mut ctx),
            Err(Refusal::NotEnoughRunes {
                needed: ENERGY,
                ready: ENERGY - 1
            })
        );
        assert!(ctx.blob.queue.is_empty(), "the play never became pending");
        assert!(ctx.effects.is_empty(), "nothing was paid");
    }
}
