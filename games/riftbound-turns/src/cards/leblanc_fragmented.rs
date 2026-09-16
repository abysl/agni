use super::prelude::{deathknell, done, draw, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::Phase;

pub const DRAWS: usize = 1;
pub const BEGINNING_DRAWS: usize = 2;

pub fn your_beginning_phase(ctx: &Ctx, seat: u8) -> bool {
    ctx.blob.phase() == Some(Phase::Beginning) && ctx.turn_player() == seat
}

fn shatter(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let count = if your_beginning_phase(ctx, seat) {
        BEGINNING_DRAWS
    } else {
        DRAWS
    };
    let drawn = draw(ctx, seat, count);
    ctx.narrate(format!("{{seat {seat}}} draws {drawn}"));
    done()
}

pub static CARD: Card = unit(
    "LeBlanc - Fragmented",
    &[Keyword::Assault(1), Keyword::Deathknell],
    &[deathknell(&[], shatter)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::settle;
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const LEBLANC: u32 = 90;

    fn leblanc(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(LEBLANC, zone, seat, "LeBlanc - Fragmented", 3)
        }
    }

    fn rose(phase: Phase) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(leblanc(fixtures::BF1, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.set_phase(phase);
        fixture.resolve();
        fixture
    }

    fn die_and_resolve(ctx: &mut Ctx) {
        assert_eq!(ctx.kill(LEBLANC, Cause::Rule), Killed::Yes);
        settle(ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == LEBLANC
        ));
        assert!(ctx.blob.prompt.is_none());
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_script_prints_assault_one_and_deathknell_with_one_targetless_death_ability() {
        assert!(std::ptr::eq(
            script_of("LeBlanc - Fragmented").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "LeBlanc - Fragmented");
        assert_eq!(CARD.keywords, [Keyword::Assault(1), Keyword::Deathknell]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let death = &CARD.abilities[0];
        assert_eq!(death.trigger, Trigger::Death);
        assert!(death.targets.is_empty());
        assert!(!death.optional);
        assert!(death.condition.is_none(), "the phase is read at resolution");
        assert_eq!((DRAWS, BEGINNING_DRAWS), (1, 2));
        let mut fixture = rose(Phase::Action);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(LEBLANC).unwrap(), &CARD));
        assert!(ctx.has_keyword(LEBLANC, Keyword::Assault(1)));
    }

    #[test]
    fn the_phase_check_reads_the_beginning_phase_of_the_controllers_own_turn_only() {
        let mut fixture = rose(Phase::Beginning);
        let ctx = fixture.ctx();
        assert_eq!(ctx.turn_player(), 0);
        assert!(your_beginning_phase(&ctx, 0));
        assert!(
            !your_beginning_phase(&ctx, 1),
            "the opponent's Beginning Phase is not yours"
        );
        drop(ctx);
        for phase in [
            Phase::Awaken,
            Phase::Channel,
            Phase::Draw,
            Phase::Action,
            Phase::Ending,
            Phase::Cleanup,
            Phase::Expiration,
        ] {
            let mut fixture = rose(phase);
            let ctx = fixture.ctx();
            assert!(
                !your_beginning_phase(&ctx, 0),
                "315.2 · {phase:?} is not the Beginning Phase"
            );
        }
    }

    #[test]
    fn dying_in_the_action_phase_draws_one_when_the_deathknell_resolves() {
        let mut fixture = rose(Phase::Action);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        die_and_resolve(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        assert!(!ctx.events.contains(&Event::Drew { seat: 0, nth: 2 }));
        assert_eq!(ctx.hand_of(1).len(), 1);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 1".to_string()));
    }

    #[test]
    fn dying_in_your_own_beginning_phase_draws_two_instead() {
        let mut fixture = rose(Phase::Beginning);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        die_and_resolve(&mut ctx);
        assert!(ctx.blob.log.contains(&"{seat 0} draws 2".to_string()));
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 2 }));
        assert_eq!(
            ctx.blob.phase(),
            Some(Phase::Action),
            "the emptied chain lets the turn run on through the Draw Phase"
        );
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + BEGINNING_DRAWS + 1,
            "two from the Deathknell and the turn's own draw"
        );
        assert_eq!(ctx.hand_of(1).len(), 1);
    }

    #[test]
    fn the_opponents_leblanc_dying_in_your_beginning_phase_draws_them_one() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(leblanc(fixtures::BASE, 1));
        fixture.blob.set_phase(Phase::Beginning);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let mine = ctx.hand_of(0).len();
        let theirs = ctx.hand_of(1).len();
        assert!(!your_beginning_phase(&ctx, 1));
        die_and_resolve(&mut ctx);
        assert_eq!(ctx.hand_of(1).len(), theirs + DRAWS);
        assert_eq!(
            ctx.hand_of(0).len(),
            mine + 1,
            "only the turn's own draw is yours"
        );
        assert!(ctx.blob.log.contains(&"{seat 1} draws 1".to_string()));
    }
}
