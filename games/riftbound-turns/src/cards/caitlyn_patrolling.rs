use super::prelude::{
    a_unit_at_a_battlefield, activated, at_battlefield, card_target, deal, done, exhausting_self,
    named, unit, usable_if,
};
use super::{Card, Cost, Flow, Item, Keyword, Source, Stage, Timing};
use crate::engine::ctx::Ctx;

pub fn patrolling(ctx: &Ctx, source: Source) -> bool {
    at_battlefield(ctx, source.card)
}

pub fn shot_of(ctx: &Ctx, me: u32) -> u8 {
    u8::try_from(ctx.current_might(me).max(0)).unwrap_or(u8::MAX)
}

fn snipe(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let amount = shot_of(ctx, me);
    deal(ctx, item, unit, amount);
    done()
}

pub static CARD: Card = unit(
    "Caitlyn - Patrolling",
    &[Keyword::Backline],
    &[named(
        usable_if(
            exhausting_self(activated(
                Timing::Sorcery,
                Cost::FREE,
                &[a_unit_at_a_battlefield("a unit at a battlefield to shoot")],
                snipe,
            )),
            patrolling,
        ),
        "deal my Might to a unit at a battlefield",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{might_this_turn, UNIT_AT_BATTLEFIELD};
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, combat, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const CAITLYN: u32 = 90;
    const RAIDER: u32 = 91;
    const ALLY: u32 = 92;

    fn caitlyn(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(CAITLYN, zone, 0, "Caitlyn - Patrolling", 3)
        }
    }

    fn her_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == CAITLYN)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    fn patrol(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(caitlyn(zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 7));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn caitlyn_is_backline_with_one_exhaust_ability_usable_only_at_a_battlefield() {
        assert!(std::ptr::eq(
            script_of("Caitlyn - Patrolling").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Backline]);
        assert_eq!(CARD.abilities.len(), 1);
        let shot = &CARD.abilities[0];
        assert_eq!(shot.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(shot.self_cost, SelfCost::Exhaust);
        assert_eq!(shot.cost, Some(Cost::FREE));
        assert_eq!(shot.targets.len(), 1);
        assert_eq!(shot.targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert!(shot.usable.is_some());
        assert_eq!(shot.label, Some("deal my Might to a unit at a battlefield"));
        let mut fixture = patrol(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(patrolling(
            &ctx,
            Source {
                card: CAITLYN,
                ability: 0
            }
        ));
        assert_eq!(shot_of(&ctx, CAITLYN), 3);
    }

    #[test]
    fn she_is_assigned_combat_damage_last() {
        let mut fixture = patrol(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 2));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(CAITLYN, Keyword::Backline));
        assert!(!ctx.has_keyword(CAITLYN, Keyword::Tank));
        assert_eq!(
            combat::ordered(&ctx, &[CAITLYN, ALLY], false),
            [ALLY],
            "815 · the plain unit takes damage before her"
        );
        assert_eq!(
            combat::ordered(&ctx, &[CAITLYN], false),
            [CAITLYN],
            "alone she is the only assignment"
        );
    }

    #[test]
    fn at_a_battlefield_the_shot_exhausts_her_and_deals_her_current_might_when_it_resolves() {
        let mut fixture = patrol(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            her_offers(&ctx),
            [(
                format!("{{card {CAITLYN}}}: deal my Might to a unit at a battlefield (exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, CAITLYN, 0).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {CAITLYN}}}"),
                format!("{{card {RAIDER}}}"),
                "cancel".to_string()
            ],
            "units at battlefields, herself included; Vi at the base is not offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {RAIDER}}}")).unwrap();
        assert!(
            ctx.card(CAITLYN).unwrap().exhausted,
            "exhausting her is the cost"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.damage_on(RAIDER), 0, "nothing until it resolves");
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, CAITLYN, 2, None);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.damage_on(RAIDER),
            5,
            "her Might is read as the ability resolves, the +2 included"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, n: 5, .. } if *card == RAIDER
        )));
        assert!(ctx.on_board(RAIDER), "5 damage on 7 Might is not lethal");
        assert_eq!(
            activate::activate(&mut ctx, 0, CAITLYN, 0),
            Err(Refusal::Exhausted),
            "one shot per ready"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn at_the_base_the_ability_is_neither_offered_nor_usable() {
        let mut fixture = patrol(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(
            her_offers(&ctx).is_empty(),
            "use this ability only while I'm at a battlefield"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, CAITLYN, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, CAITLYN, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(!ctx.card(CAITLYN).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
    }
}
