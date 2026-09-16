use super::prelude::{done, grant_this_turn, on_attack, optional, unit, with_cost};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 2;
pub const FURY: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};

fn reap(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    if grant_this_turn(ctx, me, Keyword::Assault(ASSAULT)) {
        ctx.narrate(format!("{{card {me}}} gains Assault {ASSAULT} this turn"));
    }
    done()
}

pub static CARD: Card = unit(
    "Baccai Reaper",
    &[],
    &[optional(with_cost(on_attack(&[], reap), FURY))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const REAPER: u32 = 90;
    const MIGHT: u8 = 4;

    fn reaper() -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(REAPER, fixtures::BF1, 0, "Baccai Reaper", MIGHT)
        }
    }

    fn dunes() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(reaper());
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(REAPER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn fury_runes_recycled(ctx: &Ctx) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Move {
                        card,
                        zone: fixtures::RUNE_DECK,
                        seat: 0,
                        ..
                    } if [fixtures::RUNE_A, 41, 43].contains(card)
                )
            })
            .count()
    }

    fn attacks(ctx: &mut Ctx) {
        assert!(ctx.mark_attacker(REAPER));
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_unit_with_one_optional_fury_costed_attack_trigger() {
        assert!(std::ptr::eq(script_of("Baccai Reaper").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.optional, "you may");
        assert_eq!(ability.cost, Some(FURY));
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_none());
        assert_eq!(ASSAULT, 2);
    }

    #[test]
    fn paying_the_fury_grants_assault_two_for_the_turn_which_counts_while_he_attacks() {
        let mut fixture = dunes();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("pay 1 Fury power for the {{card {REAPER}}} trigger?")
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(fury_runes_recycled(&ctx), 1, "{:?}", ctx.effects);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == REAPER
        ));
        assert!(!ctx.has_keyword(REAPER, Keyword::Assault(0)));
        assert_eq!(
            ctx.current_might(REAPER),
            i32::from(MIGHT),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.has_keyword(REAPER, Keyword::Assault(0)));
        assert_eq!(
            ctx.current_might(REAPER),
            i32::from(MIGHT + ASSAULT),
            "807.1.c · +2 while he is an attacker"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {REAPER}}} gains Assault {ASSAULT} this turn"
        )));
        ctx.clear_designation(REAPER);
        assert_eq!(
            ctx.current_might(REAPER),
            i32::from(MIGHT),
            "Assault reads nothing off a unit that is not attacking"
        );
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert!(
            !ctx.has_keyword(REAPER, Keyword::Assault(0)),
            "the grant expires with the turn"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn declining_or_lacking_a_fury_rune_removes_the_trigger_and_grants_nothing() {
        let mut fixture = dunes();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {REAPER}}} trigger is removed · its cost is declined"
        )));
        assert_eq!(fury_runes_recycled(&ctx), 0);
        assert!(!ctx.has_keyword(REAPER, Keyword::Assault(0)));
        assert_eq!(ctx.current_might(REAPER), i32::from(MIGHT));
        drop(ctx);

        let mut spent = dunes();
        spent
            .table
            .cards
            .retain(|card| ![fixtures::RUNE_A, 41, 43].contains(&card.id));
        spent.resolve();
        let mut ctx = spent.ctx();
        attacks(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "no confirm for an unpayable cost"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {REAPER}}} trigger is removed · its cost can't be paid"
        )));
        assert!(!ctx.has_keyword(REAPER, Keyword::Assault(0)));
        drop(ctx);

        let mut gone = dunes();
        let mut ctx = gone.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        ctx.table.card_mut(REAPER).unwrap().zone = Some(fixtures::TRASH);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.blob
                .log
                .iter()
                .any(|line| line.contains("gains Assault")),
            "a Reaper that left the board gains nothing"
        );
    }
}
