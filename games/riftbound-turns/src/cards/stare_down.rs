use super::emperors_divide::send_home;
use super::prelude::{
    a_battlefield, a_friendly_unit, card_target, done, gain_xp, play, spell, zone_target, Location,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const XP: u8 = 1;
const STARER: usize = 0;
const BATTLEFIELD: usize = 1;

pub fn outstared(ctx: &Ctx, starer: u32, zone: u16) -> Vec<u32> {
    let seat = ctx.controller(starer);
    let might = ctx.current_might(starer);
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat && ctx.current_might(*unit) < might)
        .collect()
}

fn stare(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let starer = card_target(ctx, item, STARER);
    let zone = zone_target(item, BATTLEFIELD).filter(|zone| ctx.zones.is_battlefield(*zone));
    if let (Some(starer), Some(zone)) = (starer, zone) {
        let cowed = outstared(ctx, starer, zone);
        if cowed.is_empty() {
            ctx.narrate(format!(
                "no enemy unit at {{zone {zone}}} has less Might than {{card {starer}}}"
            ));
        }
        for unit in cowed {
            send_home(ctx, item, unit);
        }
    }
    gain_xp(ctx, item.controller, XP);
    done()
}

pub static CARD: Card = spell(
    "Stare Down",
    &[],
    &[play(
        &[
            a_friendly_unit("a friendly unit"),
            a_battlefield("a battlefield"),
        ],
        stare,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::equipment;
    use crate::cards::prelude::{attach_gear, might_this_turn};
    use crate::cards::{script_of, Filter, TargetKind, Trigger};
    use crate::engine::ctx::{EntryMove, Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const STARE: u32 = 90;
    const THEIR_STARE: u32 = 91;
    const WEAK: u32 = 92;
    const EQUAL: u32 = 93;
    const MINE_THERE: u32 = 94;

    fn stare_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Stare Down", 2, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn standoff() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(stare_card(STARE, 0));
        fixture.table.cards.push(stare_card(THEIR_STARE, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(WEAK, fixtures::BF2, 1, "Poro", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(EQUAL, fixtures::BF2, 1, "Brute", 3));
        fixture
            .table
            .cards
            .push(fixtures::unit(MINE_THERE, fixtures::BF2, 0, "Recruit", 1));
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

    #[test]
    fn the_script_is_a_plain_spell_over_a_friendly_unit_and_a_battlefield() {
        assert!(std::ptr::eq(script_of("Stare Down").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(
            ability.targets[0].filter,
            crate::cards::prelude::FRIENDLY_UNIT
        );
        assert_eq!(ability.targets[1].kind, TargetKind::Zone);
        assert_eq!(ability.targets[1].filter, Filter::AtBattlefield);
        assert_eq!(XP, 1);
    }

    #[test]
    fn the_weaker_enemies_there_go_home_the_equal_one_stays_and_you_gain_one_xp() {
        let mut fixture = standoff();
        let mut ctx = fixture.ctx();
        assert_eq!(outstared(&ctx, fixtures::VI, fixtures::BF2), [WEAK]);
        fixtures::play_from_hand(&mut ctx, 0, STARE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 94}", "cancel"],
            "your units, wherever they stand"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "the two battlefields in play, never a base"
        );
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Zone(fixtures::BF2)
            ]
        );
        assert_eq!(ctx.xp(0), 0, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.location(WEAK), Some(Location::Base(1)));
        assert!(ctx.events.contains(&Event::Moved {
            card: WEAK,
            from: Some(Location::Battlefield(fixtures::BF2)),
            to: Location::Base(1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        assert_eq!(
            ctx.location(EQUAL),
            Some(Location::Battlefield(fixtures::BF2)),
            "less Might, not less or equal"
        );
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.location(MINE_THERE),
            Some(Location::Battlefield(fixtures::BF2)),
            "your own weak Recruit is no enemy"
        );
        assert_eq!(ctx.xp(0), 1);
        assert!(ctx.blob.log.contains(&"{seat 0} gains 1 XP".to_string()));
        assert_eq!(ctx.card(STARE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_might_is_read_as_the_spell_resolves_so_a_pump_in_response_widens_the_stare() {
        let mut fixture = standoff();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STARE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, fixtures::VI, 2, None);
        fixtures::pass_until_open(&mut ctx);
        for unit in [WEAK, EQUAL, fixtures::SPRITE] {
            assert_eq!(
                ctx.location(unit),
                Some(Location::Base(1)),
                "{unit} has less than 5 Might"
            );
        }
        assert_eq!(ctx.xp(0), 1);
    }

    #[test]
    fn a_starer_that_left_the_board_moves_nobody_but_the_xp_is_still_gained() {
        let mut fixture = standoff();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STARE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(WEAK),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert_eq!(ctx.xp(0), 1, "the battlefield is still a legal target");
        assert_eq!(ctx.card(STARE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn it_waits_for_your_turn_and_refuses_an_enemy_unit_or_a_base_as_the_battlefield() {
        let mut fixture = standoff();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STARE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, STARE).unwrap();
        for wrong in [fixtures::SPRITE, WEAK, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        for wrong in [fixtures::BASE, fixtures::TRASH, fixtures::HAND] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[u32::from(wrong)]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(STARE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 0);
    }

    #[test]
    fn a_jagged_cutlass_wearer_stands_while_the_other_weaker_enemies_go_home() {
        const CUTLASS: u32 = 96;
        let mut fixture = standoff();
        fixture
            .table
            .cards
            .push(equipment(CUTLASS, 1, "Jagged Cutlass", 3, "Body"));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, CUTLASS, WEAK);
        assert_eq!(ctx.current_might(WEAK), 3, "the Poro wears +2");
        fixtures::play_from_hand(&mut ctx, 0, STARE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, fixtures::VI, 2, None);
        let mut cowed = outstared(&ctx, fixtures::VI, fixtures::BF2);
        cowed.sort_unstable();
        assert_eq!(
            cowed,
            [fixtures::SPRITE, WEAK, EQUAL],
            "the wearer is outstared like the rest"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(WEAK),
            Some(Location::Battlefield(fixtures::BF2)),
            "an enemy spell can't move the wearer"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {WEAK}}} can't be moved by {{card {STARE}}}"
        )));
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::Moved { card, .. } if *card == WEAK
        )));
        for unit in [EQUAL, fixtures::SPRITE] {
            assert_eq!(
                ctx.location(unit),
                Some(Location::Base(1)),
                "{unit} goes home"
            );
        }
        assert_eq!(ctx.xp(0), 1);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }
}
