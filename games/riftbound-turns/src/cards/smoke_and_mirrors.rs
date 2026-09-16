use super::prelude::{
    a_card, a_friendly_unit, card_target, done, draw, is_temporary, play, spell, swap_units,
    Swapped, FRIENDLY_UNIT_ELSEWHERE_THAN_FIRST,
};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const CARDS: usize = 1;

pub const TARGETS: &[TargetSpec] = &[
    a_friendly_unit("a unit you control"),
    a_card(
        FRIENDLY_UNIT_ELSEWHERE_THAN_FIRST,
        "another unit you control at a different location",
    ),
];

pub fn either_temporary(ctx: &Ctx, first: u32, second: u32) -> bool {
    is_temporary(ctx, first) || is_temporary(ctx, second)
}

fn mirror(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let (Some(first), Some(second)) = (card_target(ctx, item, 0), card_target(ctx, item, 1)) {
        if either_temporary(ctx, first, second) {
            if swap_units(ctx, first, second) != Swapped::Swapped {
                ctx.narrate(format!(
                    "{{card {first}}} and {{card {second}}} stay where they are"
                ));
            }
        } else {
            ctx.narrate(format!(
                "neither {{card {first}}} nor {{card {second}}} is Temporary"
            ));
        }
    }
    draw(ctx, item.controller, CARDS);
    done()
}

