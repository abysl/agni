use super::prelude::{
    a_card, asking, attach_gear, attached_to, card_target, charm_destination, done, friendly_gear,
    move_unit, play, spell, with_candidates, CHARM_DESTINATION, MOVABLE_FRIENDLY_UNIT,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const UNIT: usize = 0;
const DESTINATION: usize = 1;
const ATTACH: u8 = 1;

pub fn equipment_with_the_same_controller(ctx: &Ctx, unit: u32) -> Vec<u32> {
    let mut equipment: Vec<u32> = friendly_gear(ctx, ctx.controller(unit))
        .into_iter()
        .filter(|gear| {
            ctx.script(*gear)
                .is_some_and(|script| script.is_equipment())
        })
        .filter(|gear| attached_to(ctx, *gear) != Some(unit))
        .collect();
    equipment.sort_unstable();
    equipment
}

pub fn retreats_on_conquer_this_turn(ctx: &mut Ctx, _: &Item, unit: u32) {
    let turn = ctx.turn();
    ctx.narrate(format!(
        "{{card {unit}}} may move to its base when it conquers this turn (turn {turn})"
    ));
}

fn equipment_for_the_pursuer(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    card_target(ctx, item, UNIT)
        .map(|unit| equipment_with_the_same_controller(ctx, unit))
        .unwrap_or_default()
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn pursue(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    if stage.0 == ATTACH {
        let offered = equipment_with_the_same_controller(ctx, unit);
        if let Some(gear) = ctx
            .picks()
            .first()
            .copied()
            .filter(|picked| offered.contains(picked))
        {
            attach_gear(ctx, gear, unit);
        }
        retreats_on_conquer_this_turn(ctx, item, unit);
        return done();
    }
    if let Some(to) = charm_destination(ctx, item, unit, DESTINATION) {
        move_unit(ctx, item, unit, to);
    }
    if equipment_with_the_same_controller(ctx, unit).is_empty() {
        retreats_on_conquer_this_turn(ctx, item, unit);
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, ATTACH, 0, 1))
}

pub static CARD: Card = spell(
    "Relentless Pursuit",
    &[Keyword::Action],
    &[asking(
        with_candidates(
            play(
                &[
                    a_card(MOVABLE_FRIENDLY_UNIT, "a friendly unit to move"),
                    CHARM_DESTINATION,
                ],
                pursue,
            ),
            equipment_for_the_pursuer,
        ),
        "an Equipment to attach to it",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{equip, gear, is_attached, with_statics, Location, ONE_ENERGY};
    use crate::cards::{script_of, Trigger};
    use crate::engine::cleanup::{self, Established};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const PURSUIT: u32 = 90;
    const THEIR_PURSUIT: u32 = 91;
    const BLADE: u32 = 92;
    const SHIELD: u32 = 93;
    const TRINKET: u32 = 94;
    const THEIR_BLADE: u32 = 95;
    const FURY_RUNE: u32 = 100;
    const BODY_RUNE: u32 = 101;

    static BLADE_CARD: Card = with_statics(
        gear("Blade", &[Keyword::Equip(ONE_ENERGY)], &[equip(ONE_ENERGY)]),
        &[],
    );

    fn pursuit(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Relentless Pursuit", 2, 2);
        card.domain = vec!["Fury".into(), "Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(pursuit(PURSUIT, 0));
        fixture.table.cards.push(pursuit(THEIR_PURSUIT, 1));
        for (id, seat) in [(BLADE, 0), (SHIELD, 0), (TRINKET, 0), (THEIR_BLADE, 1)] {
            fixture
                .table
                .cards
                .push(fixtures::gear(id, fixtures::BASE, seat, "Blade", 1));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(FURY_RUNE, 0, "Fury", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BLADE, &BLADE_CARD)
            .with_script(SHIELD, &BLADE_CARD)
            .with_script(THEIR_BLADE, &BLADE_CARD);
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

    fn send_vi_to(ctx: &mut Ctx, zone: &str) {
        fixtures::play_from_hand(ctx, 0, PURSUIT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(ctx), ["{card 50}", "cancel"]);
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "both battlefields in play, the base it stands in excluded"
        );
        fixtures::choose(ctx, 0, zone).unwrap();
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_moves_a_friendly_unit_then_offers_its_controllers_equipment() {
        assert!(std::ptr::eq(
            script_of("Relentless Pursuit").unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, MOVABLE_FRIENDLY_UNIT);
        assert_eq!(ability.targets[1], CHARM_DESTINATION);
        assert_eq!(ability.question, Some("an Equipment to attach to it"));
        assert!(prompts::resume_questions().contains(&"an Equipment to attach to it"));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            equipment_with_the_same_controller(&ctx, fixtures::VI),
            [BLADE, SHIELD],
            "the Trinket is no Equipment and the enemy Blade has another controller"
        );
        attach_gear(&mut ctx, BLADE, fixtures::VI);
        assert_eq!(
            equipment_with_the_same_controller(&ctx, fixtures::VI),
            [SHIELD],
            "what it already wears is not offered"
        );
        assert_eq!(
            equipment_with_the_same_controller(&ctx, fixtures::THEIR_UNIT),
            [THEIR_BLADE]
        );
    }

    #[test]
    fn the_unit_moves_then_its_controller_may_attach_an_equipment_that_follows_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        send_vi_to(&mut ctx, "{zone 9}");
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, cause: MoveCause::Effect, .. } if *card == fixtures::VI
        )));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: ATTACH
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 93}", "skip"],
            "a may: skip is offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "{card 90}: choose an Equipment to attach to it (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, SHIELD), Some(fixtures::VI));
        assert_eq!(
            ctx.location(SHIELD),
            Some(Location::Battlefield(fixtures::BF1)),
            "the Equipment follows its wearer"
        );
        assert!(!is_attached(&ctx, BLADE));
        let turn = ctx.turn();
        assert!(ctx.blob.log.contains(&format!(
            "{{card 50}} may move to its base when it conquers this turn (turn {turn})"
        )));
        assert!(ctx.blob.log.contains(&"{card 90} resolves".to_string()));
        assert_eq!(ctx.card(PURSUIT).unwrap().zone, Some(fixtures::TRASH));
        assert!(
            ctx.blob.showdown.is_some(),
            "the effect move contests the empty battlefield and opens a showdown"
        );
    }

    #[test]
    fn skipping_leaves_the_gear_alone_and_without_equipment_nothing_is_asked() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        send_vi_to(&mut ctx, "{zone 10}");
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(!is_attached(&ctx, BLADE) && !is_attached(&ctx, SHIELD));
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2)),
            "onto the enemy's held battlefield, contesting it"
        );
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![BLADE, SHIELD].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        send_vi_to(&mut ctx, "{zone 9}");
        assert!(
            ctx.blob.prompt.is_none(),
            "the Trinket alone is no Equipment"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
    }

    #[test]
    fn enemy_units_are_refused_and_the_action_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_PURSUIT)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, PURSUIT).unwrap();
        for wrong in [
            fixtures::THEIR_UNIT,
            fixtures::SPRITE,
            BLADE,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(PURSUIT).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · floating turn effects: a granted trigger for the turn (\"When I conquer, you may move me to my base\") has no home, triggers::sources lists in-play scripts only and Grant carries no Ability; retreats_on_conquer_this_turn only narrates until a turn-scoped granted ability lands"]
    fn when_the_moved_unit_conquers_this_turn_its_controller_may_move_it_home() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        send_vi_to(&mut ctx, "{zone 9}");
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF1),
            Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "the granted may asks · {:?}",
            ctx.blob.log
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
    }
}
