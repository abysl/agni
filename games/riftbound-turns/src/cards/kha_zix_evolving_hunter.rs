use super::prelude::{
    a_card, card_target, deal, done, on_attack, optional, spending_xp, unit, ENEMY_UNIT_HERE,
};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const XP: u8 = 3;
pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to deal my Might to");

fn leap(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !ctx.on_board(me) {
        return done();
    }
    let might = u8::try_from(ctx.current_might(me).max(0)).unwrap_or(u8::MAX);
    if deal(ctx, item, unit, might) {
        ctx.narrate(format!("{{card {me}}} deals {might} to {{card {unit}}}"));
    }
    done()
}

pub static CARD: Card = unit(
    "Kha'Zix - Evolving Hunter",
    &[Keyword::Hunt(1)],
    &[optional(spending_xp(on_attack(&[TARGET], leap), XP))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::might_this_turn;
    use crate::cards::{script_of, Trigger, Who, IMPLICIT_HUNT};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{chain, prompts, settle, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};

    const KHA: u32 = 90;
    const BRUTE: u32 = 91;

    fn hunting_grounds(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut kha = fixtures::unit(KHA, fixtures::BF1, 0, "Kha'Zix - Evolving Hunter", 5);
        kha.domain = vec!["Body".into()];
        kha.energy = Some(5);
        kha.power = Some(1);
        fixture.table.cards.push(kha);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 6));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        if xp > 0 {
            fixture.set_xp(0, xp);
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(KHA).unwrap(), &CARD));
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        ctx.raise(Event::Attacks { card: KHA });
        assert_eq!(triggers::collect(ctx), 1);
        chain::proceed(ctx);
    }

    #[test]
    fn the_script_prints_hunt_and_one_optional_attack_trigger_that_spends_three_xp() {
        assert!(std::ptr::eq(
            script_of("Kha'Zix - Evolving Hunter").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, &[Keyword::Hunt(1)]);
        assert_eq!(CARD.hunt(), 1);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.optional);
        assert_eq!(ability.xp, XP);
        assert!(ability.cost.is_none());
        assert_eq!(ability.targets, &[TARGET]);
        assert_eq!(TARGET.filter, ENEMY_UNIT_HERE);
        assert_eq!(XP, 3);
    }

    #[test]
    fn attacking_picks_an_enemy_here_then_asks_for_the_xp_and_the_leap_deals_his_might_as_it_resolves(
    ) {
        let mut fixture = hunting_grounds(4);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {BRUTE}}}")
            ],
            "both enemies here are offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "spend 3 XP for the {card 90} trigger?"
        );
        assert_eq!(ctx.xp(0), 4, "nothing is spent before the answer");
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.xp(0), 1, "three XP pay the trigger");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == KHA
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        assert_eq!(ctx.damage_on(BRUTE), 0, "nothing until it resolves");
        let pump = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &pump, KHA, 1, None);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: 6,
            source: Cause::Item(1)
        }));
        assert!(
            !ctx.on_board(BRUTE),
            "6 with the pump, not the printed 5: lethal on 6 Might"
        );
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {KHA}}} deals 6 to {{card {BRUTE}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_xp_or_holding_fewer_than_three_removes_the_trigger() {
        let mut fixture = hunting_grounds(3);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 3, "nothing is spent");
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost is declined".to_string()));
        drop(ctx);

        let mut poor = hunting_grounds(2);
        let mut ctx = poor.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "two XP cannot pay three");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost can't be paid".to_string()));
        ctx.raise(Event::Defends { card: KHA });
        assert_eq!(triggers::collect(&mut ctx), 0, "defending is not attacking");
    }

    #[test]
    fn hunt_gains_one_xp_when_he_conquers_or_holds() {
        let mut fixture = hunting_grounds(0);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(KHA), 1);
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![KHA],
        });
        assert_eq!(triggers::collect(&mut ctx), 1);
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index } if source == KHA && index == IMPLICIT_HUNT
        ));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.xp(0), 0, "383.4.c.2.a · the hunt waits on the chain");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), 1);
        ctx.raise(Event::Held {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![KHA],
        });
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.xp(0), 2);
        assert!(ctx.fault.is_none());
    }
}
