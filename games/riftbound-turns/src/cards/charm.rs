use super::prelude::{
    a_card, card_target, charm_destination, done, move_unit, play, spell, CHARM_DESTINATION,
    MOVABLE_ENEMY_UNIT,
};
use super::{Card, Flow, Item, Stage};
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
    done()
}

pub static CARD: Card = spell(
    "Charm",
    &[],
    &[play(
        &[
            a_card(MOVABLE_ENEMY_UNIT, "an enemy unit"),
            CHARM_DESTINATION,
        ],
        resolve,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Filter, TargetKind};
    use crate::engine::ctx::{EntryMove, Event, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, prompts, resume, settle, targets};
    use crate::state::{Origin, PromptWhy, TargetRef, FLAG_NO_MOVE_BY_OWNER};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::CardInfo;

    const CHARM: u32 = 90;
    const THEIR_CHARM: u32 = 91;
    const THEIR_CALM: u32 = 46;
    const ENERGY: u8 = 1;
    const POWER: u8 = 1;

    fn charm(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Charm", ENERGY, POWER);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(charm(CHARM, 0));
        fixture.table.cards.push(charm(THEIR_CHARM, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(THEIR_CALM, 1, "Calm", false));
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
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn option(ctx: &Ctx, label: &str) -> u16 {
        labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx))) as u16
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let picked = option(ctx, label);
        pick(ctx, seat, picked)
    }

    fn pass_both(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_spell_over_one_enemy_unit_and_where_it_goes() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Charm").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Charm");
        assert!(CARD.keywords.is_empty(), "no Action, no Reaction");
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.targets.len(), 2);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, MOVABLE_ENEMY_UNIT);
        assert_eq!(
            MOVABLE_ENEMY_UNIT,
            Filter::And(&[Filter::Unit, Filter::Enemy, Filter::Movable])
        );
        assert_eq!(ability.targets[1], CHARM_DESTINATION);
        assert_eq!(ability.targets[1].kind, TargetKind::Zone);
        assert_eq!(ability.targets[1].filter, Filter::DifferentLocationFrom(0));
        assert_eq!((ability.targets[1].min, ability.targets[1].max), (1, 1));
        assert!(
            ability.candidates.is_none(),
            "352.3: the destination is a target, not a resume hook"
        );
    }

    #[test]
    fn the_destination_is_chosen_with_the_target_and_the_base_is_the_charmed_units_own() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, CHARM).unwrap();
        choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            labels(&ctx),
            ["{zone 8}", "{zone 9}", "cancel"],
            "the destination is a second target: its own base or the other battlefield"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 1 }),
            "{card 90}: choose where it goes (0 of 1)"
        );
        let pending = ctx.blob.pending(1).unwrap().item.clone();
        assert_eq!(
            targets::zone_location(&ctx, &pending, 0, &CHARM_DESTINATION, fixtures::BASE),
            Some(Location::Base(1)),
            "the one base zone reads as the charmed unit's own base"
        );
        assert_eq!(
            targets::zone_location(&ctx, &pending, 0, &CHARM_DESTINATION, fixtures::BF1),
            Some(Location::Battlefield(fixtures::BF1))
        );
        choose(&mut ctx, 0, "{zone 8}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::SPRITE),
                TargetRef::Zone(fixtures::BASE)
            ],
            "both choices are locked in before anyone may react"
        );
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2)),
            "nothing moves until the spell resolves"
        );
    }

    #[test]
    fn charm_pulls_an_enemy_unit_off_a_battlefield_back_to_its_own_base() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, CHARM).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "only the other seat's units"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose an enemy unit (0 of 1)"
        );
        choose(&mut ctx, 0, "{card 60}").unwrap();
        assert!(
            ctx.effects.is_empty(),
            "nothing is paid before the destination"
        );
        choose(&mut ctx, 0, "{zone 8}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::SPRITE),
                TargetRef::Zone(fixtures::BASE)
            ]
        );
        assert!(
            ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move { card, zone, .. }
                    if *card == 42 && Some(*zone) == ctx.zones.rune_deck
            )),
            "one Calm power recycled: {:?}",
            ctx.effects
        );
        assert_eq!(
            ctx.effects
                .iter()
                .filter(
                    |effect| matches!(effect, Effect::Annotate { key, .. } if key == "exhausted")
                )
                .count(),
            1,
            "one energy: {:?}",
            ctx.effects
        );
        assert!(ctx.blob.prompt.is_none(), "every choice is made at play");
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        pass_both(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing left to ask at resolution"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Base(1)),
            "a charmed unit goes home to its own base, not the caster's"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::SPRITE,
            zone: fixtures::BASE,
            seat: 1,
            index: TOP
        }));
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::SPRITE,
            from: Some(Location::Battlefield(fixtures::BF2)),
            to: Location::Base(1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} moves to their base".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(CHARM).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.blob.holder(fixtures::BF2),
            None,
            "the last unit charmed away leaves the hold behind"
        );
        assert_eq!(
            ctx.blob.log,
            [
                "{seat 0} plays {card 90}",
                "{seat 0} passes",
                "{seat 1} passes",
                "{card 60} moves to their base",
                "{card 90} resolves"
            ],
            "184.4.c is silent: the hold is simply gone"
        );
        assert!(ctx.blob.showdown.is_none());
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.blob.seat(0).played_main);
    }

    #[test]
    fn charm_pushes_an_enemy_unit_onto_a_battlefield_and_the_contest_it_makes_stages_a_showdown() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, CHARM).unwrap();
        choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "both battlefields; its base is where it already stands"
        );
        choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Zone(fixtures::BF1)
            ]
        );
        pass_both(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::THEIR_UNIT,
            from: Some(Location::Base(1)),
            to: Location::Battlefield(fixtures::BF1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(1),
            "the mover's controller contests, not the caster"
        );
        assert_eq!(ctx.card(CHARM).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_move_locked_enemy_unit_is_still_charmed_and_a_lone_destination_is_still_confirmed() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().zone = Some(fixtures::TRASH);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.lock_move(fixtures::THEIR_UNIT);
        assert!(ctx.has_flag(fixtures::THEIR_UNIT, FLAG_NO_MOVE_BY_OWNER));
        assert_eq!(ctx.zones.battlefields, [fixtures::BF2]);
        play_from_hand(&mut ctx, 0, CHARM).unwrap();
        assert!(
            labels(&ctx).contains(&"{card 81}".to_string()),
            "Vex's lock refuses its owner's moves, not the opponent's Charm"
        );
        choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            labels(&ctx),
            ["{zone 10}", "cancel"],
            "one legal destination, and the spell may still be cancelled"
        );
        choose(&mut ctx, 0, "{zone 10}").unwrap();
        pass_both(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} moves to {zone 10}".to_string()));
        assert!(ctx.has_flag(fixtures::THEIR_UNIT, FLAG_NO_MOVE_BY_OWNER));
        assert_eq!(ctx.card(CHARM).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_target_that_left_the_board_is_not_moved_when_the_spell_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, CHARM).unwrap();
        choose(&mut ctx, 0, "{card 81}").unwrap();
        choose(&mut ctx, 0, "{zone 9}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::TRASH, 1),
                1,
            )
            .unwrap();
        pass_both(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "no target, nothing to ask");
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert_eq!(ctx.card(CHARM).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    fn a_destination_the_unit_already_reached_before_resolution_is_mistargeted() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, CHARM).unwrap();
        choose(&mut ctx, 0, "{card 60}").unwrap();
        choose(&mut ctx, 0, "{zone 8}").unwrap();
        ctx.actor = 1;
        assert_eq!(
            ctx.move_unit(fixtures::SPRITE, Location::Base(1), MoveCause::Effect),
            Moved::Moved,
            "a reaction walks the Sprite home first"
        );
        ctx.actor = 0;
        let moved_before = ctx.events.len();
        assert!(
            !targets::valid(&ctx, &ctx.blob.chain[0], 1),
            "356.3.e.2: the base is now where it stands, so the destination is illegal"
        );
        assert!(targets::valid(&ctx, &ctx.blob.chain[0], 0));
        pass_both(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(fixtures::SPRITE), Some(Location::Base(1)));
        assert_eq!(
            ctx.events
                .iter()
                .skip(moved_before)
                .filter(|event| matches!(event, Event::Moved { .. }))
                .count(),
            0,
            "356.3.e.6: the move instruction is ignored"
        );
        assert_eq!(ctx.card(CHARM).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn charm_is_refused_off_turn_and_a_friendly_unit_is_not_a_legal_target() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CHARM)),
            Err(Refusal::NotYourTurn),
            "no Action or Reaction keyword: only on your own turn"
        );
        play_from_hand(&mut ctx, 0, CHARM).unwrap();
        for wrong in [
            fixtures::VI,
            fixtures::HAND_UNIT,
            fixtures::THEIR_HAND_CARD,
            fixtures::GROUNDS,
            CHARM,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit on the board"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "one enemy unit is required"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        choose(&mut ctx, 0, "{card 60}").unwrap();
        for wrong in [
            fixtures::BF2,
            fixtures::TRASH,
            fixtures::HAND,
            fixtures::CHAIN,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[u32::from(wrong)]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is where it stands or not a location at all"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a card is not a destination"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a destination is required"
        );
        assert!(
            ctx.blob.prompt.is_some(),
            "the destination prompt stays open"
        );
        assert!(ctx.effects.is_empty(), "still nothing paid");
        choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CHARM).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn with_no_enemy_unit_on_the_board_there_is_nothing_to_charm() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.owner != 1 || !card.is_kind("Unit"));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, CHARM).unwrap();
        let offered = labels(&ctx);
        assert_eq!(offered, ["cancel"], "no enemy unit to choose");
        choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CHARM).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
