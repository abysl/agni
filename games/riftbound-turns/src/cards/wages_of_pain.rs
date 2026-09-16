use super::prelude::{a_unit_at_a_battlefield, card_target, deal, done, play, spawn_gold, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 3;
pub const GOLD_ARRIVES_READY: bool = false;

fn wages(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    spawn_gold(ctx, seat, GOLD_ARRIVES_READY);
    done()
}

pub static CARD: Card = spell(
    "Wages of Pain",
    &[Keyword::Hidden, Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        wages,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, Trigger, TOKEN_GOLD};
    use crate::engine::ctx::{Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const WAGES: u32 = 90;

    fn ledger() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::spell(WAGES, fixtures::HAND, 0, "Wages of Pain", 3, 0)
        });
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(5);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(WAGES).unwrap(), &CARD));
        fixture
    }

    fn damage(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn gold_of<'c>(ctx: &'c Ctx, seat: u8) -> Vec<&'c CardInfo> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .collect()
    }

    #[test]
    fn the_script_is_a_hidden_action_over_a_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Wages of Pain").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(DAMAGE, 3);
    }

    #[test]
    fn three_damage_lands_and_an_exhausted_gold_is_played_to_the_casters_base() {
        let mut fixture = ledger();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WAGES).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "units at battlefields · Vi in base is not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(gold_of(&ctx, 0).is_empty(), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(damage(&ctx, fixtures::THEIR_UNIT), 3);
        assert!(ctx.on_board(fixtures::THEIR_UNIT), "5 Might survives 3");
        assert!(ctx.blob.log.contains(&"{card 81} takes 3".to_string()));
        let gold = gold_of(&ctx, 0);
        assert_eq!(gold.len(), 1);
        assert!(gold[0].exhausted, "played exhausted");
        assert_eq!(gold[0].zone, Some(fixtures::BASE));
        assert!(gold_of(&ctx, 1).is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert_eq!(ctx.card(WAGES).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn three_is_lethal_for_a_small_unit_and_the_gold_still_comes() {
        let mut fixture = ledger();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WAGES).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            !ctx.on_board(fixtures::SPRITE),
            "3 damage kills the 3 Might Sprite"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, .. } if *card == fixtures::SPRITE
        )));
        assert_eq!(gold_of(&ctx, 0).len(), 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_target_that_left_takes_nothing_but_the_gold_is_still_played() {
        let mut fixture = ledger();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WAGES).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.bounce(fixtures::THEIR_UNIT);
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert_eq!(
            gold_of(&ctx, 0).len(),
            1,
            "356.3.e.5 · the Gold is not a target"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_in_a_base_is_refused() {
        let mut fixture = ledger();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WAGES).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(WAGES).unwrap().zone, Some(fixtures::HAND));
        assert!(gold_of(&ctx, 0).is_empty());
    }
}
