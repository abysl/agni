use super::prelude::{card_target, done, draw, kill, play, spell, target, GEAR};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::{Ctx, Killed};

pub const CARDS: usize = 2;

const VICTIM: TargetSpec = target(GEAR, 1, 1, TargetKind::Card, "a gear to kill");

fn detonate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(gear) = card_target(ctx, item, 0) else {
        return done();
    };
    let controller = ctx.controller(gear);
    if kill(ctx, item, gear) == Killed::Yes {
        ctx.narrate(format!("{{card {gear}}} dies"));
    }
    let drawn = draw(ctx, controller, CARDS);
    ctx.narrate(format!("{{seat {controller}}} draws {drawn}"));
    done()
}

pub static CARD: Card = spell("Detonate", &[], &[play(&[VICTIM], detonate)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const DETONATE: u32 = 90;
    const THEIR_DETONATE: u32 = 91;
    const MY_GEAR: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const THEIR_GOLD: u32 = 94;

    fn detonate(id: u32, seat: u8) -> CardInfo {
        fixtures::spell(id, fixtures::HAND, seat, "Detonate", 1, 1)
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(detonate(DETONATE, 0));
        fixture.table.cards.push(detonate(THEIR_DETONATE, 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gold(THEIR_GOLD, 1, false));
        fixture.table.tokens.push(THEIR_GOLD);
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

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_is_a_sorcery_over_one_gear() {
        assert!(std::ptr::eq(script_of("Detonate").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, GEAR);
    }

    #[test]
    fn an_enemy_gear_dies_and_its_controller_draws_two() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let my_hand = ctx.hand_of(0).len();
        let their_hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, DETONATE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 93}", "{card 94}", "cancel"]
        );
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        assert!(
            ctx.on_board(THEIR_GEAR),
            "nothing happens before it resolves"
        );
        both_pass(&mut ctx);
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().seat, 1);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: false, .. } if *card == THEIR_GEAR
        )));
        assert_eq!(
            ctx.hand_of(1).len(),
            their_hand + 2,
            "the gear's controller draws, not the caster"
        );
        assert_eq!(
            ctx.hand_of(0).len(),
            my_hand - 1,
            "only Detonate left my hand"
        );
        assert!(ctx.blob.log.contains(&"{card 93} dies".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 1} draws 2".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(DETONATE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn my_own_gear_draws_me_two_and_a_gold_token_vanishes() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let my_hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, DETONATE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(ctx.card(MY_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.hand_of(0).len(), my_hand - 1 + 2);
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let their_hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, DETONATE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 94}").unwrap();
        both_pass(&mut ctx);
        assert!(ctx.effects.contains(&Effect::Despawn { card: THEIR_GOLD }));
        assert!(ctx.card(THEIR_GOLD).is_none());
        assert_eq!(ctx.hand_of(1).len(), their_hand + 2);
    }

    #[test]
    fn a_gear_that_left_the_board_first_draws_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DETONATE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(THEIR_GEAR, fixtures::HAND, 1), 1)
            .unwrap();
        let their_hand = ctx.hand_of(1).len();
        both_pass(&mut ctx);
        assert_eq!(
            ctx.hand_of(1).len(),
            their_hand,
            "356.3.e · the whole effect is skipped"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert_eq!(ctx.card(DETONATE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn non_gear_is_refused_and_the_sorcery_is_the_turn_players() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DETONATE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, DETONATE).unwrap();
        for wrong in [fixtures::VI, fixtures::HAND_GEAR, fixtures::THEIR_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a gear on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(DETONATE).unwrap().zone, Some(fixtures::HAND));
    }
}
