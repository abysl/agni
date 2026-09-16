use super::frisky_hunter::play_birds;
use super::prelude::{deathknell, done, unit, Location};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const BIRDS: usize = 1;

fn dredge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    play_birds(ctx, seat, Location::Base(seat), BIRDS);
    done()
}

pub static CARD: Card = unit(
    "Carrion Dredger",
    &[Keyword::Deathknell],
    &[deathknell(&[], dredge)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::frisky_hunter::tests::birds_of;
    use crate::cards::frisky_hunter::{is_bird, BIRD_MIGHT};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const DREDGER: u32 = 90;

    fn dredger(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(DREDGER, zone, seat, "Carrion Dredger", 1);
        card.domain = vec!["Order".into()];
        card.energy = Some(2);
        card
    }

    fn scrapyard(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dredger(zone, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DREDGER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_deathknell_unit_with_one_targetless_death_trigger() {
        assert!(std::ptr::eq(script_of("Carrion Dredger").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Death);
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert!(ability.targets.is_empty(), "the Bird goes to your base");
        assert_eq!(BIRDS, 1);
    }

    #[test]
    fn dying_at_a_battlefield_it_plays_an_exhausted_bird_with_deflect_to_its_controllers_base() {
        let mut fixture = scrapyard(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(DREDGER, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(!ctx.on_board(DREDGER));
        assert_eq!(ctx.card(DREDGER).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DREDGER
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.prompt.is_none(), "it chooses nothing");
        assert!(birds_of(&ctx, 0).is_empty(), "the Bird waits for the chain");
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let birds = birds_of(&ctx, 0);
        assert_eq!(birds, [next]);
        let bird = birds[0];
        assert!(is_bird(&ctx, bird));
        assert_eq!(
            ctx.location(bird),
            Some(Location::Base(0)),
            "your base, not where the Dredger died"
        );
        assert!(ctx.is_token(bird));
        assert_eq!(ctx.current_might(bird), i32::from(BIRD_MIGHT));
        assert_eq!(ctx.deflect_of(bird), 1);
        assert!(
            ctx.card(bird).unwrap().exhausted,
            "185.2.d · it enters exhausted"
        );
        assert_eq!(ctx.controller(bird), 0);
        assert!(birds_of(&ctx, 1).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {bird}}} to their base")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn killed_by_an_enemy_the_bird_still_goes_to_its_own_controllers_base() {
        let mut fixture = scrapyard(fixtures::BASE);
        let mut ctx = fixture.ctx();
        ctx.actor = 1;
        assert_eq!(ctx.kill(DREDGER, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain[0].controller, 0);
        resolve_chain(&mut ctx);
        let bird = *birds_of(&ctx, 0).first().expect("the Deathknell's Bird");
        assert_eq!(ctx.location(bird), Some(Location::Base(0)));
        assert!(birds_of(&ctx, 1).is_empty(), "never the killer's");
    }

    #[test]
    fn a_dredger_that_leaves_without_dying_plays_nothing() {
        let mut fixture = scrapyard(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(ctx.bounce(DREDGER));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(DREDGER).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty(), "a bounce is not a death");
        assert!(birds_of(&ctx, 0).is_empty());
    }
}
