use super::prelude::{a_unit_at_a_battlefield, bounce, card_target, done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        bounce(ctx, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Rebuke",
    &[Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        resolve,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play as plays, priority, prompts, settle};
    use crate::state::Showdown;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::CardInfo;

    const THEIR_REBUKE: u32 = 84;
    const CHAOS_A: u32 = 46;
    const CHAOS_B: u32 = 47;

    fn rebuke(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Rebuke", 2, 2);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let spark = fixture
            .table
            .cards
            .iter()
            .position(|card| card.id == fixtures::HAND_SPELL)
            .unwrap();
        fixture.table.cards[spark] = rebuke(fixtures::HAND_SPELL, 0);
        fixture.table.cards.push(rebuke(THEIR_REBUKE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_A, 0, "Chaos", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_B, 0, "Chaos", false));
        let jinx = fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap();
        jinx.zone = Some(fixtures::BF1);
        jinx.seat = 0;
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
        plays::begin(ctx, seat, card, crate::state::Origin::Hand, None)?;
        settle(ctx)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? else {
            return settle(ctx);
        };
        match (answered.why, answered.answer) {
            (PromptWhy::Target { item, .. }, Answer::Cancel) => plays::cancel(ctx, item),
            (PromptWhy::Target { item, spec }, _) => {
                plays::choose_targets(ctx, item, spec, &answered.prompt.picked)?
            }
            (other, _) => panic!("unexpected prompt {other:?}"),
        }
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_is_an_action_that_targets_one_unit_at_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Rebuke").unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, crate::cards::Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].kind, crate::cards::TargetKind::Card);
        assert_eq!(
            ability.targets[0].filter,
            crate::cards::prelude::UNIT_AT_BATTLEFIELD
        );
    }

    #[test]
    fn rebuke_returns_an_enemy_unit_at_a_battlefield_to_its_owners_hand() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let my_hand = ctx.hand_of(0).len();
        let their_hand = ctx.hand_of(1).len();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "only units at battlefields are offered, never the ones in a base"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 71}: choose a unit at a battlefield (0 of 1)"
        );
        assert!(ctx.effects.is_empty(), "targets come before the cost");
        pick(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(ctx.events.contains(&Event::Chosen {
            card: fixtures::THEIR_UNIT,
            by: 0,
            item: 1
        }));
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(CHAOS_A),
                Effect::exhaust(CHAOS_B),
                Effect::Move {
                    card: CHAOS_A,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: BOTTOM
                },
                Effect::Move {
                    card: CHAOS_B,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    index: BOTTOM
                },
            ],
            "the two Chaos runes pay both the energy and the power"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "the off-domain runes stay ready"
        );
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::BF1),
            "nothing happens before it resolves"
        );
        resolve_chain(&mut ctx);
        let jinx = ctx.card(fixtures::THEIR_UNIT).unwrap();
        assert_eq!(jinx.zone, Some(fixtures::HAND));
        assert_eq!(
            jinx.seat, 1,
            "it goes to the owner's hand, not the caster's"
        );
        assert_eq!(ctx.hand_of(1).len(), their_hand + 1);
        assert_eq!(
            ctx.hand_of(0).len(),
            my_hand - 1,
            "Rebuke itself left the hand"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::THEIR_UNIT,
            zone: fixtures::HAND,
            seat: 1,
            index: TOP
        }));
        assert!(ctx.state_of(fixtures::THEIR_UNIT).is_none());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 81} returns to hand"));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 71} resolves");
        assert!(ctx.blob.seat(0).played_main);
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_token_at_a_battlefield_vanishes_instead_of_going_to_a_hand() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let their_hand = ctx.hand_of(1).len();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        resolve_chain(&mut ctx);
        assert!(ctx.effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert_eq!(
            ctx.hand_of(1).len(),
            their_hand,
            "a token is not a card in hand"
        );
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_target_that_left_the_battlefield_before_resolution_is_left_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        pick(&mut ctx, 0, 1).unwrap();
        let base = ctx.zones.base.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::THEIR_UNIT, base, 1), 1)
            .unwrap();
        let their_hand = ctx.hand_of(1).len();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.card(fixtures::THEIR_UNIT).unwrap().zone, Some(base));
        assert_eq!(ctx.hand_of(1).len(), their_hand);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("returns to hand")));
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH),
            "the spell still resolves and is trashed"
        );
    }

    #[test]
    fn units_in_a_base_and_non_units_are_refused_as_targets() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            plays::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in a base is not at a battlefield"
        );
        assert_eq!(
            plays::choose_targets(&mut ctx, 1, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a battlefield card is not a unit"
        );
        assert_eq!(
            plays::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in hand is not on the board"
        );
        assert_eq!(
            plays::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        let cancel = labels(&ctx).len() as u16 - 1;
        pick(&mut ctx, 0, cancel).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn rebuke_needs_the_players_own_turn_or_an_open_chain_free_of_reactions() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_REBUKE)),
            Err(Refusal::NotYourTurn),
            "an action has no window on the other seat's turn outside a showdown"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        pick(&mut ctx, 0, 1).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_REBUKE)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 1 waits for priority"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_REBUKE)),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "an action is not a reaction: it cannot join a chain"
        );
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::HAND)
        );
    }

    #[test]
    fn the_focus_holder_plays_rebuke_inside_a_showdown_and_the_window_stays_open() {
        let mut fixture = armed();
        fixture.blob.showdown = Some(Showdown::open(fixtures::BF1, 0, 1));
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_REBUKE)),
            Err(Refusal::NotYourFocus),
            "the attacker holds focus first"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(labels(&ctx), ["{card 60}", "{card 81}", "cancel"]);
        pick(&mut ctx, 0, 1).unwrap();
        assert_eq!(priority::holder(&ctx), Some(0));
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_REBUKE)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "a showdown closed state follows priority"
        );
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::HAND)
        );
        let showdown = ctx
            .blob
            .showdown
            .clone()
            .expect("the showdown is still open");
        assert_eq!(
            showdown.focus(),
            1,
            "focus hands to the next seat once the play's chain empties (343)"
        );
        assert_eq!(showdown.passes(), 0);
        assert!(ctx.blob.chain.is_empty() && ctx.blob.priority.is_none());
    }

    #[test]
    fn rebuke_is_refused_when_no_unit_stands_at_a_battlefield() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.table.tokens.clear();
        let jinx = fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap();
        jinx.zone = Some(fixtures::BASE);
        jinx.seat = 1;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let offered = labels(&ctx);
        assert!(
            !offered.iter().any(|label| label.starts_with("{card")),
            "units in bases are no candidates: {offered:?}"
        );
        assert_eq!(offered.last().map(String::as_str), Some("cancel"));
        let cancel = offered.len() as u16 - 1;
        pick(&mut ctx, 0, cancel).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(ctx.blob.is_neutral_open());
    }
}
