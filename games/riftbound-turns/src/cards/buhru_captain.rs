use super::prelude::{asking, buff, done, draw, optional, play, unit, with_candidates};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const DRAWS: usize = 1;
pub const STAGE_DRAW: u8 = 1;
pub const STAGE_BUFF: u8 = 2;
pub const QUESTION: &str = "the main deck to draw 1, then the captain to buff · or skip";

fn buffable(ctx: &Ctx, me: u32) -> bool {
    ctx.on_board(me) && ctx.is_unit(me)
}

pub fn choices(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    let me = item.kind.source();
    match stage.0 {
        STAGE_DRAW => ctx
            .zones
            .main_deck
            .map(|main_deck| vec![TargetRef::Zone(main_deck)])
            .unwrap_or_default(),
        STAGE_BUFF if buffable(ctx, me) => vec![TargetRef::Card(me)],
        _ => Vec::new(),
    }
}

fn offer_buff(ctx: &mut Ctx, item: &Item) -> Flow {
    if !buffable(ctx, item.kind.source()) {
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, STAGE_BUFF, 0, 1))
}

fn captain(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_DRAW => {
            let main_deck = ctx.zones.main_deck.map(u32::from);
            if ctx.picks().first().copied() == main_deck {
                draw(ctx, item.controller, DRAWS);
                return done();
            }
            offer_buff(ctx, item)
        }
        STAGE_BUFF => {
            if ctx.picks().first().copied() == Some(me) && buff(ctx, me) {
                ctx.narrate(format!("{{card {me}}} is buffed"));
            }
            done()
        }
        _ => {
            if ctx.zones.main_deck.is_none() {
                return offer_buff(ctx, item);
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_DRAW, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Buhru Captain",
    &[],
    &[asking(
        optional(with_candidates(play(&[], captain), choices)),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const CAPTAIN: u32 = 90;
    const BODY_RUNE: u32 = 46;
    const TOP_OF_MAIN_DECK: u32 = 23;

    fn captain_card(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Body".into()],
            ..fixtures::unit(CAPTAIN, zone, 0, "Buhru Captain", 3)
        }
    }

    fn harbour() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(captain_card(fixtures::HAND));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture.resolve();
        fixture
    }

    fn sail(ctx: &mut Ctx) -> u16 {
        fixtures::play_from_hand(ctx, 0, CAPTAIN).unwrap();
        assert_eq!(ctx.location(CAPTAIN), Some(Location::Base(0)));
        assert!(ctx.blob.prompt.is_none(), "the choice comes at resolution");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CAPTAIN
        ));
        let item = ctx.blob.chain[0].id;
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: STAGE_DRAW
            })
        );
        item
    }

    fn decline_the_draw(ctx: &mut Ctx, item: u16) {
        fixtures::choose(ctx, 0, "skip").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item,
                stage: STAGE_BUFF
            }),
            "declining the draw offers the buff"
        );
        assert_eq!(
            fixtures::labels(ctx),
            [format!("{{card {CAPTAIN}}}"), "skip".to_string()]
        );
    }

    fn draws(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: 0, .. }))
            .count()
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_asks_deck_or_captain_at_resolution() {
        assert!(std::ptr::eq(script_of("Buhru Captain").unwrap(), &CARD));
        assert_eq!(CARD.name, "Buhru Captain");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.optional, "you may");
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn the_resolving_trigger_offers_the_deck_then_the_captain_and_the_deck_draws_one() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let item = sail(&mut ctx);
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::MAIN_DECK),
                "skip".to_string()
            ],
            "one kind of ref per prompt · the captain is offered after a declined draw"
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Resume {
                    item,
                    stage: STAGE_DRAW
                }
            ),
            format!("{{card {CAPTAIN}}}: choose {QUESTION} (0 of 1)")
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the other seat cannot choose for us"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::MAIN_DECK)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + DRAWS);
        assert!(ctx.in_hand(TOP_OF_MAIN_DECK));
        assert_eq!(draws(&ctx), 1);
        assert!(!ctx.is_buffed(CAPTAIN), "draw, not buff");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn picking_the_captain_buffs_him_and_draws_nothing() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let item = sail(&mut ctx);
        decline_the_draw(&mut ctx, item);
        fixtures::choose(&mut ctx, 0, &format!("{{card {CAPTAIN}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(CAPTAIN));
        assert_eq!(ctx.current_might(CAPTAIN), 4, "the buff's +1");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {CAPTAIN}}} is buffed")));
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "no draw");
        assert_eq!(draws(&ctx), 0);
    }

    #[test]
    fn skipping_does_neither() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let item = sail(&mut ctx);
        decline_the_draw(&mut ctx, item);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(CAPTAIN));
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert_eq!(draws(&ctx), 0);
    }

    #[test]
    fn a_captain_gone_before_resolution_leaves_only_the_deck_to_choose() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CAPTAIN).unwrap();
        ctx.bounce(CAPTAIN);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::MAIN_DECK),
                "skip".to_string()
            ],
            "nothing on the board to buff"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::MAIN_DECK)).unwrap();
        assert_eq!(draws(&ctx), 1);
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CAPTAIN).unwrap();
        ctx.bounce(CAPTAIN);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none(), "no captain to offer");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(draws(&ctx), 0);
    }

    #[test]
    fn a_captain_whose_id_is_the_main_deck_zone_id_still_draws_when_the_deck_is_picked() {
        let mut fixture = harbour();
        let collision = u32::from(fixtures::MAIN_DECK);
        fixture.table.cards.retain(|card| card.id != collision);
        fixture.table.card_mut(CAPTAIN).unwrap().id = collision;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, collision).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::MAIN_DECK),
                "skip".to_string()
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::MAIN_DECK)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(draws(&ctx), 1, "the pick is the deck, not the captain");
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + DRAWS);
        assert!(!ctx.is_buffed(collision));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }
}
