use super::prelude::{
    card_target, done, optional, play, swap_units, target, unit, HIDDEN, UNIT_AT_ANOTHER_LOCATION,
};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const SWAP_WITH: TargetSpec = target(
    UNIT_AT_ANOTHER_LOCATION,
    0,
    1,
    TargetKind::Card,
    "a friendly unit at another location",
);

fn turn_the_tide(ctx: &mut Ctx, item: &Item, _stage: Stage) -> Flow {
    let me = item.kind.source();
    let Some(other) = card_target(ctx, item, 0) else {
        return done();
    };
    swap_units(ctx, me, other);
    done()
}

pub static CARD: Card = unit(
    "Tideturner",
    HIDDEN,
    &[optional(play(&[SWAP_WITH], turn_the_tide))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::Location;
    use crate::cards::{Filter, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{act, hide, legal, priority, prompts, resume, settle, targets};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef, FLAG_FROM_FACEDOWN};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::CardInfo;

    const TIDETURNER: u32 = 90;
    const ALLY_HERE: u32 = 91;
    const ALLY_AWAY: u32 = 92;

    fn tideturner(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Tideturner", 2);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn ally(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Jinx", 2);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn afloat() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(tideturner(TIDETURNER, fixtures::HAND, 0));
        fixture.table.cards.push(ally(ALLY_HERE, fixtures::BF1, 0));
        fixture.table.cards.push(ally(ALLY_AWAY, fixtures::BASE, 0));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn landed() -> Fixture {
        let mut fixture = afloat();
        fixture.table.card_mut(TIDETURNER).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture
    }

    fn alone() -> Fixture {
        let mut fixture = afloat();
        fixture.table.cards.retain(|card| card.id != ALLY_AWAY);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture
    }

    fn face_down(fixture: &mut Fixture) {
        let card = fixture.table.card_mut(TIDETURNER).unwrap();
        card.name = String::new();
        card.kind = None;
        card.energy = None;
        card.might = None;
        card.domain.clear();
        fixture.resolve();
    }

    fn face_up(fixture: &mut Fixture) {
        let zone = fixture.table.card(TIDETURNER).and_then(|card| card.zone);
        let seat = fixture
            .table
            .card(TIDETURNER)
            .map(|card| card.seat)
            .unwrap_or(0);
        let card = fixture.table.card_mut(TIDETURNER).unwrap();
        *card = CardInfo {
            zone,
            seat,
            ..tideturner(TIDETURNER, zone.unwrap_or(fixtures::HAND), 0)
        };
        fixture.resolve();
    }

    fn entry(ctx: &Ctx, card: u32, to: u16) -> EntryMove {
        let before = ctx.card(card);
        EntryMove {
            card,
            from: before.and_then(|held| held.zone),
            from_seat: before.map(|held| held.seat).unwrap_or(0),
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn drag(fixture: &mut Fixture, seat: u8, card: u32, to: u16) -> Result<legal::Intent, Refusal> {
        let action = fixtures::move_action(card, to, 0);
        let ctx = fixture.ctx_for(seat, &action);
        let moved = ctx.entry.expect("a drag is an entry move");
        legal::classify(&ctx, seat, &moved)
    }

    fn hide_now(fixture: &mut Fixture, seat: u8, zone: u16) -> Result<(), Refusal> {
        let action = fixtures::move_action(TIDETURNER, zone, 0);
        let mut ctx = fixture.ctx_for(seat, &action);
        let moved = ctx.entry.expect("a drag is an entry move");
        let intent = legal::classify(&ctx, seat, &moved)?;
        assert_eq!(
            intent,
            legal::Intent::Hide {
                card: TIDETURNER,
                zone
            }
        );
        let done = act(&mut ctx, seat, intent);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        done
    }

    fn hidden(fixture: &mut Fixture) {
        face_down(fixture);
        hide_now(fixture, 0, fixtures::BF1).expect("the [A] hide is legal");
        fixture.blob.core_mut().unwrap().turn += 1;
        face_up(fixture);
    }

    fn play_from_facedown(fixture: &mut Fixture) -> Fixture {
        let action = fixtures::move_action(TIDETURNER, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let moved = ctx.entry.expect("a drag is an entry move");
        let intent = legal::classify(&ctx, 0, &moved).expect("737.6 · it reacts from facedown");
        assert_eq!(intent, legal::Intent::PlayFromFacedown { card: TIDETURNER });
        act(&mut ctx, 0, intent).expect("737.1.b · played ignoring its base cost");
        settle(&mut ctx).expect("the play trigger settles onto the chain");
        let table = ctx.table.clone();
        drop(ctx);
        let mut played = Fixture::enforced();
        played.blob = fixture.blob.clone();
        played.commit(table);
        played
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn answer(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        answer(ctx, seat, option)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn item(origin: Origin) -> ChainItem {
        ChainItem::new(1, ItemKind::Permanent { card: TIDETURNER }, 0, origin)
    }

    #[test]
    fn the_script_is_a_hidden_unit_whose_play_trigger_may_swap_with_a_unit_elsewhere() {
        assert_eq!(CARD.name, "Tideturner");
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.optional, "you may choose a unit");
        assert!(ability.cost.is_none());
        assert!(ability.extra.is_none());
        assert!(ability.condition.is_none());
        assert_eq!(ability.once, crate::cards::Once::Never);
        assert_eq!(ability.targets.len(), 1);
        let spec = &ability.targets[0];
        assert_eq!(spec.filter, UNIT_AT_ANOTHER_LOCATION);
        assert_eq!(
            spec.filter,
            Filter::And(&[Filter::Unit, Filter::Friendly, Filter::Not(&Filter::Here)])
        );
        assert_eq!((spec.min, spec.max), (0, 1), "the may is a 0-of-1 target");
        assert_eq!(spec.kind, TargetKind::Card);
        let fixture = afloat();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(TIDETURNER).unwrap(),
            &CARD
        ));
        assert!(std::ptr::eq(
            super::super::script_of("Tideturner").unwrap(),
            &CARD
        ));
    }

    #[test]
    fn its_target_is_free_of_the_facedown_restriction_because_here_can_never_match() {
        let mut fixture = landed();
        let ctx = fixture.ctx();
        let spec = &CARD.abilities[0].targets[0];
        assert!(
            hide::lifted_by(&spec.filter),
            "737.1.d · the Tideturner lift"
        );
        assert!(!hide::lifted_by(&Filter::Here));
        let hidden = item(Origin::Facedown {
            zone: fixtures::BF1,
        });
        assert!(targets::from_facedown(
            &ctx,
            &hidden,
            spec,
            TargetRef::Card(ALLY_AWAY)
        ));
        let offered = targets::candidates(&ctx, &hidden, spec);
        assert_eq!(
            offered,
            [TargetRef::Card(ALLY_AWAY)],
            "737.1.d · the only offer is the friendly unit away from the hiding battlefield"
        );
        assert_eq!(
            targets::candidates(&ctx, &item(Origin::Board), spec),
            offered,
            "the lift means a play from facedown offers exactly what a play from hand offers"
        );
    }

    #[test]
    fn hiding_it_costs_one_rune_and_playing_it_from_facedown_lands_it_at_that_battlefield() {
        let mut fixture = afloat();
        face_down(&mut fixture);
        let runes = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.zone == Some(fixtures::RUNE_POOL) && card.owner == 0)
            .count();
        hide_now(&mut fixture, 0, fixtures::BF1).unwrap();
        assert_eq!(
            fixture
                .blob
                .card_state(TIDETURNER)
                .and_then(|row| row.hidden_at),
            Some(fixtures::BF1),
            "737.1.b · it is hidden facedown at the battlefield it was dragged to"
        );
        assert_eq!(
            fixture
                .table
                .cards
                .iter()
                .filter(|card| card.zone == Some(fixtures::RUNE_POOL) && card.owner == 0)
                .count(),
            runes - 1,
            "[A] recycles exactly one rune"
        );
        fixture.blob.core_mut().unwrap().turn += 1;
        face_up(&mut fixture);
        let action = fixtures::move_action(TIDETURNER, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        assert_eq!(intent, legal::Intent::PlayFromFacedown { card: TIDETURNER });
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(TIDETURNER),
            Some(Location::Battlefield(fixtures::BF1)),
            "737.1.d.1 · a hidden unit must be played to that battlefield"
        );
        assert_eq!(
            ctx.blob
                .card_state(TIDETURNER)
                .and_then(|row| row.hidden_at),
            None,
            "it has left the facedown zone"
        );
        assert!(ctx.has_flag(TIDETURNER, FLAG_FROM_FACEDOWN));
        let paid: Vec<&Effect> = ctx
            .effects
            .iter()
            .filter(|effect| {
                matches!(effect, Effect::Move { zone, index, .. }
                    if *zone == fixtures::RUNE_DECK && *index == BOTTOM)
            })
            .collect();
        assert!(paid.is_empty(), "737.1.b · it is played for nothing");
    }

    #[test]
    fn playing_it_from_facedown_asks_only_for_a_friendly_unit_somewhere_else() {
        let mut fixture = afloat();
        hidden(&mut fixture);
        let mut played = play_from_facedown(&mut fixture);
        let ctx = played.ctx();
        let item = match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play trigger asks for a unit, not {other:?}"),
        };
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            labels(&ctx),
            [format!("{{card {ALLY_AWAY}}}"), "skip".to_string()],
            "737.1.d · the restriction is lifted, so a unit at another location is on offer"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {TIDETURNER}}}: choose a friendly unit at another location (0 of 1)")
        );
    }

    #[test]
    fn choosing_the_unit_records_it_on_the_trigger_and_the_swap_runs_at_resolution() {
        let mut fixture = afloat();
        hidden(&mut fixture);
        let mut played = play_from_facedown(&mut fixture);
        let mut ctx = played.ctx();
        choose(&mut ctx, 0, &format!("{{card {ALLY_AWAY}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == TIDETURNER
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(ALLY_AWAY)]);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "the trigger resolved");
        assert_eq!(ctx.location(TIDETURNER), Some(Location::Base(0)));
        assert_eq!(
            ctx.location(ALLY_AWAY),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let moves: Vec<&Event> = ctx
            .events
            .iter()
            .filter(|event| matches!(event, Event::Moved { .. }))
            .collect();
        assert_eq!(
            moves,
            [
                &Event::Moved {
                    card: TIDETURNER,
                    from: Some(Location::Battlefield(fixtures::BF1)),
                    to: Location::Base(0),
                    cause: MoveCause::Swap,
                    by: Some(0)
                },
                &Event::Moved {
                    card: ALLY_AWAY,
                    from: Some(Location::Base(0)),
                    to: Location::Battlefield(fixtures::BF1),
                    cause: MoveCause::Swap,
                    by: Some(0)
                },
            ],
            "one batch: both units moved before either trigger is collected"
        );
    }

    #[test]
    fn the_swap_trades_the_two_units_places_in_one_move() {
        let mut fixture = afloat();
        hidden(&mut fixture);
        let mut played = play_from_facedown(&mut fixture);
        let mut ctx = played.ctx();
        choose(&mut ctx, 0, &format!("{{card {ALLY_AWAY}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.location(TIDETURNER),
            Some(Location::Base(0)),
            "move me to its location"
        );
        assert_eq!(
            ctx.location(ALLY_AWAY),
            Some(Location::Battlefield(fixtures::BF1)),
            "and it to my original location"
        );
    }

    #[test]
    fn the_may_is_a_skip_and_with_no_unit_elsewhere_it_asks_nothing_at_all() {
        let mut skipped = afloat();
        hidden(&mut skipped);
        let mut played = play_from_facedown(&mut skipped);
        let mut ctx = played.ctx();
        choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].targets.is_empty(), "nothing was chosen");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(TIDETURNER),
            Some(Location::Battlefield(fixtures::BF1))
        );

        let mut only = alone();
        hidden(&mut only);
        let mut played = play_from_facedown(&mut only);
        let ctx = played.ctx();
        assert!(
            ctx.blob.prompt.is_none(),
            "no friendly unit anywhere else, so the 0-of-1 target asks nothing"
        );
        assert!(
            ctx.blob.chain.is_empty() || ctx.blob.chain[0].targets.is_empty(),
            "and the trigger is not dropped for want of a target"
        );
    }

    #[test]
    fn played_from_hand_it_lands_in_base_and_may_swap_with_a_unit_at_a_battlefield() {
        let mut fixture = afloat();
        let action = fixtures::move_action(TIDETURNER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let moved = ctx.entry.expect("a drag is an entry move");
        let intent = legal::classify(&ctx, 0, &moved).expect("a unit plays from hand to base");
        assert_eq!(
            intent,
            legal::Intent::Play {
                card: TIDETURNER,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false
            }
        );
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(TIDETURNER), Some(Location::Base(0)));
        let item = match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play trigger asks for a unit, not {other:?}"),
        };
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {ALLY_HERE}}}"),
                "skip".to_string()
            ],
            "from base only the units at the battlefield are elsewhere · the ally in base is not"
        );
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, item, 0, &[ALLY_AWAY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit at my own location swaps nothing"
        );
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, item, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit is not friendly"
        );
        choose(&mut ctx, 0, &format!("{{card {ALLY_HERE}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(ALLY_HERE)]);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(TIDETURNER),
            Some(Location::Battlefield(fixtures::BF1)),
            "move me to its location"
        );
        assert_eq!(
            ctx.location(ALLY_HERE),
            Some(Location::Base(0)),
            "and it to my original location"
        );
        assert_eq!(
            ctx.location(ALLY_AWAY),
            Some(Location::Base(0)),
            "the unchosen ally stays"
        );
        let swaps = ctx
            .events
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
            .count();
        assert_eq!(swaps, 2, "one batch of two Swap moves");
        assert!(
            !ctx.card(ALLY_HERE).unwrap().exhausted,
            "a swap is not a standard move, so the ally is not exhausted by it"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn it_cannot_be_hidden_without_the_battlefield_nor_played_back_on_the_hiding_turn() {
        let mut unheld = afloat();
        unheld.blob.set_holder(fixtures::BF1, None);
        face_down(&mut unheld);
        assert_eq!(
            hide_now(&mut unheld, 0, fixtures::BF1),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "737.1.b · a battlefield you control"
        );

        let mut taken = afloat();
        face_down(&mut taken);
        taken
            .blob
            .card_state_mut(fixtures::THEIR_HAND_CARD)
            .hidden_at = Some(fixtures::BF1);
        assert_eq!(
            hide_now(&mut taken, 0, fixtures::BF1),
            Err(Refusal::Illegal(Reason::OneFacedown)),
            "106.4.b · one card per facedown zone"
        );

        let mut early = afloat();
        face_down(&mut early);
        hide_now(&mut early, 0, fixtures::BF1).unwrap();
        face_up(&mut early);
        {
            let ctx = early.ctx();
            assert_eq!(
                hide::play_legal(&ctx, 0, TIDETURNER),
                Err(Refusal::Illegal(Reason::HiddenThisTurn)),
                "737.1.b · the grant starts on the next turn"
            );
            assert_eq!(
                legal::classify(&ctx, 0, &entry(&ctx, TIDETURNER, fixtures::CHAIN)),
                Err(Refusal::Illegal(Reason::HiddenThisTurn))
            );
        }
        early.blob.core_mut().unwrap().turn += 1;
        assert_eq!(
            drag(&mut early, 0, TIDETURNER, fixtures::CHAIN),
            Ok(legal::Intent::PlayFromFacedown { card: TIDETURNER })
        );
    }
}
