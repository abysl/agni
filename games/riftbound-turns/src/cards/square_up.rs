use super::prelude::{
    a_unit, ask_discard, card_target, discarded, done, grant_this_turn, play, spell,
};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 4;
pub const DISCARDS: u8 = 1;
pub const REPEAT: Cost = Cost::FREE;
pub const STAGE_GRANT: u8 = 1;
const REPEATED: u8 = 1;

pub fn a_discard_can_be_offered_as_the_repeat_cost(ctx: &Ctx, seat: u8) -> bool {
    ctx.hand_of(seat).len() >= usize::from(DISCARDS)
}

pub fn discard_paid_at_resolution_until_play_offers_a_discard_at_the_repeat_stage(
    ctx: &mut Ctx,
    item: &Item,
) -> Option<Flow> {
    if item.execution != REPEATED {
        return None;
    }
    Some(match ask_discard(ctx, item, STAGE_GRANT) {
        Some(ask) => Flow::Ask(ask),
        None => {
            ctx.narrate(format!(
                "{{card {}}} · nothing to discard for the Repeat, nothing to repeat",
                item.kind.source()
            ));
            done()
        }
    })
}

fn square(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 != STAGE_GRANT {
        if let Some(flow) =
            discard_paid_at_resolution_until_play_offers_a_discard_at_the_repeat_stage(ctx, item)
        {
            return flow;
        }
    } else if discarded(ctx).is_none() {
        return done();
    }
    if let Some(unit) = card_target(ctx, item, 0) {
        if grant_this_turn(ctx, unit, Keyword::Assault(ASSAULT)) {
            ctx.narrate(format!(
                "{{card {unit}}} gets [Assault {ASSAULT}] this turn"
            ));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Square Up",
    &[Keyword::Repeat(REPEAT)],
    &[play(&[a_unit("a unit")], square)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::{Expiry, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const SQUARE: u32 = 90;
    const THEIR_SQUARE: u32 = 91;
    const EXTRA_RUNE: u32 = 100;

    fn square_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Square Up", 4, 0);
        card.domain = vec!["Fury".into()];
        card
    }

    fn ring() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(square_card(SQUARE, 0));
        fixture.table.cards.push(square_card(THEIR_SQUARE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(EXTRA_RUNE, 0, "Fury", false));
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

    fn assaults_of(ctx: &Ctx, unit: u32) -> usize {
        ctx.state_of(unit)
            .map(|state| {
                state
                    .granted
                    .iter()
                    .filter(|(keyword, _)| *keyword == Keyword::Assault(ASSAULT))
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_sorcery_spell_over_one_unit_whose_repeat_carries_no_resource_cost() {
        assert!(std::ptr::eq(script_of("Square Up").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Repeat(REPEAT)]);
        assert_eq!(
            REPEAT,
            Cost::FREE,
            "the discard is not an energy or a power"
        );
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Assault(ASSAULT)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!((ASSAULT, DISCARDS), (4, 1));
        let mut fixture = ring();
        let ctx = fixture.ctx();
        assert!(a_discard_can_be_offered_as_the_repeat_cost(&ctx, 0));
        assert!(a_discard_can_be_offered_as_the_repeat_cost(&ctx, 1));
    }

    #[test]
    fn declined_it_grants_assault_four_for_the_turn_without_a_discard() {
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, SQUARE).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "four energy");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "no discard without the Repeat");
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        let turn = ctx.turn();
        assert_eq!(
            ctx.state_of(fixtures::THEIR_UNIT).unwrap().granted,
            [(Keyword::Assault(ASSAULT), Expiry::EndOfTurn(turn))]
        );
        ctx.mark_attacker(fixtures::THEIR_UNIT);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 6, "2 + Assault 4");
        ctx.expire(Expiry::EndOfTurn(turn));
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Assault(ASSAULT)));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} gets [Assault 4] this turn".to_string()));
        assert_eq!(ctx.card(SQUARE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_the_second_grant_waits_for_a_discard_and_the_same_unit_twice_stacks_two_assaults() {
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SQUARE).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(item.spec_counts, [1, 1]);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "four energy for the spell and none for the Repeat"
        );
        let hand = ctx.hand_of(0).len();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(assaults_of(&ctx, fixtures::VI), 1, "the first grant landed");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: STAGE_GRANT
            }),
            "the Repeat's discard is asked before the second grant"
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(assaults_of(&ctx, fixtures::VI), 2);
        ctx.mark_attacker(fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 11, "3 + 4 + 4");
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| line == &"{card 50} gets [Assault 4] this turn")
                .count(),
            2
        );
        assert_eq!(ctx.card(SQUARE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_nothing_left_to_discard_the_repeat_grants_nothing() {
        let mut fixture = ring();
        fixture.table.cards.retain(|card| {
            card.zone != Some(fixtures::HAND) || card.id == SQUARE || card.owner != 0
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SQUARE).unwrap();
        assert!(
            !a_discard_can_be_offered_as_the_repeat_cost(&ctx, 0),
            "the spell itself was the last card in hand"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "no card to discard, no prompt");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(assaults_of(&ctx, fixtures::VI), 1);
        assert_eq!(
            assaults_of(&ctx, fixtures::THEIR_UNIT),
            0,
            "the unpaid Repeat grants nothing"
        );
        assert!(ctx.blob.log.contains(
            &"{card 90} · nothing to discard for the Repeat, nothing to repeat".to_string()
        ));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_copy_waits_for_their_turn_and_a_gear_or_a_hand_card_is_refused() {
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SQUARE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, SQUARE).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        for wrong in [fixtures::GROUNDS, fixtures::HAND_UNIT, fixtures::HAND_GEAR] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(SQUARE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · non-resource costs at the pay stage (355.10.c, 357): a discard as the Repeat cost is a pick prompt before the item exists, and an empty hand withholds the Repeat ask; play::begin reads Keyword::Repeat as a resource cost only, so the stub carries Repeat(Cost::FREE) and discard_paid_at_resolution_until_play_offers_a_discard_at_the_repeat_stage collects it as the spell resolves (the Brazen Buccaneer seam)"]
    fn the_discard_is_paid_at_the_repeat_stage_and_an_empty_hand_is_not_asked_to_repeat() {
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SQUARE).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Discard { item: 1, .. })),
            "the discard follows the yes, before any target · {:?}",
            ctx.blob.why
        );
        drop(ctx);
        let mut empty = ring();
        empty.table.cards.retain(|card| {
            card.zone != Some(fixtures::HAND) || card.id == SQUARE || card.owner != 0
        });
        empty.resolve();
        let mut ctx = empty.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SQUARE).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 1, spec: 0 }),
            "with nothing to discard the Repeat is never offered"
        );
    }
}
