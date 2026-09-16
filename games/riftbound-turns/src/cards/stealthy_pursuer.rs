use super::prelude::{
    asking, done, move_destinations, move_to_location_of, optional, trigger_subject, triggered,
    unit, when, with_candidates,
};
use super::{Card, Event, Flow, Item, Source, Stage, Trigger, Where, Who};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str = "the unit to be moved along with";

pub const A_FRIENDLY_UNIT_MOVES: Trigger = Trigger::Move {
    of: Who::Friendly,
    to: Where::FromLocation,
};

fn another_friendly_unit_left_my_location(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Moved {
        card,
        from: Some(from),
        ..
    } = event
    else {
        return false;
    };
    *card != source.card && ctx.location(source.card) == Some(*from)
}

pub fn companion(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let me = item.kind.source();
    let Some(mover) = trigger_subject(item) else {
        return Vec::new();
    };
    let Some(there) = ctx.location(mover) else {
        return Vec::new();
    };
    let follows = mover != me
        && ctx.is_unit(mover)
        && ctx.on_board(me)
        && ctx.location(me) != Some(there)
        && move_destinations(ctx, me).contains(&there);
    if follows {
        vec![TargetRef::Card(mover)]
    } else {
        Vec::new()
    }
}

fn pursue(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_PICKED => {
            let offered = companion(ctx, item, stage);
            let Some(mover) = ctx
                .picks()
                .first()
                .copied()
                .filter(|mover| offered.contains(&TargetRef::Card(*mover)))
            else {
                return done();
            };
            ctx.narrate(format!("{{card {me}}} slips along with {{card {mover}}}"));
            move_to_location_of(ctx, item, me, mover);
            done()
        }
        _ => {
            if companion(ctx, item, Stage(STAGE_PICKED)).is_empty() {
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Stealthy Pursuer",
    &[],
    &[asking(
        with_candidates(
            optional(when(
                triggered(A_FRIENDLY_UNIT_MOVES, &[], pursue),
                another_friendly_unit_left_my_location,
            )),
            companion,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Event, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{act, legal, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const PURSUER: u32 = 90;

    fn pursuer(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(PURSUER, zone, seat, "Stealthy Pursuer", 4)
        }
    }

    fn shadowing(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(pursuer(zone, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn march(fixture: &mut Fixture, unit: u32, to: u16) -> Ctx<'_> {
        let action = fixtures::move_action(unit, to, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.expect("the drag is an entry move");
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        ctx
    }

    fn decline_the_group_move(ctx: &mut Ctx) {
        assert!(matches!(ctx.blob.why, Some(PromptWhy::GroupMove { .. })));
        fixtures::choose(ctx, 0, "done").unwrap();
    }

    fn moves_of(ctx: &Ctx, card: u32) -> Vec<(Option<Location>, Location)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::Moved {
                    card: moved,
                    from,
                    to,
                    ..
                } if *moved == card => Some((*from, *to)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_optional_conditional_friendly_move_trigger() {
        assert!(std::ptr::eq(script_of("Stealthy Pursuer").unwrap(), &CARD));
        assert_eq!(CARD.name, "Stealthy Pursuer");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let follow = &CARD.abilities[0];
        assert_eq!(follow.trigger, A_FRIENDLY_UNIT_MOVES);
        assert!(follow.optional, "I may be moved");
        assert!(
            follow.condition.is_some(),
            "from my location, and not my own move"
        );
        assert!(follow.candidates.is_some());
        assert_eq!(follow.question, Some(QUESTION));
        assert!(follow.targets.is_empty());
        assert!(follow.cost.is_none());
    }

    #[test]
    fn a_friendly_march_out_of_the_base_lets_the_pursuer_be_moved_along_when_the_trigger_resolves()
    {
        let mut fixture = shadowing(fixtures::BASE);
        let mut ctx = march(&mut fixture, fixtures::VI, fixtures::BF1);
        decline_the_group_move(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PURSUER
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.prompt.is_none(), "the may is asked at resolution");
        assert_eq!(ctx.location(PURSUER), Some(Location::Base(0)));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let item = match ctx.blob.why {
            Some(PromptWhy::Resume { item, stage }) => {
                assert_eq!(stage, STAGE_PICKED);
                item
            }
            other => panic!("the trigger asks whether to follow, not {other:?}"),
        };
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Resume {
                    item,
                    stage: STAGE_PICKED
                }
            ),
            format!("{{card {PURSUER}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(PURSUER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            moves_of(&ctx, PURSUER),
            [(
                Some(Location::Base(0)),
                Location::Battlefield(fixtures::BF1)
            )]
        );
        assert!(
            !ctx.card(PURSUER).unwrap().exhausted,
            "moved by an effect, not marched: it stays ready"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {PURSUER}}} slips along with {{card {}}}",
            fixtures::VI
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_leaves_the_pursuer_where_it_was() {
        let mut fixture = shadowing(fixtures::BASE);
        let mut ctx = march(&mut fixture, fixtures::VI, fixtures::BF1);
        decline_the_group_move(&mut ctx);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(PURSUER), Some(Location::Base(0)));
        assert!(moves_of(&ctx, PURSUER).is_empty());
    }

    #[test]
    fn a_friend_leaving_another_location_and_its_own_march_trigger_nothing() {
        let mut fixture = shadowing(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = march(&mut fixture, fixtures::VI, fixtures::BASE);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(
            ctx.blob.chain.is_empty(),
            "Vi left {{zone 9}}, not the base"
        );

        let mut fixture = shadowing(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        let ctx = march(&mut fixture, PURSUER, fixtures::BF1);
        assert_eq!(
            ctx.location(PURSUER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "its own move is not a friend's");
    }

    #[test]
    fn an_enemy_leaving_its_location_is_not_friendly() {
        let mut fixture = shadowing(fixtures::BF2);
        let mut ctx = fixture.ctx();
        ctx.actor = 1;
        assert_eq!(
            ctx.move_unit(
                fixtures::SPRITE,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(PURSUER),
            Some(Location::Battlefield(fixtures::BF2))
        );
    }

    #[test]
    fn taken_along_by_the_group_move_it_has_already_left_and_asks_nothing_more() {
        let mut fixture = shadowing(fixtures::BASE);
        let mut ctx = march(&mut fixture, fixtures::VI, fixtures::BF1);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::GroupMove { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {PURSUER}}}")).unwrap();
        assert_eq!(
            ctx.location(PURSUER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "143.3 · the leader's move is collected while the group is declared, so the trigger fires"
        );
        assert!(ctx.blob.prompt.is_none());
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "already there: nothing to follow, so no question: {:?}",
            ctx.blob.why
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(PURSUER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(moves_of(&ctx, PURSUER).len(), 1, "one move, the group's");
    }
}
