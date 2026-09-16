use super::prelude::{done, draw, play, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 4;

fn progress(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = spell("Progress Day", &[], &[play(&[], progress)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::priority;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const PROGRESS_DAY: u32 = 90;
    const MIND_RUNE: u32 = 46;
    const EXTRA_RUNES: [u32; 3] = [47, 48, 49];

    fn progress_day(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(PROGRESS_DAY, fixtures::HAND, seat, "Progress Day", 6, 1);
        card.domain = vec!["Mind".into()];
        card
    }

    fn funded() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(progress_day(0));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PROGRESS_DAY).unwrap(),
            &CARD
        ));
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

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_plain_spell_with_no_targets() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Progress Day").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(DRAWS, 4);
    }

    #[test]
    fn progress_day_draws_four_for_six_energy_and_one_mind_power() {
        let mut fixture = funded();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let deck = ctx.table.held(fixtures::MAIN_DECK, 0).count();
        assert_eq!(deck, 4);
        fixtures::play_from_hand(&mut ctx, 0, PROGRESS_DAY).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "six of eight runes exhaust for the energy: {:?}",
            ctx.effects
        );
        assert_eq!(
            ctx.card(MIND_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "the Mind rune exhausts for energy and then recycles for the power"
        );
        assert_eq!(drew(&ctx, 0), 0, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1 + DRAWS,
            "the spell leaves, four arrive"
        );
        assert_eq!(ctx.table.held(fixtures::MAIN_DECK, 0).count(), 0);
        assert_eq!(ctx.card(PROGRESS_DAY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{card 90} resolves".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn it_is_refused_short_of_six_ready_runes_and_off_turn() {
        let mut short = Fixture::enforced();
        short.table.cards.push(progress_day(0));
        short
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        short.resolve();
        let ctx = short.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, PROGRESS_DAY)),
            Err(Refusal::NotEnoughRunes {
                needed: 6,
                ready: 4
            })
        );
        let mut theirs = Fixture::enforced();
        theirs.table.cards.push(progress_day(1));
        theirs.resolve();
        let ctx = theirs.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, PROGRESS_DAY)),
            Err(Refusal::NotYourTurn),
            "a plain spell waits for its controller's turn"
        );
        let mut closed = funded();
        closed.blob.priority = Some(crate::state::Priority {
            active: 0,
            passes: 0,
        });
        let ctx = closed.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, PROGRESS_DAY)),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "no Action or Reaction: the chain must be open"
        );
    }
}
