use super::prelude::{activated, done, exhausting_self, gear, named};
use super::{Card, Cost, Flow, Item, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const BONUS: u8 = 1;

pub fn next_spell_deals_bonus_damage(ctx: &mut Ctx, seat: u8, bonus: u8) -> bool {
    ctx.promise_spell_bonus(seat, bonus);
    ctx.narrate(format!(
        "the next spell {{seat {seat}}} plays this turn deals {bonus} Bonus Damage"
    ));
    true
}

fn study(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    next_spell_deals_bonus_damage(ctx, item.controller, BONUS);
    done()
}

pub static CARD: Card = gear(
    "Ravenborn Tome",
    &[],
    &[named(
        exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], study)),
        "the next spell you play this turn deals 1 Bonus Damage",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, deal, play, spell};
    use crate::cards::{SelfCost, Trigger};
    use crate::engine::ctx::COUNTER_DAMAGE;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::Refusal;
    use agni_plugin_sdk::table::Target;

    const TOME: u32 = 90;
    const SECOND_SPELL: u32 = 91;

    fn zap(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
        if let Some(unit) = crate::cards::prelude::card_target(ctx, item, 0) {
            deal(ctx, item, unit, 1);
        }
        done()
    }

    static ZAP: Card = spell("Zap", &[], &[play(&[a_unit("a unit")], zap)]);

    fn library() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut tome = fixtures::gear(TOME, fixtures::BASE, 0, "Ravenborn Tome", 3);
        tome.domain = vec!["Fury".into()];
        fixture.table.cards.push(tome);
        fixture.table.cards.push(fixtures::spell(
            SECOND_SPELL,
            fixtures::HAND,
            0,
            "Second Spark",
            2,
            1,
        ));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(5);
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &ZAP)
            .with_script(SECOND_SPELL, &ZAP);
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_one_untargeted_exhaust_activation() {
        let fixture = library();
        assert!(std::ptr::eq(fixture.scripts.of_card(TOME).unwrap(), &CARD));
        assert_eq!(CARD.name, "Ravenborn Tome");
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn the_tome_exhausts_onto_the_chain_and_records_the_promise_when_it_resolves() {
        let mut fixture = library();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, TOME, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, TOME, 0).unwrap();
        assert!(ctx.card(TOME).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("Bonus Damage")));
        assert_eq!(ctx.blob.seat(0).next_spell_bonus, 0);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"the next spell {seat 0} plays this turn deals 1 Bonus Damage".to_string()));
        assert_eq!(ctx.blob.seat(0).next_spell_bonus, BONUS);
        assert_eq!(ctx.blob.seat(1).next_spell_bonus, 0);
        assert_eq!(
            activate::activate(&mut ctx, 0, TOME, 0),
            Err(Refusal::Exhausted)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_next_spell_this_turn_deals_one_more_and_the_one_after_does_not() {
        let mut fixture = library();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, TOME, 0).unwrap();
        resolve_chain(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.table
                .counter(Target::Card(fixtures::THEIR_UNIT), COUNTER_DAMAGE)
                .unwrap_or(0),
            2,
            "1 printed plus 1 Bonus Damage"
        );
        assert_eq!(ctx.blob.seat(0).next_spell_bonus, 0, "the promise is spent");
        fixtures::play_from_hand(&mut ctx, 0, SECOND_SPELL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.table
                .counter(Target::Card(fixtures::VI), COUNTER_DAMAGE)
                .unwrap_or(0),
            1,
            "the following spell has no bonus"
        );
        assert!(ctx.fault.is_none());
    }
}
