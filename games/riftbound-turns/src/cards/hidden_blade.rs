use super::prelude::{a_unit_at_a_battlefield, card_target, done, draw, kill, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::{Ctx, Killed};

pub const CARDS: usize = 2;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let controller = ctx.controller(unit);
    if kill(ctx, item, unit) == Killed::Yes {
        ctx.narrate(format!("{{card {unit}}} dies"));
    }
    draw(ctx, controller, CARDS);
    done()
}

pub static CARD: Card = spell(
    "Hidden Blade",
    &[Keyword::Hidden, Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        resolve,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const BLADE: u32 = 90;
    const THEIR_BLADE: u32 = 91;
    const ORDER_RUNE: u32 = 100;

    fn blade(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Hidden Blade", 2, 1);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(blade(BLADE, 0));
        fixture.table.cards.push(blade(THEIR_BLADE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
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
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_hidden_action_over_one_unit_at_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Hidden Blade").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Hidden Blade");
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, UNIT_AT_BATTLEFIELD);
    }

    #[test]
    fn the_blade_kills_an_enemy_unit_at_a_battlefield_and_its_controller_draws_two() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let my_hand = ctx.hand_of(0).len();
        let their_hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "units at battlefields only, never the ones in a base"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(
            ctx.effects.contains(&Effect::exhaust(ORDER_RUNE)),
            "two energy and one Order power: {:?}",
            ctx.effects
        );
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(
            drew(&ctx, 1),
            2,
            "355.10.d · the controller is not a target, it just draws"
        );
        assert_eq!(drew(&ctx, 0), 0);
        assert_eq!(ctx.hand_of(1).len(), their_hand + 2);
        assert_eq!(ctx.hand_of(0).len(), my_hand - 1);
        assert!(ctx.blob.log.contains(&"{card 81} dies".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(BLADE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn killing_your_own_unit_draws_you_the_two_cards() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let my_hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(drew(&ctx, 0), 2);
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.hand_of(0).len(), my_hand - 1 + 2);
    }

    #[test]
    fn played_from_facedown_it_is_free_and_only_reaches_the_hiding_battlefield() {
        let mut fixture = armed();
        fixture.table.card_mut(BLADE).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(BLADE).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let their_hand = ctx.hand_of(1).len();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BLADE, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            BLADE,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "cancel"],
            "737.1.d · the Sprite at the other battlefield is out of reach"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move { zone, .. } if Some(*zone) == ctx.zones.rune_deck
            )),
            "no power is paid from facedown"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.hand_of(1).len(), their_hand + 2);
        assert_eq!(ctx.card(BLADE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_target_that_left_the_battlefield_is_left_alone_and_nobody_draws() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let their_hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let base = ctx.zones.base.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::THEIR_UNIT, base, 1), 1)
            .unwrap();
        both_pass(&mut ctx);
        assert_eq!(ctx.card(fixtures::THEIR_UNIT).unwrap().zone, Some(base));
        assert_eq!(
            ctx.hand_of(1).len(),
            their_hand,
            "356.3.e · the draw rides on the kill's target and fizzles with it"
        );
        assert_eq!(drew(&ctx, 1), 0);
        assert_eq!(ctx.card(BLADE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn units_in_a_base_are_refused_and_the_action_waits_for_your_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_BLADE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, BLADE).unwrap();
        for wrong in [fixtures::VI, fixtures::GROUNDS, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit at a battlefield"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(BLADE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
