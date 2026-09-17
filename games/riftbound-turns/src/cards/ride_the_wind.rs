use super::prelude::{
    a_card, card_target, charm_destination, done, move_unit, play, ready, spell, CHARM_DESTINATION,
    MOVABLE_FRIENDLY_UNIT,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

const UNIT: usize = 0;
const DESTINATION: usize = 1;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    if let Some(to) = charm_destination(ctx, item, unit, DESTINATION) {
        move_unit(ctx, item, unit, to);
    }
    if ready(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is readied"));
    }
    done()
}

pub static CARD: Card = spell(
    "Ride The Wind",
    &[Keyword::Action],
    &[play(
        &[
            a_card(MOVABLE_FRIENDLY_UNIT, "a friendly unit"),
            CHARM_DESTINATION,
        ],
        resolve,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Filter, TargetKind};
    use crate::engine::ctx::{EntryMove, Event, Location, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play as play_engine, priority, prompts, resume, settle};
    use crate::state::{Origin, PromptWhy, ShowdownStage, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::Pick;

    const RIDE: u32 = 90;
    const THEIR_RIDE: u32 = 91;
    const CHAOS_RUNE: u32 = 100;
    const THEIR_CHAOS: u32 = 101;
    const SPARE: u32 = 55;

    fn ride(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Ride The Wind", 2, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        let mut spare = fixtures::unit(SPARE, fixtures::BF1, 0, "Unsung Hero", 2);
        spare.exhausted = true;
        fixture.table.cards.push(spare);
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.table.cards.push(ride(RIDE, 0));
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
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
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

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        pick(ctx, seat, option)
    }

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_card_is_an_action_over_one_movable_friendly_unit_and_where_it_goes() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Ride The Wind").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Ride The Wind");
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.targets.len(), 2);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, MOVABLE_FRIENDLY_UNIT);
        assert_eq!(
            MOVABLE_FRIENDLY_UNIT,
            Filter::And(&[Filter::Unit, Filter::Friendly, Filter::Movable])
        );
        assert_eq!(ability.targets[1], CHARM_DESTINATION);
        assert_eq!(ability.targets[1].kind, TargetKind::Zone);
        assert_eq!(ability.targets[1].filter, Filter::DifferentLocationFrom(0));
        assert!(
            ability.candidates.is_none(),
            "352.3: the destination is a target chosen at play, not a resume hook"
        );
    }

    #[test]
    fn a_unit_at_base_names_its_destination_at_play_then_moves_there_and_readies() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, RIDE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 55}", "cancel"],
            "only this seat's units, and only the ones it may move"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a friendly unit (0 of 1)"
        );
        choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "every battlefield but the one it stands on, chosen with the unit (352.3)"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 1 }),
            "{card 90}: choose where it goes (0 of 1)"
        );
        assert!(
            ctx.effects.is_empty(),
            "nothing is paid before the destination"
        );
        choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Zone(fixtures::BF1)
            ],
            "both choices are locked in before anyone may react"
        );
        assert!(
            ctx.effects.contains(&Effect::exhaust(CHAOS_RUNE)),
            "two energy: {:?}",
            ctx.effects
        );
        assert!(
            ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move { card, zone, .. }
                    if *card == CHAOS_RUNE && Some(*zone) == ctx.zones.rune_deck
            )),
            "one Chaos power: {:?}",
            ctx.effects
        );
        assert!(ctx.blob.prompt.is_none(), "every choice is made at play");
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "nothing happens before the spell resolves"
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        both_pass(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing left to ask at resolution"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::VI,
            zone: fixtures::BF1,
            seat: 0,
            index: TOP
        }));
        assert!(ctx.effects.contains(&Effect::ready(fixtures::VI)));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::VI,
            from: Some(Location::Base(0)),
            to: Location::Battlefield(fixtures::BF1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert!(ctx.events.contains(&Event::Readied {
            card: fixtures::VI,
            by: 0
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} moves to {zone 9}".to_string()));
        assert!(ctx.blob.log.contains(&"{card 50} is readied".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(RIDE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.contains(&Event::PlayedSpell {
            item: 1,
            controller: 0,
            nth: 1
        }));
        assert!(ctx.blob.seat(0).played_main);
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            None,
            "seat 0 already holds this one"
        );
        assert!(ctx.blob.staged.is_empty() && ctx.blob.showdown.is_none());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_unit_at_a_battlefield_may_ride_home_or_onto_a_contested_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, RIDE).unwrap();
        choose(&mut ctx, 0, "{card 55}").unwrap();
        assert_eq!(
            labels(&ctx),
            ["{zone 8}", "{zone 10}", "cancel"],
            "its base and the far battlefield: no standing still, no Ganking check"
        );
        choose(&mut ctx, 0, "{zone 8}").unwrap();
        assert_eq!(
            ctx.location(SPARE),
            Some(Location::Battlefield(fixtures::BF1)),
            "a reacting seat sees where it is going, but it has not gone yet"
        );
        both_pass(&mut ctx);
        assert_eq!(ctx.location(SPARE), Some(Location::Base(0)));
        assert!(!ctx.card(SPARE).unwrap().exhausted);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 55} moves to their base".to_string()));
        assert!(ctx.blob.log.contains(&"{card 55} is readied".to_string()));
        assert!(ctx.blob.staged.is_empty(), "a retreat contests nothing");
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, RIDE).unwrap();
        choose(&mut ctx, 0, "{card 50}").unwrap();
        choose(&mut ctx, 0, "{zone 10}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert_eq!(
            ctx.blob.contester(fixtures::BF2),
            Some(0),
            "428: the destination is contested"
        );
        let showdown = ctx.blob.showdown.as_ref().expect("a combat opened");
        assert_eq!(showdown.zone, fixtures::BF2);
        assert!(showdown.combat);
        assert!(matches!(showdown.stage, ShowdownStage::Open));
        assert!(ctx.is_attacker(fixtures::VI));
        assert!(ctx.is_defender(fixtures::SPRITE));
    }

    #[test]
    fn a_target_that_left_the_board_moves_nothing_and_readies_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, RIDE).unwrap();
        choose(&mut ctx, 0, "{card 50}").unwrap();
        choose(&mut ctx, 0, "{zone 9}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::TRASH, 0), 0)
            .unwrap();
        both_pass(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing is asked at resolution");
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.effects.contains(&Effect::ready(fixtures::VI)));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert_eq!(ctx.card(RIDE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn it_is_refused_off_turn_and_an_enemy_or_move_locked_unit_is_no_target() {
        let mut fixture = armed();
        fixture.table.cards.push(ride(THEIR_RIDE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(THEIR_CHAOS, 1, "Chaos", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_RIDE)),
            Err(Refusal::NotYourTurn),
            "an Action is playable on your turn or in a showdown, not in the other seat's open state"
        );
        play_from_hand(&mut ctx, 0, RIDE).unwrap();
        for wrong in [
            fixtures::THEIR_UNIT,
            fixtures::SPRITE,
            fixtures::HAND_UNIT,
            fixtures::GROUNDS,
            RIDE,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        ctx.lock_move(fixtures::VI);
        assert_eq!(
            labels(&ctx),
            ["{card 55}", "cancel"],
            "a move-locked unit leaves the candidates of its owner's move effect"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(RIDE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
        drop(ctx);

        let mut poor = armed();
        poor.table.card_mut(CHAOS_RUNE).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, RIDE)),
            Err(Refusal::NoPowerOf),
            "the energy is there but no Chaos rune is"
        );
    }
}
