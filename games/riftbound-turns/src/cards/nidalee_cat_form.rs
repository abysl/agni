use super::prelude::{done, draw, triggered, unit};
use super::{Card, Flow, Item, Keyword, Stage, Trigger, Who};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

fn pounce(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = unit(
    "Nidalee - Cat Form",
    &[Keyword::Ambush],
    &[triggered(Trigger::CombatWon(Who::Me), &[], pounce)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, settle, showdown};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const NIDALEE: u32 = 90;
    const DEFENDER: u32 = 91;

    fn nidalee(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Body".into()],
            ..fixtures::unit(NIDALEE, zone, seat, "Nidalee - Cat Form", 4)
        }
    }

    fn hunt(defender_might: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(nidalee(fixtures::BF1, 0));
        fixture.table.cards.push(fixtures::unit(
            DEFENDER,
            fixtures::BF1,
            1,
            "Defender",
            defender_might,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn fight(ctx: &mut Ctx) {
        cleanup::run(ctx, None);
        let open = ctx.blob.showdown.clone().expect("a combat opens");
        assert!(open.combat);
        showdown::pass(ctx, 0).unwrap();
        showdown::pass(ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_prints_ambush_and_one_combat_won_trigger_of_her_own() {
        assert!(std::ptr::eq(
            script_of("Nidalee - Cat Form").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Nidalee - Cat Form");
        assert_eq!(CARD.keywords, [Keyword::Ambush]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let won = &CARD.abilities[0];
        assert_eq!(won.trigger, Trigger::CombatWon(Who::Me));
        assert!(won.targets.is_empty());
        assert!(!won.optional);
        assert!(won.cost.is_none() && won.condition.is_none());
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn surviving_a_won_combat_draws_her_controller_one_when_the_trigger_resolves() {
        let mut fixture = hunt(2);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fight(&mut ctx);
        assert_eq!(
            ctx.card(DEFENDER).unwrap().zone,
            Some(fixtures::TRASH),
            "four on two is lethal"
        );
        assert_eq!(
            ctx.location(NIDALEE),
            Some(Location::Battlefield(fixtures::BF1)),
            "two on four is not"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::CombatWon { zone, seat: 0 } if *zone == fixtures::BF1
        )));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == NIDALEE
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.blob.prompt.is_none(), "she asks nothing");
        assert_eq!(ctx.hand_of(0).len(), hand, "the draw waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert!(ctx.events.contains(&Event::Drew { seat: 0, nth: 1 }));
        assert_eq!(ctx.blob.seat(1).draws, 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn dying_in_the_combat_is_not_winning_it() {
        let mut fixture = hunt(6);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fight(&mut ctx);
        assert_eq!(
            ctx.card(NIDALEE).unwrap().zone,
            Some(fixtures::TRASH),
            "six on four is lethal"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::CombatLost { zone, seat: 0 } if *zone == fixtures::BF1
        )));
        assert!(
            !ctx.blob
                .chain
                .iter()
                .any(|item| item.kind.source() == NIDALEE),
            "she did not remain after combat"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn a_win_elsewhere_or_by_the_other_seat_is_not_hers() {
        let mut fixture = hunt(2);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF2,
            seat: 0,
        });
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "466.3.c · she inherits only the result where she stands"
        );
        ctx.raise(Event::CombatWon {
            zone: fixtures::BF1,
            seat: 1,
        });
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the other seat's win is not hers"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
    }
}
