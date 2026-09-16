use super::prelude::{card_target, done, kill, play, target, unit, GEAR};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::{Ctx, Killed};

const VICTIM: TargetSpec = target(GEAR, 0, 1, TargetKind::Card, "a gear to kill");

fn rake(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(gear) = card_target(ctx, item, 0) else {
        return done();
    };
    if kill(ctx, item, gear) == Killed::Yes {
        ctx.narrate(format!("{{card {gear}}} dies"));
    }
    done()
}

pub static CARD: Card = unit("Disarming Rake", &[], &[play(&[VICTIM], rake)]);

#[cfg(test)]
mod tests {
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::Origin;
    use crate::state::PromptWhy;

    const RAKE: u32 = 90;
    const TRINKET: u32 = 91;

    #[test]
    fn the_rake_may_kill_a_gear_and_skips_when_declined() {
        let mut fixture = Fixture::enforced();
        let mut rake = fixtures::unit(RAKE, fixtures::HAND, 0, "Disarming Rake", 2);
        rake.energy = Some(3);
        rake.power = Some(1);
        fixture.table.cards.push(rake);
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 1, "Trinket", 2));
        fixture.resolve();
        let action = fixtures::move_action(RAKE, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_engine::begin(&mut ctx, 0, RAKE, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert_eq!(fixtures::labels(&ctx), ["{card 91}", "skip"]);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(TRINKET).unwrap().zone, Some(fixtures::TRASH));
    }
}
