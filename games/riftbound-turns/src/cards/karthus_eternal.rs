use super::prelude::{friendly_units, unit};
use super::Card;
use crate::engine::ctx::Ctx;

pub const EXTRA_TRIGGERS: usize = 1;

pub fn extra_deathknell_triggers_until_triggers_deathknells_queues_each_match_again(
    ctx: &Ctx,
    seat: u8,
) -> usize {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|held| {
            ctx.script(*held)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .count()
        * EXTRA_TRIGGERS
}

pub static CARD: Card = unit("Karthus - Eternal", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use agni_plugin_sdk::table::CardInfo;

    const KARTHUS: u32 = 90;
    const SECOND_KARTHUS: u32 = 91;
    const THEIR_KARTHUS: u32 = 92;
    const SENTRY: u32 = 93;

    fn karthus(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Karthus - Eternal", 3);
        card.energy = Some(3);
        card.power = Some(1);
        card.domain = vec!["Order".into()];
        card
    }

    fn requiem() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(karthus(KARTHUS, fixtures::BASE, 0));
        let mut sentry = fixtures::unit(SENTRY, fixtures::BASE, 0, "Watchful Sentry", 1);
        sentry.domain = vec!["Mind".into()];
        fixture.table.cards.push(sentry);
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_stub_is_the_pool_name_with_no_keywords_and_the_doubling_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Karthus - Eternal").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(EXTRA_TRIGGERS, 1);
        let fixture = requiem();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(KARTHUS).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn the_seam_counts_one_extra_trigger_per_karthus_its_seat_controls_on_the_board() {
        let mut fixture = requiem();
        fixture
            .table
            .cards
            .push(karthus(THEIR_KARTHUS, fixtures::BASE, 1));
        fixture
            .table
            .cards
            .push(karthus(SECOND_KARTHUS, fixtures::HAND, 0));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            extra_deathknell_triggers_until_triggers_deathknells_queues_each_match_again(&ctx, 0),
            1
        );
        assert_eq!(
            extra_deathknell_triggers_until_triggers_deathknells_queues_each_match_again(&ctx, 1),
            1,
            "their Karthus doubles their Deathknells, not yours"
        );
        drop(ctx);
        fixture.table.card_mut(SECOND_KARTHUS).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            extra_deathknell_triggers_until_triggers_deathknells_queues_each_match_again(&ctx, 0),
            2,
            "two Karthus, two extra times"
        );
    }

    #[test]
    fn a_dead_karthus_counts_for_nothing() {
        let mut fixture = requiem();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(KARTHUS, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "no Deathknell of his own");
        assert_eq!(
            extra_deathknell_triggers_until_triggers_deathknells_queues_each_match_again(&ctx, 0),
            0
        );
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(SENTRY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the sentry's one Deathknell");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
    }

    #[test]
    #[ignore = "engine gap · triggers::deathknells queues each Death match once; with Karthus on the board it should queue each of his controller's Deathknells an additional time"]
    fn with_karthus_out_a_friendly_deathknell_is_queued_twice_and_the_sentry_draws_two() {
        let mut fixture = requiem();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.kill(SENTRY, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2, "the Deathknell and its echo");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 2);
    }
}
