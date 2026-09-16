use super::prelude::{a_friendly_unit, an_enemy_unit, bounce, card_target, done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

const TARGETS: &[TargetSpec] = &[
    a_friendly_unit("a friendly unit to return to its owner's hand"),
    an_enemy_unit("an enemy unit to return to its owner's hand"),
];

fn part_them(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for index in 0..TARGETS.len() {
        if let Some(unit) = card_target(ctx, item, index) {
            bounce(ctx, unit);
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Star-Crossed",
    &[Keyword::Reaction],
    &[play(TARGETS, part_them)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Filter;
    use crate::engine::ctx::{Cause, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play, priority, prompts, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};

    const MINE: u32 = 90;
    const THEIRS: u32 = 91;
    const THEIR_RUNE_A: u32 = 46;
    const THEIR_RUNE_B: u32 = 47;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            MINE,
            fixtures::HAND,
            0,
            "Star-Crossed",
            3,
            1,
        ));
        let mut theirs = fixtures::spell(THEIRS, fixtures::HAND, 1, "Star-Crossed", 3, 1);
        theirs.domain = vec!["Mind".into()];
        fixture.table.cards.push(theirs);
        for rune in [THEIR_RUNE_A, THEIR_RUNE_B] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Mind", false));
        }
        fixture.resolve();
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

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let answered = prompts::answer(ctx, seat, Pick { prompt, option })?;
        if let Some(answered) = answered {
            match (answered.why, answered.answer) {
                (PromptWhy::Target { item, .. }, Answer::Cancel) => play::cancel(ctx, item),
                (PromptWhy::Target { item, spec }, _) => {
                    play::choose_targets(ctx, item, spec, &answered.prompt.picked)?
                }
                _ => panic!("Star-Crossed only opens target prompts: {answered:?}"),
            }
        }
        settle(ctx)
    }

    fn pick_card(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        let option = prompts::offered(ctx)
            .iter()
            .position(|opt| opt.card == Some(card))
            .expect("the unit is offered") as u16;
        pick(ctx, seat, option)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn moves_to_hand(ctx: &Ctx, card: u32) -> Vec<u8> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card: moved,
                    zone,
                    seat,
                    ..
                } if *moved == card && *zone == fixtures::HAND => Some(*seat),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_reaction_with_a_friendly_and_an_enemy_unit_target() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Star-Crossed").unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let specs = CARD.abilities[0].targets;
        assert_eq!(specs.len(), 2);
        assert_eq!(
            specs[0].filter,
            Filter::And(&[Filter::Unit, Filter::Friendly])
        );
        assert_eq!(specs[1].filter, Filter::And(&[Filter::Unit, Filter::Enemy]));
        for spec in specs {
            assert_eq!((spec.min, spec.max), (1, 1));
        }
    }

    #[test]
    fn it_returns_a_friendly_unit_and_an_enemy_unit_to_their_owners_hands() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MINE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "cancel"],
            "only my own unit is friendly"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a friendly unit to return to its owner's hand (0 of 1)"
        );
        assert!(ctx.effects.is_empty(), "targets come before the cost");
        pick_card(&mut ctx, 0, fixtures::VI).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "the enemy's units, token included"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 1 }),
            "{card 90}: choose an enemy unit to return to its owner's hand (0 of 1)"
        );
        pick_card(&mut ctx, 0, fixtures::THEIR_UNIT).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
        for card in [fixtures::VI, fixtures::THEIR_UNIT] {
            assert!(ctx.events.contains(&Event::Chosen {
                card,
                by: 0,
                item: 1
            }));
        }
        assert!(!ctx.effects.is_empty(), "the cost is paid at finalize");
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::BASE),
            "nothing moves until it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        let vi = ctx.card(fixtures::VI).unwrap();
        assert_eq!((vi.zone, vi.seat), (Some(fixtures::HAND), 0));
        let jinx = ctx.card(fixtures::THEIR_UNIT).unwrap();
        assert_eq!((jinx.zone, jinx.seat), (Some(fixtures::HAND), 1));
        assert_eq!(moves_to_hand(&ctx, fixtures::VI), [0]);
        assert_eq!(moves_to_hand(&ctx, fixtures::THEIR_UNIT), [1]);
        assert!(ctx.state_of(fixtures::VI).is_none());
        assert!(ctx.state_of(fixtures::THEIR_UNIT).is_none());
        assert_eq!(ctx.card(MINE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} returns to hand".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} returns to hand".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert!(ctx.blob.seat(0).played_main);
    }

    #[test]
    fn a_returned_token_ceases_to_exist() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MINE).unwrap();
        pick_card(&mut ctx, 0, fixtures::VI).unwrap();
        pick_card(&mut ctx, 0, fixtures::SPRITE).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert!(ctx.effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert_eq!(moves_to_hand(&ctx, fixtures::SPRITE), Vec::<u8>::new());
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::BASE)
        );
    }

    #[test]
    fn a_unit_that_left_the_board_is_skipped_and_the_other_still_returns() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MINE).unwrap();
        pick_card(&mut ctx, 0, fixtures::VI).unwrap();
        pick_card(&mut ctx, 0, fixtures::THEIR_UNIT).unwrap();
        ctx.kill(fixtures::THEIR_UNIT, Cause::Rule);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(moves_to_hand(&ctx, fixtures::THEIR_UNIT), Vec::<u8>::new());
        assert_eq!(ctx.card(MINE).unwrap().zone, Some(fixtures::TRASH));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MINE).unwrap();
        pick_card(&mut ctx, 0, fixtures::VI).unwrap();
        pick_card(&mut ctx, 0, fixtures::THEIR_UNIT).unwrap();
        ctx.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::HAND);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(moves_to_hand(&ctx, fixtures::VI), Vec::<u8>::new());
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::HAND)
        );
    }

    #[test]
    fn it_is_refused_out_of_priority_and_refuses_units_from_the_wrong_side() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            play_from_hand(&mut ctx, 1, THEIRS),
            Err(Refusal::NotYourTurn),
            "a reaction in the other seat's neutral open is refused"
        );
        assert_eq!(ctx.card(THEIRS).unwrap().zone, Some(fixtures::HAND));
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            play_from_hand(&mut ctx, 1, THEIRS),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 1 waits for priority"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, THEIRS).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "friendly is read from the reacting seat"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "my unit is not friendly to them"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the friendly unit is not optional"
        );
        pick_card(&mut ctx, 1, fixtures::THEIR_UNIT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 1 }));
        assert_eq!(labels(&ctx), ["{card 50}", "cancel"]);
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 1, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "their own token is not an enemy"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 1, &[THEIRS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a spell on the chain is not a unit"
        );
        assert!(ctx.blob.pending(2).is_some(), "the play is still pending");
        pick_card(&mut ctx, 1, fixtures::VI).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(
            ctx.blob.chain[1].targets,
            [
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Card(fixtures::VI)
            ]
        );
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.blob.chain[0].kind.source(), fixtures::HAND_SPELL);
        assert_eq!(moves_to_hand(&ctx, fixtures::THEIR_UNIT), [1]);
        assert_eq!(moves_to_hand(&ctx, fixtures::VI), [0]);
        assert_eq!(ctx.card(THEIRS).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.seat(1).played_main);
    }

    #[test]
    fn with_no_enemy_unit_the_second_half_only_offers_cancel_and_nothing_is_paid() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE && card.id != fixtures::THEIR_UNIT);
        fixture.table.tokens.clear();
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, MINE).unwrap();
        assert_eq!(labels(&ctx), ["{card 50}", "cancel"]);
        pick_card(&mut ctx, 0, fixtures::VI).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        let offered = labels(&ctx);
        assert!(
            !offered.iter().any(|label| label.starts_with("{card")),
            "no enemy unit to return: {offered:?}"
        );
        let cancel = offered.iter().position(|label| label == "cancel").unwrap();
        assert!(ctx.effects.is_empty(), "nothing was paid");
        pick(&mut ctx, 0, cancel as u16).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(
            ctx.effects,
            [Effect::Move {
                card: MINE,
                zone: fixtures::HAND,
                seat: 0,
                index: TOP
            }]
        );
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::BASE));
        assert!(ctx.blob.is_neutral_open());
    }
}
