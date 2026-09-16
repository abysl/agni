use super::prelude::{done, play, seat_target, target, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::PromptWhy;

pub const STAGE_DISCARDED: u8 = 1;
pub const A_PLAYER: TargetSpec = target(
    Filter::Any,
    1,
    1,
    TargetKind::Seat,
    "a player who discards 1",
);

fn ask_seat_discard(ctx: &mut Ctx, item: &Item, seat: u8) -> Flow {
    if ctx.hand_of(seat).is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no card to discard"));
        return done();
    }
    Flow::Ask(ctx.ask(
        seat,
        1,
        1,
        false,
        PromptWhy::Discard {
            item: item.id,
            stage: STAGE_DISCARDED,
        },
    ))
}

fn bewitch(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == STAGE_DISCARDED {
        return done();
    }
    match seat_target(item, 0) {
        Some(seat) => ask_seat_discard(ctx, item, seat),
        None => done(),
    }
}

pub static CARD: Card = unit("Bewitching Spirit", &[], &[play(&[A_PLAYER], bewitch)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{discard, play as play_engine, prompts, settle};
    use crate::state::{ItemKind, ItemStatus, Origin, TargetRef, FLAG_DISCARDED};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::{CardInfo, Face};

    const SPIRIT: u32 = 90;
    const CHAOS_RUNE: u32 = 46;

    fn spirit(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(0),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(SPIRIT, zone, seat, "Bewitching Spirit", 2)
        }
    }

    fn haunt() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(spirit(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.resolve();
        fixture
    }

    fn summon(ctx: &mut Ctx) -> u16 {
        play_engine::begin(ctx, 0, SPIRIT, Origin::Hand, Some(Location::Base(0))).unwrap();
        settle(ctx).unwrap();
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play asks for a player, not {other:?}"),
        }
    }

    #[test]
    fn the_script_targets_any_one_player_including_yourself() {
        assert!(std::ptr::eq(script_of("Bewitching Spirit").unwrap(), &CARD));
        assert_eq!(CARD.name, "Bewitching Spirit");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert_eq!(ability.targets, [A_PLAYER]);
        assert_eq!(A_PLAYER.kind, TargetKind::Seat);
        assert_eq!(A_PLAYER.filter, Filter::Any);
        assert_eq!((A_PLAYER.min, A_PLAYER.max), (1, 1));
        assert!(ability.question.is_none());
    }

    #[test]
    fn choosing_the_opponent_hands_them_the_discard_prompt_and_their_pick_is_trashed_once_its_face_arrives(
    ) {
        let mut fixture = haunt();
        let mut ctx = fixture.ctx();
        let item = summon(&mut ctx);
        assert_eq!(fixtures::labels(&ctx), ["{seat 0}", "{seat 1}"]);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {SPIRIT}}}: choose a player who discards 1 (0 of 1)")
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a player must be chosen"
        );
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SPIRIT
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Seat(1)]);
        let chain_item = ctx.blob.chain[0].id;
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: chain_item,
                stage: STAGE_DISCARDED
            })
        );
        assert_eq!(
            ctx.blob.prompt.as_ref().unwrap().seat,
            1,
            "the chosen player discards, not the spirit's controller"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::THEIR_HAND_CARD)]
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 0, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 1 })),
            "you cannot discard for them"
        );
        fixtures::choose(
            &mut ctx,
            1,
            &format!("{{card {}}}", fixtures::THEIR_HAND_CARD),
        )
        .unwrap();
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::THEIR_HAND_CARD,
            zone: fixtures::TRASH,
            seat: 1,
            index: TOP
        }));
        assert!(
            ctx.has_flag(fixtures::THEIR_HAND_CARD, FLAG_DISCARDED),
            "a face the table does not know is awaited"
        );
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Resolving);
        assert_eq!(ctx.blob.chain[0].stage, STAGE_DISCARDED);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 1}} discards {{card {}}}",
            fixtures::THEIR_HAND_CARD
        )));
        ctx.table
            .apply_entry(
                &Action::Reveal {
                    card: fixtures::THEIR_HAND_CARD,
                    face: Face::named("Jinx").with_kind(crate::cards::KIND_UNIT),
                },
                1,
            )
            .unwrap();
        assert!(discard::revealed(&mut ctx, fixtures::THEIR_HAND_CARD).unwrap());
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the trigger finished with the reveal"
        );
        assert!(ctx.hand_of(1).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn choosing_yourself_asks_you_and_the_pick_leaves_your_hand_at_once() {
        let mut fixture = haunt();
        let mut ctx = fixture.ctx();
        summon(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{seat 0}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        let hand = ctx.hand_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert_eq!(ctx.hand_of(1).len(), 1, "the opponent keeps theirs");
    }

    #[test]
    fn a_player_with_no_hand_has_nothing_to_discard() {
        let mut fixture = haunt();
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::THEIR_HAND_CARD);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        summon(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{seat 1}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 1} has no card to discard".to_string()));
    }
}
