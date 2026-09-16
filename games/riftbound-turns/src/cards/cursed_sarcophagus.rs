use super::prelude::{
    activated, asking, banish_by, done, exhausting_self, gear, named, play, with_candidates,
    Location,
};
use super::the_zero_drive::units_banished_with;
use super::{Card, Cost, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;
use crate::engine::play as play_engine;
use crate::state::{Origin, TargetRef};

pub const ENTOMB: u8 = 0;
pub const RAISE: u8 = 1;
pub const PICK: u8 = 1;

pub fn units_in_the_trash_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.trash_of(seat)
        .into_iter()
        .filter(|card| ctx.is_unit(*card))
        .collect()
}

fn entomb(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let me = item.kind.source();
    let units = units_in_the_trash_of(ctx, seat);
    if units.is_empty() {
        ctx.narrate(format!(
            "{{seat {seat}}} has no unit in their trash to banish"
        ));
        return done();
    }
    let mut banished = 0;
    for unit in units {
        if banish_by(ctx, unit, item.controller) {
            banished += 1;
            ctx.narrate(format!("{{card {unit}}} is banished with {{card {me}}}"));
        }
    }
    ctx.narrate(format!("{{card {me}}} banishes {banished} from the trash"));
    done()
}

pub fn entombed(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != PICK {
        return Vec::new();
    }
    units_banished_with(ctx, item.kind.source())
        .into_iter()
        .filter(|unit| ctx.in_banishment(*unit) && ctx.is_unit(*unit))
        .map(TargetRef::Card)
        .collect()
}

fn raise(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    let me = item.kind.source();
    if stage.0 != PICK {
        if entombed(ctx, item, Stage(PICK)).is_empty() {
            ctx.narrate(format!("no unit is banished with {{card {me}}}"));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, PICK, 1, 1));
    }
    let Some(unit) = ctx
        .picks()
        .first()
        .copied()
        .filter(|unit| entombed(ctx, item, stage).contains(&TargetRef::Card(*unit)))
    else {
        ctx.narrate(format!("no unit is played from {{card {me}}}"));
        return done();
    };
    let _ = play_engine::begin(
        ctx,
        seat,
        unit,
        Origin::Banishment,
        Some(Location::Base(seat)),
    );
    done()
}

