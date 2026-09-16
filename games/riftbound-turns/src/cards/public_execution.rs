use super::prelude::{a_card, a_friendly_unit, card_target, done, kill, play, spell};
use super::{Card, Cost, Filter, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::{Ctx, Killed};
use crate::state::TargetRef;

pub const FLOW: Cost = Cost {
    energy: 5,
    power: &[Power::Rainbow, Power::Rainbow],
};

const CHOSEN: usize = 0;
const CONDEMNED: usize = 1;

pub const ENEMY_UNIT_WITH_LESS_MIGHT: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Enemy,
    Filter::MightLessThan(CHOSEN as u8),
]);

pub fn has_less_might_than(ctx: &Ctx, enemy: u32, chosen: u32) -> bool {
    ctx.current_might(enemy) < ctx.current_might(chosen)
}

fn execute(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(chosen) = card_target(ctx, item, CHOSEN) else {
        return done();
    };
    let Some(TargetRef::Card(enemy)) = item.targets.get(CONDEMNED).copied() else {
        return done();
    };
    if card_target(ctx, item, CONDEMNED).is_none() {
        if ctx.on_board(enemy) && !has_less_might_than(ctx, enemy, chosen) {
            ctx.narrate(format!(
                "{{card {enemy}}} does not have less Might than {{card {chosen}}} · it is spared"
            ));
        }
        return done();
    }
    if kill(ctx, item, enemy) == Killed::Yes {
        ctx.narrate(format!("{{card {enemy}}} is executed"));
    }
    done()
}

pub static CARD: Card = spell(
    "Public Execution",
    &[Keyword::Flow(FLOW)],
    &[play(
        &[
            a_friendly_unit("a friendly unit"),
            a_card(
                ENEMY_UNIT_WITH_LESS_MIGHT,
                "an enemy unit with less Might than it",
            ),
        ],
        execute,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{might_this_turn, FRIENDLY_UNIT};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{ChainItem, ItemKind, Leave, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const EXECUTION: u32 = 90;
    const THEIR_EXECUTION: u32 = 91;
    const BRUTE: u32 = 92;
    const SPARE_RUNES: [u32; 3] = [100, 101, 102];

    fn execution(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Public Execution", 2, 1);
        card.domain = vec!["Body".into(), "Order".into()];
        card
    }

    fn square(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(execution(EXECUTION, zone, 0));
        fixture
            .table
            .cards
            .push(execution(THEIR_EXECUTION, fixtures::HAND, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(1));
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

    fn cast(ctx: &mut Ctx, chosen: u32, enemy: u32) {
        fixtures::play_from_hand(ctx, 0, EXECUTION).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {chosen}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(ctx, 0, &format!("{{card {enemy}}}")).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(chosen), TargetRef::Card(enemy)]
        );
    }

    #[test]
    fn the_script_is_a_flow_sorcery_over_a_friendly_unit_then_an_enemy_unit() {
        assert!(std::ptr::eq(script_of("Public Execution").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[CHOSEN].filter, FRIENDLY_UNIT);
        assert_eq!(
            ability.targets[CONDEMNED].filter,
            ENEMY_UNIT_WITH_LESS_MIGHT
        );
        let mut fixture = square(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(has_less_might_than(
            &ctx,
            fixtures::THEIR_UNIT,
            fixtures::VI
        ));
        assert!(!has_less_might_than(&ctx, fixtures::SPRITE, fixtures::VI));
        assert!(!has_less_might_than(&ctx, BRUTE, fixtures::VI));
    }

    #[test]
    fn an_enemy_with_less_might_than_the_chosen_friendly_unit_dies() {
        let mut fixture = square(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EXECUTION).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "my units first"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "cancel"],
            "only the enemy unit with less Might than Vi is offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "nothing before it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH),
            "2 is less than 3"
        );
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, unit: true, .. } if *card == fixtures::THEIR_UNIT)
        ));
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx.blob.log.contains(&"{card 81} is executed".to_string()));
        assert_eq!(ctx.card(EXECUTION).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_equal_or_bigger_enemy_is_spared_and_a_might_change_in_response_changes_the_verdict() {
        let mut fixture = square(fixtures::HAND);
        let mut ctx = fixture.ctx();
        cast(&mut ctx, fixtures::VI, fixtures::THEIR_UNIT);
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, fixtures::VI, -1, None);
        assert_eq!(ctx.current_might(fixtures::VI), 2);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(fixtures::THEIR_UNIT), "2 is not less than 2");
        assert!(ctx.blob.log.contains(
            &"{card 81} does not have less Might than {card 50} · it is spared".to_string()
        ));
        drop(ctx);

        let mut pumped = square(fixtures::HAND);
        let mut ctx = pumped.ctx();
        let pump = ChainItem::new(9, ItemKind::Spell { card: EXECUTION }, 0, Origin::Hand);
        might_this_turn(&mut ctx, &pump, fixtures::VI, 2, None);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        cast(&mut ctx, fixtures::VI, BRUTE);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(BRUTE).unwrap().zone,
            Some(fixtures::TRASH),
            "4 is less than the pumped 5"
        );
        drop(ctx);

        let mut shrunk = square(fixtures::HAND);
        let mut ctx = shrunk.ctx();
        cast(&mut ctx, fixtures::VI, fixtures::THEIR_UNIT);
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, fixtures::VI, -2, Some(0));
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "2 is not less than the shrunk 1"
        );
    }

    #[test]
    fn from_the_trash_the_flow_play_executes_and_is_banished() {
        let mut fixture = square(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            EXECUTION,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.banished_of(0), [EXECUTION]);
        assert!(!ctx.trash_of(0).contains(&EXECUTION));
    }

    #[test]
    fn the_chosen_unit_leaving_spares_the_enemy_and_the_wrong_side_is_refused_for_each_pick() {
        let mut fixture = square(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_EXECUTION)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, EXECUTION).unwrap();
        for wrong in [fixtures::THEIR_UNIT, BRUTE, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        for wrong in [fixtures::VI, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is friendly or not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::HAND, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "no chosen unit to measure against"
        );
        assert_eq!(ctx.card(EXECUTION).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn only_enemies_with_less_might_than_the_chosen_unit_are_offered() {
        let mut fixture = square(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EXECUTION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "cancel"],
            "Jinx at 2 only; the 3-Might Sprite and the 4-Might Brute are out"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[BRUTE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
    }
}
