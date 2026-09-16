use super::prelude::{
    a_card, card_target, deal, done, equip, gear, on_attack, on_defend, while_attached,
    with_statics, ENEMY_UNIT_HERE,
};
use super::{
    Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, TargetSpec, GRANTED,
};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};

pub const MIGHT_BONUS: i16 = 0;
pub const DAMAGE: u8 = 2;
pub const ON_ATTACK: u8 = GRANTED;
pub const ON_DEFEND: u8 = GRANTED + 1;
pub const TARGET: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to deal 2 to");

fn shoot(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static WEARER_TEXT: [Ability; 2] = [on_attack(&[TARGET], shoot), on_defend(&[TARGET], shoot)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear("Recurve Bow", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, queue_granted, GEAR};
    use crate::cards::prelude::attach_gear;
    use crate::cards::{script_of, Static, Trigger, Who};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play, triggers};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;

    const BRUTE: u32 = 91;
    const AWAY: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Recurve Bow", 2, "Fury"));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(AWAY, fixtures::BASE, 1, "Jinx", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_fury_equipment_with_no_might_bonus_and_attack_and_defend_listeners() {
        assert!(std::ptr::eq(script_of("Recurve Bow").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.abilities.len(), 1);
        let attack = &WEARER_TEXT[0];
        let defend = &WEARER_TEXT[1];
        assert_eq!(attack.trigger, Trigger::Attacks(Who::Me));
        assert_eq!(defend.trigger, Trigger::Defends(Who::Me));
        assert_eq!(attack.targets, &[TARGET]);
        assert_eq!(defend.targets, &[TARGET]);
        assert_eq!((TARGET.min, TARGET.max), (1, 1));
        assert!(attack.condition.is_none() && defend.condition.is_none());
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(0), Grant::Ability(_)]
        ));
    }

    #[test]
    fn the_wearers_attack_asks_for_an_enemy_here_and_deals_two_to_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "136.3.b · a +0 bonus changes nothing"
        );
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_ATTACK,
            TargetRef::Card(fixtures::VI),
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {BRUTE}}}")
            ],
            "the enemies where the wearer stands; the one in a base is not here"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[AWAY]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the wearer is friendly"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(BRUTE), i32::from(DAMAGE));
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_wearers_defense_shoots_too_and_a_lone_enemy_dies_to_the_arrow() {
        let mut fixture = armed();
        fixture.table.card_mut(BRUTE).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_DEFEND,
            TargetRef::Card(fixtures::VI),
        );
        if ctx.blob.prompt.is_some() {
            fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        }
        fixtures::pass_until_open(&mut ctx);
        assert!(
            !ctx.on_board(fixtures::THEIR_UNIT),
            "two damage kills the 2-Might unit"
        );
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_enemy_here_the_trigger_fizzles_and_the_engine_hears_the_attached_bow() {
        let mut fixture = armed();
        for enemy in [fixtures::THEIR_UNIT, BRUTE] {
            fixture.table.card_mut(enemy).unwrap().zone = Some(fixtures::BASE);
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_ATTACK,
            TargetRef::Card(fixtures::VI),
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} trigger fizzles · no legal target".to_string()));
        assert!(triggers::find(
            &ctx,
            &Event::Attacks {
                card: fixtures::THEIR_UNIT
            }
        )
        .is_empty());
        ctx.raise(Event::Attacks { card: fixtures::VI });
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "136.2.c · the effect text is the wearer's own"
        );
    }

    #[test]
    fn an_attack_by_the_wearer_shoots_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(Event::Attacks { card: fixtures::VI });
        assert_eq!(triggers::collect(&mut ctx), 1);
        ctx.raise(Event::Defends { card: fixtures::VI });
        assert_eq!(triggers::collect(&mut ctx), 1);
        ctx.raise(Event::Attacks {
            card: fixtures::THEIR_UNIT,
        });
        assert_eq!(triggers::collect(&mut ctx), 0, "not the wearer");
    }
}