pub static CARD: Card = gear(
    "Cursed Sarcophagus",
    &[],
    &[
        play(&[], entomb),
        named(
            asking(
                with_candidates(
                    exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], raise)),
                    entombed,
                ),
                "a unit banished with this to play",
            ),
            "play a unit banished with this",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SARCOPHAGUS: u32 = 90;
    const FALLEN: [u32; 2] = [91, 92];
    const SPENT_SPELL: u32 = 93;
    const THEIR_FALLEN: u32 = 94;
    const CHAOS_RUNES: [u32; 2] = [46, 47];

    fn sarcophagus(zone: u16, exhausted: bool) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Chaos".into()],
            exhausted,
            ..fixtures::gear(SARCOPHAGUS, zone, 0, CARD.name, 4)
        }
    }

    fn crypt(zone: u16, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sarcophagus(zone, exhausted));
        for fallen in FALLEN {
            fixture
                .table
                .cards
                .push(fixtures::unit(fallen, fixtures::TRASH, 0, "Fallen", 2));
        }
        fixture.table.cards.push(fixtures::spell(
            SPENT_SPELL,
            fixtures::TRASH,
            0,
            "Spark",
            1,
            0,
        ));
        fixture.table.cards.push(fixtures::unit(
            THEIR_FALLEN,
            fixtures::TRASH,
            1,
            "Raider",
            2,
        ));
        for rune in CHAOS_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_banishes_on_play_and_has_one_exhaust_activation_over_the_banished_with_seam() {
        assert!(std::ptr::eq(
            script_of("Cursed Sarcophagus").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let entomb = &CARD.abilities[usize::from(ENTOMB)];
        assert_eq!(entomb.trigger, Trigger::Play);
        assert!(entomb.targets.is_empty() && !entomb.optional);
        let raise = &CARD.abilities[usize::from(RAISE)];
        assert_eq!(raise.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(raise.self_cost, SelfCost::Exhaust);
        assert_eq!(raise.cost, Some(Cost::FREE));
        assert!(raise.candidates.is_some());
        assert_eq!(raise.question, Some("a unit banished with this to play"));
        assert!(raise.targets.is_empty());
    }

    #[test]
    fn playing_it_banishes_every_unit_in_your_trash_and_leaves_spells_and_their_trash_alone() {
        let mut fixture = crypt(fixtures::HAND, false);
        let mut ctx = fixture.ctx();
        assert_eq!(units_in_the_trash_of(&ctx, 0), FALLEN.to_vec());
        fixtures::play_from_hand(&mut ctx, 0, SARCOPHAGUS).unwrap();
        assert!(ctx.on_board(SARCOPHAGUS));
        assert!(
            !ctx.card(SARCOPHAGUS).unwrap().exhausted,
            "149.1 · gear enters ready"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == SARCOPHAGUS && index == ENTOMB
        ));
        assert!(ctx.in_trash(FALLEN[0]), "nothing until it resolves");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for fallen in FALLEN {
            assert!(ctx.in_banishment(fallen));
            assert!(ctx.events.iter().any(|event| matches!(
                event,
                Event::Banished {
                card,
                owner: 0,
                by: 0,
                ..
            } if *card == fallen
            )));
            assert!(ctx.blob.log.contains(&format!(
                "{{card {fallen}}} is banished with {{card {SARCOPHAGUS}}}"
            )));
        }
        assert!(ctx.in_trash(SPENT_SPELL), "a spell is not a unit");
        assert!(ctx.in_trash(THEIR_FALLEN), "their trash is theirs");
        assert!(units_in_the_trash_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SARCOPHAGUS}}} banishes 2 from the trash")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_an_empty_trash_the_play_trigger_banishes_nothing() {
        let mut fixture = crypt(fixtures::HAND, false);
        fixture
            .table
            .cards
            .retain(|card| !FALLEN.contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SARCOPHAGUS).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.in_trash(SPENT_SPELL));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no unit in their trash to banish".to_string()));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Banished { .. })));
    }

    #[test]
    fn today_the_activation_exhausts_it_and_finds_no_unit_banished_with_it() {
        let mut fixture = crypt(fixtures::BASE, false);
        for fallen in FALLEN {
            fixture.table.card_mut(fallen).unwrap().zone = Some(fixtures::BANISHMENT);
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.in_banishment(FALLEN[0]));
        assert!(units_banished_with(&ctx, SARCOPHAGUS).is_empty());
        assert!(entombed(
            &ctx,
            &Item::new(
                0,
                ItemKind::Ability {
                    source: SARCOPHAGUS,
                    index: RAISE
                },
                0,
                crate::state::Origin::Board
            ),
            Stage(PICK)
        )
        .is_empty());
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == SARCOPHAGUS && offer.index == RAISE)
            .unwrap();
        assert!(offer.enabled);
        assert_eq!(
            offer.label,
            format!("{{card {SARCOPHAGUS}}}: play a unit banished with this (exhaust)")
        );
        activate::activate(&mut ctx, 0, SARCOPHAGUS, RAISE).unwrap();
        assert!(ctx.card(SARCOPHAGUS).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_banishment(FALLEN[0]), "the link is the seam");
        assert!(ctx
            .blob
            .log
            .contains(&format!("no unit is banished with {{card {SARCOPHAGUS}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_spent_sarcophagus_and_the_other_seat_are_refused() {
        let mut spent = crypt(fixtures::BASE, true);
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SARCOPHAGUS, RAISE),
            Err(Refusal::Exhausted)
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, SARCOPHAGUS, RAISE),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(ctx.blob.queue.is_empty());
        drop(ctx);
        let mut held = crypt(fixtures::HAND, false);
        let mut ctx = held.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SARCOPHAGUS, RAISE),
            Err(Refusal::Illegal(Reason::NotInPlay))
        );
    }

    #[test]
    #[ignore = "engine gap · a banished-with link (the Zero Drive row): the engine keeps no record of which cards a source banished, so the_zero_drive::units_banished_with reads nothing; and a play from Banishment is free today (cost::origin_cost) where the Sarcophagus must charge the printed cost; with both the activation offers the entombed units and plays the pick for its cost"]
    fn the_activation_offers_the_units_it_banished_and_plays_one_for_its_printed_cost() {
        let mut fixture = crypt(fixtures::HAND, false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SARCOPHAGUS).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(units_banished_with(&ctx, SARCOPHAGUS), FALLEN.to_vec());
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, SARCOPHAGUS, RAISE).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", FALLEN[0]),
                format!("{{card {}}}", FALLEN[1])
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", FALLEN[0])).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(FALLEN[0]));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 2,
            "its two energy are paid"
        );
    }
}
