use super::prelude::{
    a_card, at_level, card_target, charm_destination, done, move_unit, play, spell, stun, target,
    xp_of, CHARM_DESTINATION, ENEMY_UNIT, MOVABLE_ENEMY_UNIT,
};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const LEVEL: u8 = 6;
const MOVED: usize = 0;
const DESTINATION: usize = 1;
const STUNNED: usize = 2;

pub const LEVEL_SIX_STUN_TARGET: TargetSpec = at_level(
    LEVEL,
    1,
    target(
        ENEMY_UNIT,
        0,
        1,
        TargetKind::Card,
        "an enemy unit to stun at Level 6",
    ),
);

pub fn leveled(ctx: &Ctx, seat: u8) -> bool {
    xp_of(ctx, seat) >= i32::from(LEVEL)
}

fn strike(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, MOVED) {
        if let Some(to) = charm_destination(ctx, item, unit, DESTINATION) {
            move_unit(ctx, item, unit, to);
        }
    }
    let Some(unit) = card_target(ctx, item, STUNNED) else {
        return done();
    };
    if leveled(ctx, item.controller) {
        stun(ctx, unit);
    } else {
        ctx.narrate(format!(
            "{{card {}}} · Level {LEVEL} is not active, {{card {unit}}} is not stunned",
            item.kind.source()
        ));
    }
    done()
}

pub static CARD: Card = spell(
    "Skyward Strike",
    &[],
    &[play(
        &[
            a_card(MOVABLE_ENEMY_UNIT, "an enemy unit to move"),
            CHARM_DESTINATION,
            LEVEL_SIX_STUN_TARGET,
        ],
        strike,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const STRIKE: u32 = 90;
    const THEIR_STRIKE: u32 = 91;
    const CALM_RUNE: u32 = 46;

    fn strike_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Skyward Strike", 2, 1);
        card.domain = vec!["Calm".into()];
        card
    }

    fn sky(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(strike_card(STRIKE, 0));
        fixture.table.cards.push(strike_card(THEIR_STRIKE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.set_xp(0, xp);
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
    fn the_script_moves_one_enemy_unit_and_carries_an_optional_stun_target_for_level_six() {
        assert!(std::ptr::eq(script_of("Skyward Strike").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty(), "no Action, no Reaction");
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 3);
        assert_eq!(ability.targets[0].filter, MOVABLE_ENEMY_UNIT);
        assert_eq!(ability.targets[1], CHARM_DESTINATION);
        assert_eq!(ability.targets[2], LEVEL_SIX_STUN_TARGET);
        assert_eq!(
            (LEVEL_SIX_STUN_TARGET.min, LEVEL_SIX_STUN_TARGET.max),
            (0, 1)
        );
        assert_eq!(LEVEL, 6);
        let mut fixture = sky(5);
        let ctx = fixture.ctx();
        assert!(!leveled(&ctx, 0));
        assert!(!leveled(&ctx, 1));
        let mut fixture = sky(6);
        let ctx = fixture.ctx();
        assert!(leveled(&ctx, 0), "824.1.c · six XP is Level 6");
    }

    #[test]
    fn below_level_six_the_enemy_is_moved_and_nothing_is_stunned() {
        let mut fixture = sky(5);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "the other seat's units"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(fixtures::labels(&ctx), ["{zone 9}", "{zone 10}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(ctx.blob.prompt.is_none(), "every choice is made at play");
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Zone(fixtures::BF1)
            ],
            "721.2 · the inactive stun line chooses nothing"
        );
        assert_eq!(ctx.blob.chain[0].spec_counts, [1, 1, 0]);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(1),
            "the mover's controller contests, not the caster"
        );
        assert!(
            !ctx.is_stunned(fixtures::SPRITE) && !ctx.is_stunned(fixtures::THEIR_UNIT),
            "824.1.d · below six XP the Level line is inactive"
        );
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn at_level_six_the_enemy_is_moved_home_and_a_second_enemy_is_stunned() {
        let mut fixture = sky(6);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(ctx.blob.prompt.is_none(), "every choice is made at play");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Base(1)),
            "the Sprite goes home to its own base"
        );
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert_eq!(ctx.combat_might(fixtures::THEIR_UNIT), 0);
        assert!(ctx.blob.log.contains(&"{card 81} is stunned".to_string()));
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_same_enemy_may_be_moved_and_stunned() {
        let mut fixture = sky(6);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 2, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "at Level 6 the stun target is required"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_level_reached_in_response_stuns_nothing_and_a_level_lost_in_response_cancels_the_stun() {
        let mut fixture = sky(5);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.blob.chain[0].targets.len(),
            2,
            "721.2 · the stun line is inactive, so it took no target"
        );
        ctx.score_xp(0, 1);
        assert!(leveled(&ctx, 0));
        fixtures::pass_until_open(&mut ctx);
        assert!(
            !ctx.is_stunned(fixtures::SPRITE) && !ctx.is_stunned(fixtures::THEIR_UNIT),
            "355.5 · no target was chosen at play, so the line stuns nothing"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let mut leveled_up = sky(6);
        let mut ctx = leveled_up.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert!(ctx.spend_xp(0, 1));
        assert!(!leveled(&ctx, 0));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            !ctx.is_stunned(fixtures::SPRITE),
            "824.1.d · the line went inactive before the spell resolved"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} · Level 6 is not active, {card 60} is not stunned".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn it_waits_for_your_turn_and_refuses_friendly_units_for_either_pick() {
        let mut fixture = sky(6);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STRIKE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        for wrong in [fixtures::VI, fixtures::HAND_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[u32::from(fixtures::BF2)]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "where it already stands"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        for wrong in [fixtures::VI, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 2, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is no enemy unit to stun"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn below_level_six_no_stun_target_is_asked_and_at_level_six_it_is_required() {
        let mut fixture = sky(5);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no third pick below Level 6: {:?}",
            fixtures::labels(&ctx)
        );
        let mut leveled_up = sky(6);
        let mut ctx = leveled_up.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "at Level 6 the stun target is required, so there is no skip"
        );
    }
}
