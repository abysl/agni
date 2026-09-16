use super::prelude::{
    a_card, activated, card_target, done, might_this_turn, named, paying_with, unit,
};
use super::{Card, Cost, Filter, Flow, Item, Keyword, SelfCost, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
pub const CARD_IN_YOUR_TRASH: Filter = Filter::And(&[Filter::InTrash, Filter::Friendly]);

pub fn recycle_target(ctx: &mut Ctx, item: &Item) -> bool {
    let Some(card) = card_target(ctx, item, 0) else {
        return false;
    };
    if !ctx.in_trash(card) {
        return false;
    }
    ctx.recycle_to_bottom(card);
    ctx.narrate(format!(
        "{{card {card}}} is recycled for the {{card {}}} ability",
        item.kind.source()
    ));
    true
}

fn surge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !recycle_target(ctx, item) {
        return done();
    }
    might_this_turn(ctx, item, me, MIGHT, None);
    ctx.narrate(format!("{{card {me}}} gets +{MIGHT} might this turn"));
    done()
}

pub static CARD: Card = unit(
    "Vi - Destructive",
    &[Keyword::Ganking],
    &[named(
        paying_with(
            activated(
                Timing::Sorcery,
                Cost::FREE,
                &[a_card(
                    CARD_IN_YOUR_TRASH,
                    "a card in your trash to recycle",
                )],
                surge,
            ),
            SelfCost::Free,
        ),
        "recycle 1 for +1 Might this turn",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const VI: u32 = 90;
    const SCRAP: u32 = 91;
    const MORE_SCRAP: u32 = 92;
    const THEIR_SCRAP: u32 = 93;

    fn vi() -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            ..fixtures::unit(VI, fixtures::BF1, 0, "Vi - Destructive", 3)
        }
    }

    fn scrapyard(mine: &[u32]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(vi());
        for id in mine {
            fixture
                .table
                .cards
                .push(fixtures::spell(*id, fixtures::TRASH, 0, "Spark", 2, 1));
        }
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SCRAP, fixtures::TRASH, 1, "Jinx", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn her_offers(ctx: &Ctx) -> Vec<(String, bool)> {
        activate::offers(ctx, 0)
            .iter()
            .filter(|offer| offer.source == VI)
            .map(|offer| (offer.label.clone(), offer.enabled))
            .collect()
    }

    #[test]
    fn vi_prints_ganking_and_one_free_activated_ability_that_recycles_a_trash_card() {
        assert!(std::ptr::eq(script_of("Vi - Destructive").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Ganking]);
        assert_eq!(CARD.abilities.len(), 1);
        let surge = &CARD.abilities[0];
        assert_eq!(surge.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(surge.self_cost, SelfCost::Free, "no exhaust in the cost");
        assert_eq!(surge.cost, Some(Cost::FREE));
        assert_eq!(surge.targets.len(), 1);
        assert_eq!(surge.targets[0].filter, CARD_IN_YOUR_TRASH);
        assert_eq!(surge.label, Some("recycle 1 for +1 Might this turn"));
        assert_eq!(MIGHT, 1);
    }

    #[test]
    fn each_use_recycles_one_of_her_controllers_trash_cards_and_adds_one_might_until_the_turn_ends()
    {
        let mut fixture = scrapyard(&[SCRAP, MORE_SCRAP]);
        let mut ctx = fixture.ctx();
        assert_eq!(
            her_offers(&ctx),
            [(
                format!("{{card {VI}}}: recycle 1 for +1 Might this turn"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, VI, 0).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {SCRAP}}}"),
                format!("{{card {MORE_SCRAP}}}"),
                "cancel".to_string()
            ],
            "her controller's trash, not the opponent's"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCRAP}}}")).unwrap();
        assert!(!ctx.card(VI).unwrap().exhausted, "she stays ready");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.current_might(VI), 3, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(SCRAP).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert!(
            ctx.effects.contains(&Effect::Move {
                card: SCRAP,
                zone: fixtures::MAIN_DECK,
                seat: 0,
                index: BOTTOM
            }),
            "416.1 · recycled to the bottom of her owner's main deck: {:?}",
            ctx.effects
        );
        assert_eq!(ctx.current_might(VI), 4);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SCRAP}}} is recycled for the {{card {VI}}} ability"
        )));
        activate::activate(&mut ctx, 0, VI, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {MORE_SCRAP}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(VI),
            5,
            "twice in a turn, no exhaust between"
        );
        assert_eq!(ctx.trash_of(0), Vec::<u32>::new());
        assert_eq!(
            activate::activate(&mut ctx, 0, VI, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "416.3 · an empty trash cannot pay"
        );
        assert!(her_offers(&ctx).is_empty());
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(VI), 3, "this turn only");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_no_card_of_her_own_in_the_trash_the_ability_is_refused_and_an_opponent_cannot_use_it() {
        let mut fixture = scrapyard(&[]);
        let mut ctx = fixture.ctx();
        assert!(her_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, VI, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "the opponent's trash card is not hers to recycle"
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, VI, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(ctx.card(THEIR_SCRAP).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.current_might(VI), 3);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.effects.is_empty());
    }

    #[test]
    #[ignore = "engine gap · recycle-from-trash as an activation cost; SelfCost has BanishTarget but no RecycleTarget, so recycle_target runs at resolution instead of at the pay stage"]
    fn the_recycle_is_paid_when_the_ability_is_activated_not_when_it_resolves() {
        let mut fixture = scrapyard(&[SCRAP]);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, VI, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCRAP}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the ability waits on the chain");
        assert_eq!(
            ctx.card(SCRAP).unwrap().zone,
            Some(fixtures::MAIN_DECK),
            "204.1.b · the cost before the ':' is paid to finalize"
        );
        assert_eq!(ctx.current_might(VI), 3);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(VI), 4);
    }
}
