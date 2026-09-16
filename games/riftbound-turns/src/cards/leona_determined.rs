use super::prelude::{a_card, card_target, done, on_attack, stun, unit, ENEMY_UNIT_HERE};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to stun");

fn zenith_blade(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        stun(ctx, unit);
    }
    done()
}

pub static CARD: Card = unit(
    "Leona - Determined",
    &[Keyword::Shield(1)],
    &[on_attack(&[TARGET], zenith_blade)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, cleanup, priority, showdown, triggers};
    use crate::state::{ItemKind, PromptWhy, TargetRef};

    const LEONA: u32 = 90;
    const AWAY: u32 = 91;

    fn dawn() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut leona = fixtures::unit(LEONA, fixtures::BF1, 0, "Leona - Determined", 4);
        leona.domain = vec!["Order".into()];
        leona.energy = Some(4);
        leona.power = Some(1);
        fixture.table.cards.push(leona);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(5);
        fixture
            .table
            .cards
            .push(fixtures::unit(AWAY, fixtures::BASE, 1, "Away", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(LEONA).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_prints_shield_and_one_attack_trigger_aimed_at_an_enemy_here() {
        assert!(std::ptr::eq(
            script_of("Leona - Determined").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, &[Keyword::Shield(1)]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert_eq!(ability.targets, &[TARGET]);
        assert!(!ability.optional);
    }

    #[test]
    fn attacking_stuns_the_chosen_enemy_here_and_it_deals_no_combat_damage() {
        let mut fixture = dawn();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: LEONA });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::THEIR_UNIT)],
            "the enemy in its base is not here"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == LEONA
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(!ctx.is_stunned(fixtures::THEIR_UNIT));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert!(!ctx.deals_combat_damage(fixtures::THEIR_UNIT));
        assert_eq!(ctx.combat_might(fixtures::THEIR_UNIT), 0);
        assert!(!ctx.is_stunned(AWAY));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is stunned", fixtures::THEIR_UNIT)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn in_a_real_combat_the_stunned_defender_hits_back_for_nothing_and_leona_survives() {
        let mut fixture = dawn();
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        assert!(ctx.is_attacker(LEONA));
        assert_eq!(
            ctx.current_might(LEONA),
            4,
            "Shield reads only while she defends"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)],
            "the lone defender is the only legal target"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        for _ in 0..4 {
            let Some(held) = ctx.blob.showdown.clone() else {
                break;
            };
            if ctx.blob.prompt.is_some() {
                break;
            }
            if !ctx.blob.chain.is_empty() {
                let Some(holder) = priority::holder(&ctx) else {
                    break;
                };
                priority::pass(&mut ctx, holder).unwrap();
                continue;
            }
            showdown::pass(&mut ctx, held.focus()).unwrap();
        }
        assert!(ctx.blob.showdown.is_none(), "the combat closed");
        assert!(ctx
            .blob
            .log
            .contains(&"attackers 4 might vs defenders 0 might".to_string()));
        assert!(
            ctx.on_board(LEONA),
            "a 5-Might defender that deals no combat damage leaves her at 4"
        );
        assert_eq!(ctx.damage_on(LEONA), 0);
        assert!(ctx.blob.log.contains(&"{card 81} takes 4".to_string()));
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "4 on a 5-Might defender is not lethal, so it keeps the battlefield"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(1));
        assert_eq!(ctx.location(LEONA), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_enemy_here_the_trigger_fizzles_and_defending_never_fires_it() {
        let mut fixture = dawn();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: LEONA });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
        ctx.raise(Event::Defends { card: LEONA });
        assert_eq!(triggers::collect(&mut ctx), 0);
        assert!(!ctx.is_stunned(fixtures::THEIR_UNIT));
    }
}
