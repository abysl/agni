use super::keeper_of_masks::{become_copy_of, spawn_reflection};
use super::prelude::{a_unit, card_target, done, play, spell, Location};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const REFLECTION_ARRIVES_READY: bool = true;

fn mirror(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let seat = item.controller;
    let Some(token) = spawn_reflection(ctx, seat, Location::Base(seat), REFLECTION_ARRIVES_READY)
    else {
        return done();
    };
    become_copy_of(ctx, token, unit);
    if ctx.mark_temporary(token) {
        ctx.narrate(format!("{{card {token}}} is Temporary"));
    }
    done()
}

pub static CARD: Card = spell(
    "Mirror Image",
    &[],
    &[play(&[a_unit("a unit to copy")], mirror)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::keeper_of_masks::{
        is_reflection, reflection_face_until_token_reflection_lands, REFLECTION, REFLECTION_MIGHT,
    };
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, Keyword, Resolved, Trigger, IMPLICIT_TEMPORARY, KIND_UNIT};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, triggers};
    use crate::rules::COUNTER_TEMPORARY;
    use crate::state::{GameBlob, Mode, Origin, PromptWhy, TargetRef};
    use crate::{Refusal, TurnEvent};
    use agni_plugin_sdk::decide::{Action, Effect, Request, TOP};
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const MIRROR: u32 = 90;
    const THEIR_MIRROR: u32 = 91;
    const MIND_RUNE: u32 = 100;
    const ORDER_RUNE: u32 = 101;
    const TOKEN: u32 = 200;

    fn mirror_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Mirror Image", 3, 2);
        card.domain = vec!["Mind".into(), "Order".into()];
        card
    }

    fn hall() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mirror_card(MIRROR, 0));
        fixture.table.cards.push(mirror_card(THEIR_MIRROR, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
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

    fn temporary_matches(ctx: &Ctx, seat: u8) -> Vec<u32> {
        triggers::find(ctx, &Event::BeginningPhase { seat })
            .into_iter()
            .filter(|held| held.index == IMPLICIT_TEMPORARY)
            .map(|held| held.source)
            .collect()
    }

    fn apply(
        table: &mut agni_plugin_sdk::table::Snapshot,
        blob: &mut GameBlob,
        seat: u8,
        action: Action,
    ) {
        let request = Request {
            plugin_state: blob.encode(),
            players: 2,
            seat,
            action: action.clone(),
            table: table.clone(),
        };
        let verdict =
            crate::engine::decide(&request, GameBlob::decode(&request.plugin_state).unwrap())
                .unwrap();
        if let Action::Move { .. } = action {
            table.apply_entry(&action, seat).unwrap();
        }
        table.apply_all(&verdict.effects, seat).unwrap();
        *blob = GameBlob::decode(&verdict.plugin_state.unwrap()).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_spell_over_any_unit_and_the_reflection_face_is_a_blank_unit() {
        assert!(std::ptr::eq(script_of("Mirror Image").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        let face = reflection_face_until_token_reflection_lands();
        assert_eq!(face.name, REFLECTION);
        assert_eq!(face.kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(face.might, Some(REFLECTION_MIGHT));
    }

    #[test]
    fn a_ready_reflection_is_played_to_your_base_as_a_temporary_token() {
        let mut fixture = hall();
        let mut ctx = fixture.ctx();
        let units = ctx.units_at(Location::Base(0)).len();
        fixtures::play_from_hand(&mut ctx, 0, MIRROR).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Spawn { face, zone, seat: 0, owner: Some(0) }
                if face.name == REFLECTION && *zone == fixtures::BASE
        )));
        assert!(!is_reflection(&ctx, TOKEN));
        assert_eq!(
            ctx.card(TOKEN).unwrap().name,
            ctx.card(fixtures::THEIR_UNIT).unwrap().name
        );
        assert!(ctx.is_token(TOKEN));
        assert_eq!(ctx.location(TOKEN), Some(Location::Base(0)));
        assert_eq!(ctx.units_at(Location::Base(0)).len(), units + 1);
        assert!(!ctx.card(TOKEN).unwrap().exhausted, "played ready");
        assert_eq!(ctx.controller(TOKEN), 0);
        assert!(ctx.is_temporary(TOKEN));
        assert_eq!(
            ctx.table.counter(Target::Card(TOKEN), COUNTER_TEMPORARY),
            Some(1)
        );
        assert_eq!(temporary_matches(&ctx, 0), [TOKEN]);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == TOKEN
        )));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} plays a ready {card 200} to their base".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 200} becomes a copy of {card 81}".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 200} is Temporary".to_string()));
        assert_eq!(ctx.card(MIRROR).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_unit_that_left_the_board_is_not_copied_and_no_reflection_is_played() {
        let mut fixture = hall();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MIRROR).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let hand = ctx.zones.hand.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::THEIR_UNIT, hand, 1), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.card(TOKEN).is_none(),
            "359.3.e.7 · the instruction over the illegal target does not execute"
        );
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        assert_eq!(ctx.card(MIRROR).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gear_or_a_hand_card_is_refused_and_the_opponents_copy_waits_for_their_turn() {
        let mut fixture = hall();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_MIRROR)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, MIRROR).unwrap();
        for wrong in [
            fixtures::GROUNDS,
            fixtures::HAND_UNIT,
            fixtures::HAND_GEAR,
            fixtures::LEGEND_CARD,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(MIRROR).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_reflection_carries_the_copied_units_might_name_and_script() {
        let mut fixture = hall();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MIRROR).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(TOKEN),
            3,
            "Vi's three · {:?}",
            ctx.blob.log
        );
        assert_eq!(ctx.card(TOKEN).unwrap().name, "Vi");
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Transform { card: TOKEN, face } if face.name == "Vi"
        )));
        assert!(ctx.is_temporary(TOKEN));
        assert!(ctx.is_token(TOKEN));
    }

    #[test]
    fn a_serialized_play_request_persists_the_copied_face_and_script() {
        let fixture = hall();
        let source = fixture.scripts.of_card(fixtures::VI).unwrap();
        let mut table = fixture.table.clone();
        let mut blob = GameBlob::start(2, 0, Mode::Free);
        apply(
            &mut table,
            &mut blob,
            0,
            Action::Move {
                card: MIRROR,
                to: Some(fixtures::CHAIN),
                seat: 0,
                index: TOP,
                hidden: false,
            },
        );
        let offered = blob.offered(&table).unwrap();
        let option = offered
            .iter()
            .position(|option| option.label == format!("{{card {}}}", fixtures::VI))
            .unwrap();
        let prompt = blob.prompt.as_ref().unwrap().id;
        apply(
            &mut table,
            &mut blob,
            0,
            Action::Game(
                TurnEvent::Pick(Pick {
                    prompt,
                    option: option as u16,
                })
                .encode(),
            ),
        );
        apply(
            &mut table,
            &mut blob,
            0,
            Action::Game(TurnEvent::Pass.encode()),
        );
        apply(
            &mut table,
            &mut blob,
            1,
            Action::Game(TurnEvent::Pass.encode()),
        );
        assert_eq!(table.card(TOKEN).unwrap().name, "Vi");
        assert!(table.is_token(TOKEN));
        assert!(std::ptr::eq(
            Resolved::of(&table).of_card(TOKEN).unwrap(),
            source
        ));
        let mut reloaded = GameBlob::decode(&blob.encode()).unwrap();
        let scripts = Resolved::of(&table);
        let ctx = Ctx::fresh(&table, &mut reloaded, &scripts, 0);
        assert_eq!(ctx.current_might(TOKEN), 3);
        assert!(ctx.is_temporary(TOKEN));
    }
}
