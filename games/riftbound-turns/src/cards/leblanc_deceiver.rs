use super::keeper_of_masks::{become_copy_of, spawn_reflection};
use super::prelude::{
    ask_discard, asking, done, exhausting_self, legend, optional, remember_card, remembered_cards,
    triggered, when, with_candidates, Location,
};
use super::{Ability, Card, Event, Flow, Item, Source, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;
use crate::state::TargetRef;

pub const REFLECTION_ARRIVES_READY: bool = true;
pub const COPY_QUESTION: &str = "another unit there for the Reflection to copy";
const STAGE_DISCARDED: u8 = 1;
const STAGE_COPIED: u8 = 2;

pub fn has_a_card_to_discard(ctx: &Ctx, _: &Event, source: Source) -> bool {
    !ctx.hand_of(ctx.controller(source.card)).is_empty()
}

fn scored_zone(item: &Item) -> Option<u16> {
    match item.subject {
        Some(TargetRef::Zone(zone)) => Some(zone),
        _ => None,
    }
}

fn the_reflection(item: &Item) -> Option<u32> {
    remembered_cards(item).first().copied()
}

fn other_units_there(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let Some(zone) = scored_zone(item) else {
        return Vec::new();
    };
    let token = the_reflection(item);
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| Some(*unit) != token)
        .map(TargetRef::Card)
        .collect()
}

fn give_temporary(ctx: &mut Ctx, token: u32) {
    if ctx.mark_temporary(token) {
        ctx.narrate(format!("{{card {token}}} is Temporary"));
    }
}

fn deceive(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_DISCARDED => {
            let Some(zone) = scored_zone(item) else {
                return done();
            };
            let at = Location::Battlefield(zone);
            if !ctx.units_played_here(zone) {
                ctx.narrate(format!(
                    "no Reflection · units can't be played at {}",
                    describe(at)
                ));
                return done();
            }
            let Some(token) = spawn_reflection(ctx, item.controller, at, REFLECTION_ARRIVES_READY)
            else {
                return done();
            };
            remember_card(ctx, token);
            let others: Vec<u32> = ctx
                .units_at(at)
                .into_iter()
                .filter(|unit| *unit != token)
                .collect();
            if others.is_empty() {
                ctx.narrate(format!("{{card {token}}} has no other unit there to copy"));
                give_temporary(ctx, token);
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_COPIED, 1, 1))
        }
        STAGE_COPIED => {
            let Some(token) = the_reflection(item) else {
                return done();
            };
            let offered = other_units_there(ctx, item, stage);
            if let Some(of) = ctx
                .picks()
                .iter()
                .copied()
                .find(|pick| offered.contains(&TargetRef::Card(*pick)))
            {
                become_copy_of(ctx, token, of);
            }
            give_temporary(ctx, token);
            done()
        }
        _ => match ask_discard(ctx, item, STAGE_DISCARDED) {
            Some(ask) => Flow::Ask(ask),
            None => {
                ctx.narrate(format!("{{card {me}}} · no card to discard, no Reflection"));
                done()
            }
        },
    }
}

const fn may_deceive(trigger: Trigger) -> Ability {
    asking(
        with_candidates(
            optional(exhausting_self(when(
                triggered(trigger, &[], deceive),
                has_a_card_to_discard,
            ))),
            other_units_there,
        ),
        COPY_QUESTION,
    )
}

