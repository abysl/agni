use super::prelude::{done, draw, play, spell};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;
pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};

fn dramatize(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = spell(
    "Downstage Dramatics",
    &[Keyword::Reaction, Keyword::Repeat(REPEAT)],
    &[play(&[], dramatize)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::SLOT_REPEAT;
    use crate::engine::priority;
    use crate::state::{PlayLock, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const DRAMATICS: u32 = 90;
    const THEIR_DRAMATICS: u32 = 91;
    const MY_EXTRA: [u32; 2] = [46, 47];
    const THEIR_EXTRA: [u32; 2] = [48, 49];

    fn dramatics(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Downstage Dramatics", 2, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dramatics(DRAMATICS, 0));
        fixture.table.cards.push(dramatics(THEIR_DRAMATICS, 1));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        for rune in THEIR_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Mind", false));
        }
        fixture.resolve();
        fixture
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_repeatable_reaction_with_no_targets_that_draws_one() {
        assert!(std::ptr::eq(
            script_of("Downstage Dramatics").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Downstage Dramatics");
        assert_eq!(CARD.keywords, [Keyword::Reaction, Keyword::Repeat(REPEAT)]);
        assert_eq!(REPEAT.energy, 2);
        assert!(REPEAT.power.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn unrepeated_it_draws_one_card_for_two_energy() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DRAMATICS).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none(), "no targets to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.blob.chain[0].repeated());
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "two of five energy");
        assert_eq!(drew(&ctx, 0), 0, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert_eq!(ctx.card(DRAMATICS).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert!(ctx.blob.seat(0).played_main);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_it_draws_twice_for_four_energy() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DRAMATICS).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy twice");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(drew(&ctx, 0), 2);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_reacts_to_my_spell_and_draws_before_it_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        fixtures::play_from_hand(&mut ctx, 1, THEIR_DRAMATICS).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 2, .. })
        ));
        fixtures::choose(&mut ctx, 1, "no").unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(drew(&ctx, 1), 1);
        assert_eq!(drew(&ctx, 0), 0);
    }

    #[test]
    fn a_short_purse_skips_the_repeat_and_a_spell_lock_refuses_the_play() {
        let mut poor = armed();
        poor.table.cards.retain(|card| !MY_EXTRA.contains(&card.id));
        poor.resolve();
        let mut ctx = poor.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DRAMATICS).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "three ready runes cannot pay four: the repeat is never offered"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.blob.chain[0].repeated());
        drop(ctx);

        let mut locked = armed();
        locked.blob.seat_mut(0).play_lock = PlayLock::SPELLS;
        let ctx = locked.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: DRAMATICS,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::NoSpells))
        );
    }
}
