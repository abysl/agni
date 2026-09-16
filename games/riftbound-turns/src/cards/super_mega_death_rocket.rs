use super::prelude::{
    a_unit, asking, card_target, deal, done, on_conquer, optional, play, spell, when,
    with_candidates,
};
use super::{Card, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::state::TargetRef;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const DAMAGE: u8 = 5;
pub const RETURN_FROM_TRASH: u8 = 1;
const STAGE_DISCARDED: u8 = 1;

fn rocket(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub fn fires_from_the_trash(ctx: &Ctx, _: &Event, source: Source) -> bool {
    ctx.in_trash(source.card)
}

fn hand_to_discard(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != STAGE_DISCARDED {
        return Vec::new();
    }
    ctx.hand_of(item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn discard(ctx: &mut Ctx, seat: u8, card: u32) -> bool {
    if ctx.zones.trash.is_none() {
        return false;
    }
    ctx.discard(seat, card);
    true
}

fn return_to_hand(ctx: &mut Ctx, me: u32) -> bool {
    let Some(hand) = ctx.zones.hand else {
        return false;
    };
    let owner = ctx.owner(me);
    ctx.emit(Effect::Move {
        card: me,
        zone: hand,
        seat: owner,
        index: TOP,
    });
    ctx.narrate(format!("{{card {me}}} returns from the trash to hand"));
    true
}

fn reload(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    if !ctx.in_trash(me) {
        ctx.narrate(format!("{{card {me}}} is no longer in the trash"));
        return done();
    }
    if stage.0 != STAGE_DISCARDED {
        if ctx.hand_of(seat).is_empty() {
            ctx.narrate(format!(
                "{{seat {seat}}} has nothing to discard for {{card {me}}}"
            ));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, STAGE_DISCARDED, 0, 1));
    }
    let Some(card) = ctx.picks().first().copied() else {
        ctx.narrate(format!("{{seat {seat}}} keeps {{card {me}}} in the trash"));
        return done();
    };
    if !ctx.hand_of(seat).contains(&card) || !discard(ctx, seat, card) {
        return done();
    }
    return_to_hand(ctx, me);
    done()
}

pub static CARD: Card = spell(
    "Super Mega Death Rocket!",
    &[],
    &[
        play(&[a_unit("a unit")], rocket),
        when(
            asking(
                with_candidates(optional(on_conquer(&[], reload)), hand_to_discard),
                "a card to discard to return the rocket to hand",
            ),
            fires_from_the_trash,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_PAY;
    use crate::engine::{prompts, settle, triggers};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const ROCKET: u32 = 90;
    const BRUTE: u32 = 91;

    fn rocket_card(zone: u16) -> CardInfo {
        CardInfo {
            domain: vec!["Fury".into(), "Chaos".into()],
            ..fixtures::spell(ROCKET, zone, 0, "Super Mega Death Rocket!", 4, 1)
        }
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rocket_card(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BASE, 1, "Brute", 5));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn conquered(zone: u16, seat: u8) -> Event {
        Event::Conquered {
            zone,
            seat,
            units: vec![fixtures::VI],
        }
    }

    fn queue_the_trigger(ctx: &mut Ctx) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: ROCKET,
                index: RETURN_FROM_TRASH,
            },
            0,
            Origin::Board,
        );
        item.stage = STAGE_PAY;
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
        settle(ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_rocket_has_no_timing_keyword_deals_five_to_a_unit_anywhere_and_carries_its_trash_trigger(
    ) {
        assert!(std::ptr::eq(
            script_of("Super Mega Death Rocket!").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let reload = &CARD.abilities[usize::from(RETURN_FROM_TRASH)];
        assert_eq!(reload.trigger, Trigger::Conquer(Who::You));
        assert!(reload.optional);
        assert!(reload.condition.is_some());
        assert!(reload.candidates.is_some());
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ROCKET).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "{card 91}", "cancel"],
            "any unit, in a base or at a battlefield"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(BRUTE), "five kills five Might");
        assert_eq!(ctx.card(ROCKET).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_reload_discards_a_chosen_card_and_returns_the_rocket_from_the_trash() {
        let mut fixture = armed(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        assert!(fires_from_the_trash(
            &ctx,
            &conquered(fixtures::BF1, 0),
            Source {
                card: ROCKET,
                ability: RETURN_FROM_TRASH
            }
        ));
        let hand = ctx.hand_of(0).len();
        queue_the_trigger(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_DISCARDED
            })
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 0, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 70}", "{card 71}", "{card 72}", "{card 73}", "skip"],
            "the whole hand, and the may as a skip"
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Resume {
                    item: 1,
                    stage: STAGE_DISCARDED
                }
            ),
            "{card 90}: choose a card to discard to return the rocket to hand (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{card 71}").unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.card(ROCKET).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(ROCKET).unwrap().seat, 0);
        assert_eq!(ctx.hand_of(0).len(), hand, "one out, the rocket in");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} discards {card 71}".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} returns from the trash to hand".to_string()));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_keeps_it_in_the_trash_and_an_empty_hand_is_never_asked() {
        let mut fixture = armed(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.card(ROCKET).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.hand_of(0).len(), 4, "nothing discarded");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} keeps {card 90} in the trash".to_string()));
        let mut bare = armed(fixtures::TRASH);
        bare.table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 0);
        bare.resolve();
        let mut ctx = bare.ctx();
        queue_the_trigger(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.card(ROCKET).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has nothing to discard for {card 90}".to_string()));
        let mut elsewhere = armed(fixtures::HAND);
        let mut ctx = elsewhere.ctx();
        assert!(!fires_from_the_trash(
            &ctx,
            &conquered(fixtures::BF1, 0),
            Source {
                card: ROCKET,
                ability: RETURN_FROM_TRASH
            }
        ));
        queue_the_trigger(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "a rocket in hand has nothing to return"
        );
        assert_eq!(ctx.hand_of(0).len(), 5);
    }

    #[test]
    fn the_engine_does_not_yet_source_a_conquer_trigger_from_the_trash() {
        let mut fixture = armed(fixtures::TRASH);
        let ctx = fixture.ctx();
        let found = triggers::find(&ctx, &conquered(fixtures::BF1, 0));
        assert!(
            !found.iter().any(|found| found.source == ROCKET),
            "triggers::sources lists in-play cards only: {found:?}"
        );
    }

    #[test]
    #[ignore = "engine gap · triggers::sources lists in-play cards only, so a spell in the trash never hears a conquer"]
    fn a_conquer_with_the_rocket_in_the_trash_queues_its_reload() {
        let mut fixture = armed(fixtures::TRASH);
        let ctx = fixture.ctx();
        let found = triggers::find(&ctx, &conquered(fixtures::BF1, 0));
        assert!(found
            .iter()
            .any(|found| found.source == ROCKET && found.index == RETURN_FROM_TRASH));
        let theirs = triggers::find(&ctx, &conquered(fixtures::BF1, 1));
        assert!(!theirs.iter().any(|found| found.source == ROCKET));
    }
}
