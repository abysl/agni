use super::prelude::{a_friendly_unit, card_target, done, might_this_turn, play, spell, Location};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const MIGHT: i16 = 3;
pub const XP: u8 = 2;

pub fn wins_a_combat(ctx: &Ctx, event: &Event, unit: u32) -> bool {
    let Event::CombatWon { zone, seat } = event else {
        return false;
    };
    ctx.controller(unit) == *seat && ctx.location(unit) == Some(Location::Battlefield(*zone))
}

pub fn gains_xp_when_it_wins_a_combat_this_turn(ctx: &mut Ctx, item: &Item, unit: u32) {
    let seat = item.controller;
    let turn = ctx.turn();
    ctx.narrate(format!(
        "{{seat {seat}}} gains {XP} XP when {{card {unit}}} wins a combat this turn (turn {turn})"
    ));
}

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} might this turn"));
        gains_xp_when_it_wins_a_combat_this_turn(ctx, item, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Grim Resolve",
    &[Keyword::Action],
    &[play(&[a_friendly_unit("a friendly unit")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, settle};
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const RESOLVE: u32 = 90;
    const THEIR_RESOLVE: u32 = 91;
    const BODY_RUNE: u32 = 100;

    fn resolve_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Grim Resolve", 2, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(resolve_card(RESOLVE, 0));
        fixture.table.cards.push(resolve_card(THEIR_RESOLVE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
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

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn cast_on_vi(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, RESOLVE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_an_action_over_one_friendly_unit() {
        assert!(std::ptr::eq(script_of("Grim Resolve").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!((MIGHT, XP), (3, 2));
    }

    #[test]
    fn the_friendly_unit_gets_three_might_until_the_end_of_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESOLVE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "friendly units only"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert_eq!(might_counter(&ctx, fixtures::VI), 3);
        let turn = ctx.turn();
        assert_eq!(
            ctx.state_of(fixtures::VI)
                .unwrap()
                .might
                .iter()
                .map(|held| (held.delta, held.until))
                .collect::<Vec<_>>(),
            [(MIGHT, Expiry::EndOfTurn(turn))]
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +3 might this turn".to_string()));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} gains 2 XP when {{card 50}} wins a combat this turn (turn {turn})"
        )));
        ctx.expire(Expiry::EndOfTurn(turn));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the bonus ends with the turn"
        );
        assert_eq!(ctx.card(RESOLVE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn wins_a_combat_reads_the_units_battlefield_and_its_controllers_win_only() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        let won_here = Event::CombatWon {
            zone: fixtures::BF1,
            seat: 0,
        };
        assert!(wins_a_combat(&ctx, &won_here, fixtures::VI));
        assert!(
            !wins_a_combat(&ctx, &won_here, fixtures::THEIR_UNIT),
            "466.3.c · the enemy in its base inherits nothing"
        );
        assert!(!wins_a_combat(
            &ctx,
            &Event::CombatWon {
                zone: fixtures::BF1,
                seat: 1
            },
            fixtures::VI
        ));
        assert!(!wins_a_combat(
            &ctx,
            &Event::CombatWon {
                zone: fixtures::BF2,
                seat: 0
            },
            fixtures::VI
        ));
        assert!(!wins_a_combat(
            &ctx,
            &Event::CombatLost {
                zone: fixtures::BF1,
                seat: 0
            },
            fixtures::VI
        ));
    }

    #[test]
    fn an_enemy_unit_is_refused_and_an_action_has_no_window_off_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_RESOLVE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, RESOLVE).unwrap();
        for wrong in [
            fixtures::THEIR_UNIT,
            fixtures::SPRITE,
            fixtures::HAND_UNIT,
            fixtures::GROUNDS,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(RESOLVE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · floating turn effects: a granted trigger for the turn (\"When it wins a combat this turn, gain 2 XP\") has no home, triggers::sources lists in-play scripts only and a resolved spell in the trash is not one; gains_xp_when_it_wins_a_combat_this_turn only narrates until a turn-scoped granted ability lands (the Relentless Pursuit seam)"]
    fn when_the_unit_wins_a_combat_this_turn_its_controller_gains_two_xp() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_on_vi(&mut ctx);
        assert_eq!(ctx.xp(0), 0);
        let won = Event::CombatWon {
            zone: fixtures::BF1,
            seat: 0,
        };
        assert!(wins_a_combat(&ctx, &won, fixtures::VI));
        ctx.raise(won);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), 2, "{:?}", ctx.blob.log);
        let turn = ctx.turn();
        ctx.expire(Expiry::EndOfTurn(turn));
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat: 0,
        });
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), 2, "a win after the turn ends gains nothing");
    }
}