pub static CARD: Card = legend(
    "LeBlanc - Deceiver",
    &[],
    &[
        may_deceive(Trigger::Conquer(Who::You)),
        may_deceive(Trigger::Hold(Who::You)),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::keeper_of_masks::{
        reflection_face_until_token_reflection_lands, REFLECTION, REFLECTION_MIGHT,
    };
    use crate::cards::{script_of, SelfCost, KIND_UNIT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_TARGET;
    use crate::engine::{cleanup, priority, prompts, settle};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, PromptWhy};

    const LEBLANC: u32 = fixtures::LEGEND_CARD;
    const ALLY: u32 = 90;

    fn rose() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(LEBLANC).unwrap().name = CARD.name.into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LEBLANC).unwrap(),
            &CARD
        ));
        fixture
    }

    fn hold(ctx: &mut Ctx, seat: u8) {
        cleanup::score_holds(ctx, seat);
        settle(ctx).unwrap();
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn reflections_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == REFLECTION && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    fn queue_the_trigger(ctx: &mut Ctx, zone: u16) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(
            id,
            ItemKind::Trigger {
                source: LEBLANC,
                index: 1,
            },
            0,
            Origin::Board,
        );
        item.stage = STAGE_TARGET;
        item.subject = Some(TargetRef::Zone(zone));
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
        settle(ctx).unwrap();
    }

    #[test]
    fn the_legend_has_a_may_conquer_and_a_may_hold_trigger_gated_on_a_card_to_discard() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Conquer(Who::You));
        assert_eq!(CARD.abilities[1].trigger, Trigger::Hold(Who::You));
        for ability in CARD.abilities {
            assert!(ability.optional);
            assert!(ability.cost.is_none());
            assert_eq!(ability.self_cost, SelfCost::Exhaust);
            assert!(ability.condition.is_some(), "a card to discard");
            assert!(ability.candidates.is_some());
            assert_eq!(ability.question, Some(COPY_QUESTION));
            assert!(ability.targets.is_empty(), "there is the subject");
        }
        assert_eq!(REFLECTION_MIGHT, 0, "187.6 · a 0 Might Reflection");
        let face = reflection_face_until_token_reflection_lands();
        assert_eq!(face.name, REFLECTION);
        assert_eq!(face.kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(face.might, Some(0));
        assert!(face.domain.is_empty(), "185.3.b · tokens have no domain");
        assert!(
            script_of(REFLECTION).is_none(),
            "no script until Token::Reflection lands"
        );
    }

    #[test]
    fn holding_asks_to_exhaust_her_then_a_discard_then_which_unit_the_ready_reflection_copies() {
        let mut fixture = rose();
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        hold(&mut ctx, 0);
        assert_eq!(ctx.points(0), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 1 }),
            "392.2 · the exhaust is the cost confirm"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(LEBLANC).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == LEBLANC
        ));
        resolve_top(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: STAGE_DISCARDED
            }),
            "the discard is paid as the trigger resolves"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "discard a card"
        );
        assert!(reflections_of(&ctx, 0).is_empty());
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        let token = *reflections_of(&ctx, 0).first().expect("one Reflection");
        assert!(ctx.is_token(token));
        assert!(ctx.is_unit(token));
        assert_eq!(ctx.controller(token), 0);
        assert_eq!(
            ctx.location(token),
            Some(Location::Battlefield(fixtures::BF1)),
            "played there"
        );
        assert!(!ctx.card(token).unwrap().exhausted, "played ready");
        assert_eq!(ctx.current_might(token), 0);
        assert!(!ctx.is_temporary(token), "not yet");
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_COPIED
            })
        ));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {LEBLANC}}}: choose {COPY_QUESTION} (0 of 1)")
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {ALLY}}}")
            ],
            "the other units there · the Reflection is not offered itself"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {token}}} would become a copy of {{card {}}} (3 Might) · the engine owes Ctx::become_copy",
            fixtures::VI
        )));
        assert!(ctx.is_temporary(token), "given Temporary after the copy");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {token}}} is Temporary")));
        assert_eq!(ctx.current_might(token), 0, "no copy today");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_no_other_unit_there_the_reflection_copies_nothing_and_is_temporary_at_once() {
        let mut fixture = rose();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        queue_the_trigger(&mut ctx, fixtures::BF1);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        resolve_top(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        let token = *reflections_of(&ctx, 0).first().expect("one Reflection");
        assert_eq!(
            ctx.location(token),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.prompt.is_none(), "nothing to copy, nothing to ask");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_temporary(token));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {token}}} has no other unit there to copy")));
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut walled = rose();
        walled
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        walled.resolve();
        let mut ctx = walled.ctx();
        queue_the_trigger(&mut ctx, fixtures::BF2);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        resolve_top(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert!(
            reflections_of(&ctx, 0).is_empty(),
            "Rockfall Path lets no unit be played there · the discard is still paid"
        );
        assert!(ctx.blob.log.contains(&format!(
            "no Reflection · units can't be played at {{zone {}}}",
            fixtures::BF2
        )));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn declining_an_exhausted_leblanc_or_an_empty_hand_plays_no_reflection() {
        let mut fixture = rose();
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(LEBLANC).unwrap().exhausted);
        assert!(reflections_of(&ctx, 0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {LEBLANC}}} trigger is removed · its cost is declined"
        )));
        drop(ctx);
        let mut spent = rose();
        spent.table.card_mut(LEBLANC).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        hold(&mut ctx, 0);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {LEBLANC}}} trigger is removed · its source is exhausted"
        )));
        drop(ctx);
        let mut empty = rose();
        empty
            .table
            .cards
            .retain(|card| !(card.owner == 0 && card.zone == Some(fixtures::HAND)));
        empty.resolve();
        let mut ctx = empty.ctx();
        assert!(ctx.hand_of(0).is_empty());
        hold(&mut ctx, 0);
        assert!(
            ctx.blob.prompt.is_none(),
            "no card to discard, no trigger to confirm"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(LEBLANC).unwrap().exhausted);
        drop(ctx);
        let mut theirs = rose();
        let mut ctx = theirs.ctx();
        hold(&mut ctx, 1);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "seat 1's hold is not hers");
    }

    #[test]
    fn a_conquer_fires_the_first_trigger_and_the_ally_there_is_offered_alongside_vi() {
        let mut fixture = rose();
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == LEBLANC
        ));
        resolve_top(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_UNIT)).unwrap();
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {ALLY}}}")
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        let token = *reflections_of(&ctx, 0).first().expect("one Reflection");
        assert!(ctx.is_temporary(token));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · Token::Reflection and a copy primitive: Ctx::become_copy(token, of) must give the token the copied unit's face, script and Might (477.1.b), so a Reflection played by her hold reads as a copy of the unit it mirrors"]
    fn the_reflection_becomes_a_copy_of_the_chosen_unit_and_reads_its_might() {
        let mut fixture = rose();
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        hold(&mut ctx, 0);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        resolve_top(&mut ctx);
        let token = ctx.table.next_id;
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.card(token).unwrap().name, "Vi");
        assert_eq!(ctx.current_might(token), 3);
        assert!(ctx.is_token(token));
        assert!(ctx.is_temporary(token));
    }
}