pub static CARD: Card = spell(
    "Smoke and Mirrors",
    &[Keyword::Hidden, Keyword::Action],
    &[play(TARGETS, mirror)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{Location, MoveCause, FRIENDLY_UNIT};
    use crate::cards::{Filter, TargetKind, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{hide, play as play_engine, priority, prompts, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::CardInfo;

    const SMOKE: u32 = 90;
    const THEIR_SMOKE: u32 = 91;
    const MY_SPRITE: u32 = 92;
    const ENERGY: u8 = 2;

    fn smoke(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Smoke and Mirrors", ENERGY, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn sprite(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            might: Some(3),
            ..fixtures::card(id, zone, seat, "Sprite", "Unit")
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(smoke(SMOKE, 0));
        fixture.table.cards.push(smoke(THEIR_SMOKE, 1));
        fixture
            .table
            .cards
            .push(sprite(MY_SPRITE, fixtures::BF1, 0));
        fixture.table.tokens.push(MY_SPRITE);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
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
        play_engine::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn play_from_facedown(ctx: &mut Ctx, zone: u16) -> Result<(), Refusal> {
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(SMOKE, chain, 0), 0)
            .unwrap();
        play_engine::begin(ctx, 0, SMOKE, Origin::Facedown { zone }, None)?;
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
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
            (PromptWhy::Target { item, .. }, Answer::Cancel) => play_engine::cancel(ctx, item),
            (PromptWhy::Target { item, spec }, _) => {
                play_engine::choose_targets(ctx, item, spec, &answered.prompt.picked)?
            }
            (why, answer) => {
                panic!("Smoke and Mirrors opens only target prompts: {why:?} {answer:?}")
            }
        }
        settle(ctx)
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        pick(ctx, seat, option)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    fn swaps(ctx: &Ctx) -> Vec<Event> {
        ctx.events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Moved {
                        cause: MoveCause::Swap,
                        ..
                    }
                )
            })
            .cloned()
            .collect()
    }

    #[test]
    fn the_script_is_a_hidden_action_choosing_two_friendly_units_apart() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Smoke and Mirrors").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Smoke and Mirrors");
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.targets.len(), 2);
        let first = &ability.targets[0];
        assert_eq!(first.filter, FRIENDLY_UNIT);
        assert_eq!((first.min, first.max), (1, 1));
        assert_eq!(first.kind, TargetKind::Card);
        let second = &ability.targets[1];
        assert_eq!(second.filter, FRIENDLY_UNIT_ELSEWHERE_THAN_FIRST);
        assert_eq!(
            second.filter,
            Filter::And(&[
                Filter::Unit,
                Filter::Friendly,
                Filter::DifferentLocationFrom(0)
            ])
        );
        assert_eq!((second.min, second.max), (1, 1));
        assert!(
            hide::lifted_by(&second.filter),
            "737.1.d · a unit at a different location can never be at the hiding battlefield"
        );
        assert!(!hide::lifted_by(&first.filter));
        assert_eq!(CARDS, 1);
    }

    #[test]
    fn with_a_temporary_unit_among_them_the_two_trade_places_in_one_batch_and_one_is_drawn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, SMOKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {MY_SPRITE}}}"),
                "cancel".to_string()
            ],
            "a unit you control · Jinx and the enemy Sprite are not offered"
        );
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            labels(&ctx),
            [format!("{{card {MY_SPRITE}}}"), "cancel".to_string()],
            "another unit you control at a different location"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 1 }),
            format!("{{card {SMOKE}}}: choose another unit you control at a different location (0 of 1)")
        );
        choose(&mut ctx, 0, &format!("{{card {MY_SPRITE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(MY_SPRITE)]
        );
        assert_eq!(
            ctx.effects,
            [Effect::exhaust(41), Effect::exhaust(42)],
            "two energy, no power"
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(MY_SPRITE), Some(Location::Base(0)));
        assert_eq!(
            swaps(&ctx),
            [
                Event::Moved {
                    card: fixtures::VI,
                    from: Some(Location::Base(0)),
                    to: Location::Battlefield(fixtures::BF1),
                    cause: MoveCause::Swap,
                    by: Some(0)
                },
                Event::Moved {
                    card: MY_SPRITE,
                    from: Some(Location::Battlefield(fixtures::BF1)),
                    to: Location::Base(0),
                    cause: MoveCause::Swap,
                    by: Some(0)
                },
            ],
            "one simultaneous swap"
        );
        assert!(
            !ctx.card(fixtures::VI).unwrap().exhausted,
            "a swap is not a standard move, so nothing exhausts"
        );
        assert_eq!(drew(&ctx, 0), 1);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert_eq!(ctx.card(SMOKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} and {{card {MY_SPRITE}}} swap places · {{zone {}}} and their base",
            fixtures::VI,
            fixtures::BF1
        )));
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            &format!("{{card {SMOKE}}} resolves")
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_temporary_unit_nothing_moves_but_the_draw_still_happens() {
        let mut fixture = armed();
        let vi = fixture.table.card_mut(fixtures::VI).unwrap();
        vi.zone = Some(fixtures::BF1);
        fixture.table.cards.retain(|card| card.id != MY_SPRITE);
        fixture
            .table
            .cards
            .push(fixtures::unit(MY_SPRITE, fixtures::BASE, 0, "Jinx", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, SMOKE).unwrap();
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        choose(&mut ctx, 0, &format!("{{card {MY_SPRITE}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(MY_SPRITE), Some(Location::Base(0)));
        assert!(swaps(&ctx).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "neither {{card {}}} nor {{card {MY_SPRITE}}} is Temporary",
            fixtures::VI
        )));
        assert_eq!(drew(&ctx, 0), 1, "the draw is unconditional");
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.card(SMOKE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_unit_that_left_before_resolution_stops_the_swap_and_the_draw_still_happens() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, SMOKE).unwrap();
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        choose(&mut ctx, 0, &format!("{{card {MY_SPRITE}}}")).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(MY_SPRITE, fixtures::TRASH, 0), 0)
            .unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Base(0)),
            "each to the other's location needs both"
        );
        assert!(swaps(&ctx).is_empty());
        assert_eq!(
            drew(&ctx, 0),
            1,
            "356.3.e.5 · the draw is not a target and still happens"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn played_from_facedown_the_first_unit_is_at_the_battlefield_and_the_second_is_lifted() {
        let mut fixture = armed();
        fixture.table.card_mut(SMOKE).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(SMOKE).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_facedown(&mut ctx, fixtures::BF1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            [format!("{{card {MY_SPRITE}}}"), "cancel".to_string()],
            "737.1.d · only a unit you control at the hiding battlefield"
        );
        choose(&mut ctx, 0, &format!("{{card {MY_SPRITE}}}")).unwrap();
        assert_eq!(
            labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "cancel".to_string()],
            "737.1.d lifted · the other unit is by definition elsewhere"
        );
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        let paid: Vec<&Effect> = ctx
            .effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .collect();
        assert!(paid.is_empty(), "a hidden card reacts for free");
        resolve_chain(&mut ctx);
        assert_eq!(ctx.location(MY_SPRITE), Some(Location::Base(0)));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(swaps(&ctx).len(), 2);
        assert_eq!(drew(&ctx, 0), 1, "the draw does not depend on the origin");
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.card(SMOKE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn the_wrong_seat_an_enemy_unit_a_unit_at_the_same_location_and_a_cancel_are_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SMOKE)),
            Err(Refusal::NotYourTurn),
            "an action has no window on the other seat's turn outside a showdown"
        );
        play_from_hand(&mut ctx, 0, SMOKE).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is not a unit you control"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the first choice is mandatory"
        );
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the same unit is not at a different location from itself"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the enemy Sprite is Temporary but not yours"
        );
        let hand = ctx.hand_of(0).len();
        choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty(), "cancelled before the cost");
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(drew(&ctx, 0), 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "the card is back in hand");

        let mut alone = armed();
        alone.table.cards.retain(|card| card.id != MY_SPRITE);
        alone.resolve();
        let mut ctx = alone.ctx();
        play_from_hand(&mut ctx, 0, SMOKE).unwrap();
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            labels(&ctx),
            ["cancel"],
            "with one friendly unit there is no second unit to choose"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(SMOKE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
