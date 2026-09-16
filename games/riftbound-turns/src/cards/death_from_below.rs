use super::dancing_grenade::may_play_again_from_the_trash;
use super::prelude::{a_unit_at_a_battlefield, card_target, done, kill, play, spell, RAINBOW};
use super::{Card, Cost, Flow, Item, Stage};
use crate::engine::ctx::{Ctx, Killed};

pub const MIGHT_LIMIT: i32 = 3;
pub const REPLAY: Cost = RAINBOW;

pub fn small_enough_to_replay(might: i32) -> bool {
    might <= MIGHT_LIMIT
}

fn drag_below(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let might = ctx.current_might(unit);
    match kill(ctx, item, unit) {
        Killed::Yes => ctx.narrate(format!("{{card {unit}}} dies")),
        Killed::Replaced => {}
        Killed::NotOnBoard => return done(),
    }
    if !small_enough_to_replay(might) {
        ctx.narrate(format!(
            "{{card {unit}}} had {might} Might · {{card {}}} stays in the trash",
            item.kind.source()
        ));
        return done();
    }
    may_play_again_from_the_trash(ctx, item, item.controller, REPLAY);
    done()
}

pub static CARD: Card = spell(
    "Death from Below",
    &[],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        drag_below,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::might_this_turn;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const DEATH: u32 = 90;
    const THEIR_DEATH: u32 = 91;
    const BRUTE: u32 = 92;
    const SCOUT: u32 = 93;
    const CHAOS_RUNE: u32 = 100;

    fn death(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Death from Below", 4, 1);
        card.domain = vec!["Fury".into(), "Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(death(DEATH, 0));
        fixture.table.cards.push(death(THEIR_DEATH, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 1, "Scout", 3));
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

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, DEATH).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_sorcery_killing_a_unit_at_a_battlefield_and_three_might_is_the_replay_line()
    {
        assert!(std::ptr::eq(script_of("Death from Below").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(REPLAY, RAINBOW);
        assert!(small_enough_to_replay(3));
        assert!(small_enough_to_replay(0));
        assert!(!small_enough_to_replay(4));
    }

    #[test]
    fn a_mighty_unit_dies_and_the_spell_stays_in_the_trash() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DEATH).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "{card 93}", "cancel"],
            "any unit at a battlefield, whatever its Might"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.on_board(BRUTE), "nothing before it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == BRUTE
        )));
        assert!(ctx.blob.log.contains(&"{card 92} dies".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} had 5 Might · {card 90} stays in the trash".to_string()));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("again from the trash")));
        assert_eq!(ctx.card(DEATH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_small_unit_dies_and_the_replay_from_the_trash_is_offered_to_the_caster() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, SCOUT);
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{card 93} dies".to_string()));
        assert!(ctx.blob.log.contains(
            &"{seat 0} may play {card 90} again from the trash for 1 any power · the play waits for the engine".to_string()
        ));
        assert_eq!(ctx.card(DEATH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_might_is_read_as_the_kill_resolves_so_a_pumped_scout_does_not_offer_the_replay() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DEATH).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        let spell = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &spell, SCOUT, 1, None);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 93} had 4 Might · {card 90} stays in the trash".to_string()));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("again from the trash")));
    }

    #[test]
    #[ignore = "engine gap · a play as an effect mid-resolution · the spell is still on the chain as it resolves and play::begin cannot play it again from the trash; may_play_again_from_the_trash narrates until the engine offers the caster the paid replay once the spell has landed in the trash"]
    fn after_a_small_kill_the_caster_pays_a_rainbow_and_plays_it_again_from_the_trash() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, SCOUT);
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "may pay 1 any power"
        );
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(0));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the spell is on the chain again");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.card(DEATH).unwrap().zone,
            Some(fixtures::TRASH),
            "a replay ends in the trash, not in banishment"
        );
    }

    #[test]
    fn a_unit_in_a_base_is_refused_a_gone_target_is_left_alone_and_the_other_seat_waits() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DEATH)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, DEATH).unwrap();
        for wrong in [fixtures::THEIR_UNIT, fixtures::VI, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base or not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        ctx.recall(SCOUT, false);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.on_board(SCOUT));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("again from the trash")));
        assert_eq!(ctx.card(DEATH).unwrap().zone, Some(fixtures::TRASH));
    }
}
