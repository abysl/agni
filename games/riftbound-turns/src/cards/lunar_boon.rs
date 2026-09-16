use super::prelude::{ask_discard, discarded, done, draw, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 2;
pub const STAGE_DRAW: u8 = 1;

fn boon(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 != STAGE_DRAW {
        return match ask_discard(ctx, item, STAGE_DRAW) {
            Some(ask) => Flow::Ask(ask),
            None => {
                ctx.narrate(format!("{{seat {seat}}} has nothing to discard"));
                draw(ctx, seat, DRAWS);
                done()
            }
        };
    }
    if discarded(ctx).is_none() {
        ctx.narrate(format!("{{seat {seat}}} discards nothing"));
    }
    draw(ctx, seat, DRAWS);
    done()
}

pub static CARD: Card = spell("Lunar Boon", &[Keyword::Reaction], &[play(&[], boon)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::engine::{discard, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BOON: u32 = 90;
    const THEIR_BOON: u32 = 91;
    const CHAOS_RUNES: [u32; 3] = [46, 47, 48];

    fn boon_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Lunar Boon", 3, 0);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(boon_card(BOON, 0));
        fixture.table.cards.push(boon_card(THEIR_BOON, 1));
        {
            let theirs = fixture.table.card_mut(fixtures::THEIR_HAND_CARD).unwrap();
            theirs.name = "Jinx".into();
            theirs.kind = Some("Unit".into());
        }
        for rune in CHAOS_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Chaos", false));
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
    fn the_script_is_a_reaction_with_no_targets_that_asks_the_discard_as_it_resolves() {
        assert!(std::ptr::eq(script_of("Lunar Boon").unwrap(), &CARD));
        assert_eq!(CARD.name, "Lunar Boon");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert_eq!(DRAWS, 2);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BOON).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose at play time");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "three energy paid");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: STAGE_DRAW
            }),
            "the discard is asked as the spell resolves"
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(drew(&ctx, 0), 0, "the draw waits for the discard");
    }

    #[test]
    fn the_controller_discards_one_then_draws_two() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, BOON).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 70}", "{card 71}", "{card 72}", "{card 73}"],
            "every card left in hand"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
        fixtures::choose(&mut ctx, 0, "{card 70}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(drew(&ctx, 0), 2);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "one played, one discarded, two drawn"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} discards {card 70}".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(BOON).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_hand_nothing_is_discarded_and_the_two_are_still_drawn() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 0 || card.id == BOON);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BOON).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "no card to offer");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            drew(&ctx, 0),
            2,
            "the draw is not conditional on the discard"
        );
        assert_eq!(ctx.hand_of(0).len(), 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has nothing to discard".to_string()));
    }

    #[test]
    fn the_other_seat_reacts_to_my_spell_and_its_discard_is_its_own_hand() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_BOON).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 2,
                stage: STAGE_DRAW
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 1);
        assert_eq!(fixtures::labels(&ctx), ["{card 80}"]);
        assert_eq!(
            discard::gesture(&mut ctx, 0, fixtures::HAND_UNIT),
            Err(Refusal::NoPrompt),
            "the discard is seat 1's alone"
        );
        fixtures::choose(&mut ctx, 1, "{card 80}").unwrap();
        assert_eq!(
            ctx.card(fixtures::THEIR_HAND_CARD).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(drew(&ctx, 1), 2);
        assert_eq!(drew(&ctx, 0), 0);
        assert_eq!(ctx.blob.chain.len(), 1, "my spell still waits beneath");
    }

    #[test]
    fn a_card_outside_the_hand_cannot_be_discarded_and_a_short_purse_refuses_the_play() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BOON).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            discard::pick(&mut ctx, 0, 1, STAGE_DRAW, fixtures::VI),
            Err(Refusal::NoPrompt),
            "a unit on the board is not in hand"
        );
        assert_eq!(
            discard::pick(&mut ctx, 0, 1, STAGE_DRAW, fixtures::THEIR_HAND_CARD),
            Err(Refusal::NoPrompt),
            "the other seat's card is not mine to discard"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert_eq!(drew(&ctx, 0), 0);
        drop(ctx);

        let mut broke = armed();
        broke.table.card_mut(43).unwrap().exhausted = true;
        let ctx = broke.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: BOON,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 2
            })
        );
    }
}
