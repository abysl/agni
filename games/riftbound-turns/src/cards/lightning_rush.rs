use super::ivern_nurturer::look_at_the_top_three;
use super::prelude::{asking, done, draw, play, spell, with_candidates};
use super::{Card, Cost, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const LOOK: usize = 3;
pub const QUESTION: &str = "a card among the top three to draw";
pub const PICK: u8 = 1;
pub const FLOW: Cost = Cost {
    energy: 2,
    power: &[Power::Rainbow],
};

pub fn looked(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.zones
        .main_deck
        .map(|deck| ctx.top_of(deck, seat, LOOK))
        .unwrap_or_default()
}

fn on_top(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != PICK {
        return Vec::new();
    }
    looked(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn draw_from_the_top(ctx: &mut Ctx, seat: u8, card: u32) -> bool {
    let Some(deck) = ctx.zones.main_deck else {
        return false;
    };
    ctx.emit(Effect::Move {
        card,
        zone: deck,
        seat,
        index: TOP,
    });
    let drawn = draw(ctx, seat, 1);
    if drawn == 1 {
        ctx.narrate(format!("{{seat {seat}}} draws {{card {card}}}"));
    }
    drawn == 1
}

fn trash_the_rest(ctx: &mut Ctx, seat: u8, rest: &[u32]) {
    for card in rest {
        ctx.trash(*card);
    }
    if !rest.is_empty() {
        ctx.narrate(format!(
            "{{seat {seat}}} puts {} into their trash",
            rest.len()
        ));
    }
}

fn keep(ctx: &mut Ctx, item: &Item) -> Flow {
    let seat = item.controller;
    let top = looked(ctx, seat);
    let kept = ctx
        .picks()
        .first()
        .copied()
        .filter(|card| top.contains(card));
    let rest: Vec<u32> = top
        .iter()
        .copied()
        .filter(|card| Some(*card) != kept)
        .collect();
    match kept {
        Some(card) => {
            draw_from_the_top(ctx, seat, card);
        }
        None => ctx.narrate(format!("{{seat {seat}}} draws none of them")),
    }
    trash_the_rest(ctx, seat, &rest);
    done()
}

fn rush(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == PICK {
        return keep(ctx, item);
    }
    look_at_the_top_three(ctx, item, PICK)
}

pub static CARD: Card = spell(
    "Lightning Rush",
    &[Keyword::Flow(FLOW)],
    &[asking(with_candidates(play(&[], rush), on_top), QUESTION)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent};
    use crate::engine::play as play_engine;
    use crate::engine::{prompts, settle};
    use crate::state::{Leave, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP as TOP_OF;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const RUSH: u32 = 90;
    const THEIR_RUSH: u32 = 91;
    const TOP_THREE: [u32; 3] = [23, 22, 21];

    fn rush_card(id: u32, zone: u16, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Lightning Rush", 1, 0);
        card.domain = vec!["Order".into(), "Chaos".into()];
        card
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rush_card(RUSH, zone, 0));
        fixture.table.cards.push(rush_card(THEIR_RUSH, zone, 1));
        fixture.resolve();
        fixture
    }

    fn deck_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn drag(ctx: &Ctx, seat: u8, card: u32, from: u16) -> EntryMove {
        EntryMove {
            card,
            from: Some(from),
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP_OF,
            hidden: false,
        }
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, RUSH).unwrap();
        assert!(ctx.blob.prompt.is_none(), "the look comes at resolution");
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_a_flow_spell_with_one_asking_play_ability() {
        assert!(std::ptr::eq(script_of("Lightning Rush").unwrap(), &CARD));
        assert_eq!(CARD.name, "Lightning Rush");
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        assert_eq!((FLOW.energy, FLOW.power), (2, &[Power::Rainbow][..]));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(LOOK, 3);
        assert_eq!(LOOK, crate::cards::ivern_nurturer::LOOK);
    }

    #[test]
    fn the_top_three_are_peeked_the_pick_is_drawn_and_the_other_two_are_trashed() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(deck_of(&ctx, 0), [20, 21, 22, 23]);
        let hand = ctx.hand_of(0).len();
        cast(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        for card in TOP_THREE {
            assert!(ctx.effects.contains(&Effect::Peek { card, seat: 0 }));
            assert!(!ctx.effects.contains(&Effect::Peek { card, seat: 1 }));
            assert!(
                !ctx.effects.contains(&Effect::Reveal { card }),
                "a look, not a reveal"
            );
        }
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 23}", "{card 22}", "{card 21}", "skip"],
            "the top three, top first, and the may"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {RUSH}}}: choose {QUESTION} (0 of 1)")
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} looks at the top 3 cards of their deck".to_string()));
        fixtures::choose(&mut ctx, 0, "{card 22}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(22).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert_eq!(drew(&ctx, 0), 1, "a draw, not a put");
        assert_eq!(ctx.blob.seat(0).draws, 1);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws {card 22}".to_string()));
        for trashed in [23, 21] {
            assert_eq!(ctx.card(trashed).unwrap().zone, Some(fixtures::TRASH));
            assert!(ctx.effects.contains(&Effect::Move {
                card: trashed,
                zone: fixtures::TRASH,
                seat: 0,
                index: TOP_OF
            }));
        }
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Burned { .. })),
            "the rest are put into the trash, not burned"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} puts 2 into their trash".to_string()));
        assert_eq!(deck_of(&ctx, 0), [20], "the untouched fourth card stays");
        assert_eq!(ctx.card(RUSH).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(deck_of(&ctx, 1), [24, 25]);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_draws_nothing_and_trashes_all_three() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cast(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(drew(&ctx, 0), 0);
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        for trashed in TOP_THREE {
            assert_eq!(ctx.card(trashed).unwrap().zone, Some(fixtures::TRASH));
        }
        assert_eq!(deck_of(&ctx, 0), [20]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} draws none of them".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} puts 3 into their trash".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_thin_deck_offers_what_there_is_and_an_empty_one_looks_at_nothing() {
        let mut fixture = armed(fixtures::HAND);
        fixture
            .table
            .cards
            .retain(|card| ![20, 21, 22].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert_eq!(fixtures::labels(&ctx), ["{card 23}", "skip"]);
        fixtures::choose(&mut ctx, 0, "{card 23}").unwrap();
        assert_eq!(ctx.card(23).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(drew(&ctx, 0), 1);
        assert!(deck_of(&ctx, 0).is_empty());
        assert!(ctx.trash_of(0).contains(&RUSH));
        assert_eq!(ctx.trash_of(0).len(), 1, "nothing else to trash");
        drop(ctx);
        let mut empty = armed(fixtures::HAND);
        empty
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::MAIN_DECK) || card.owner != 0);
        empty.resolve();
        let mut ctx = empty.ctx();
        cast(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing to look at");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no cards left to look at".to_string()));
        assert_eq!(ctx.card(RUSH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_the_trash_the_flow_play_looks_draws_and_is_banished() {
        let mut fixture = armed(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, 0, RUSH, fixtures::TRASH)),
            Ok(Intent::Play {
                card: RUSH,
                origin: Origin::Trash {
                    leave: Leave::Banish
                },
                location: None,
                on_chain: true,
            })
        );
        let ready = ctx.ready_runes_of(0).len();
        play_engine::begin(
            &mut ctx,
            0,
            RUSH,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - usize::from(FLOW.energy),
            "two energy, one of which pays the rainbow"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        fixtures::choose(&mut ctx, 0, "{card 21}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(21).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(ctx.banished_of(0), [RUSH]);
        let mut trashed = ctx.trash_of(0);
        trashed.sort_unstable();
        assert_eq!(trashed, [22, 23], "the two not drawn, and no Rush");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seats_pick_a_card_off_the_menu_and_their_turnless_play_are_refused() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, 1, THEIR_RUSH, fixtures::HAND)),
            Err(Refusal::NotYourTurn)
        );
        cast(&mut ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 4 }),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 4,
                count: 4
            }))
        );
        ctx.picked = vec![20];
        let item = ctx.blob.chain[0].clone();
        assert_eq!(
            keep(&mut ctx, &item),
            Flow::Done,
            "a pick outside the top three draws nothing"
        );
        assert_eq!(drew(&ctx, 0), 0);
        assert_eq!(ctx.card(20).unwrap().zone, Some(fixtures::MAIN_DECK));
        for trashed in TOP_THREE {
            assert_eq!(ctx.card(trashed).unwrap().zone, Some(fixtures::TRASH));
        }
    }
}
