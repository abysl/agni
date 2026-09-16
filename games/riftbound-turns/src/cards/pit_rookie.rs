use super::prelude::{a_card, buff, card_target, done, play, unit, ANOTHER_FRIENDLY_UNIT_THAN_ME};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn rookie(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(target) = card_target(ctx, item, 0) {
        if buff(ctx, target) {
            ctx.narrate(format!("{{card {target}}} is buffed"));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Pit Rookie",
    &[],
    &[play(
        &[a_card(
            ANOTHER_FRIENDLY_UNIT_THAN_ME,
            "another friendly unit to buff",
        )],
        rookie,
    )],
);

#[cfg(test)]
mod tests {
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::{Origin, TargetRef};

    const ROOKIE: u32 = 90;

    fn rookie_fixture() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(ROOKIE, fixtures::HAND, 0, "Pit Rookie", 2));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_rookie_buffs_another_friendly_unit_and_never_itself() {
        let mut fixture = rookie_fixture();
        let action = fixtures::move_action(ROOKIE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, ROOKIE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the only other friendly unit is picked without asking"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(fixtures::VI));
        assert!(!ctx.is_buffed(ROOKIE));
    }

    #[test]
    fn a_rookie_alone_has_no_target_and_its_trigger_fizzles() {
        let mut fixture = rookie_fixture();
        fixture.table.cards.retain(|card| card.id != fixtures::VI);
        fixture.resolve();
        let action = fixtures::move_action(ROOKIE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, ROOKIE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("trigger fizzles")));
    }
}
