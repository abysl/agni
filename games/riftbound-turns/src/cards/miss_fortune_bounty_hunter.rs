use super::prelude::{
    a_unit, activated, card_target, done, exhausting_self, grant_this_turn, legend, named,
};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const GRANTS: Keyword = Keyword::Ganking;

fn bounty(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if grant_this_turn(ctx, unit, GRANTS) {
            ctx.narrate(format!("{{card {unit}}} has Ganking this turn"));
        }
    }
    done()
}

pub static CARD: Card = legend(
    "Miss Fortune - Bounty Hunter",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            Cost::FREE,
            &[a_unit("a unit to give Ganking this turn")],
            bounty,
        )),
        "give a unit Ganking this turn",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, expiry, march, play, priority};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;

    const FORTUNE: u32 = fixtures::LEGEND_CARD;
    const DECKHAND: u32 = 90;

    fn harbour() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(FORTUNE).unwrap().name = CARD.name.into();
        fixture
            .table
            .cards
            .push(fixtures::unit(DECKHAND, fixtures::BF1, 0, "Deckhand", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn walks_between_battlefields(ctx: &Ctx, unit: u32) -> Result<(), Refusal> {
        march::legal_destination(
            ctx,
            unit,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF3),
        )
    }

    #[test]
    fn the_legend_has_one_free_sorcery_activation_paid_with_her_exhaust() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(
            CARD.keywords.is_empty(),
            "Ganking is what she gives, not hers"
        );
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.label, Some("give a unit Ganking this turn"));
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT, "any unit, either side's");
        assert_eq!(GRANTS, Keyword::Ganking);
        let mut fixture = harbour();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(FORTUNE).unwrap(), &CARD));
        assert!(cost::of_activation(&ctx, FORTUNE, 0).is_free());
        assert_eq!(
            activate::offers(&ctx, 0)
                .iter()
                .map(|offer| (offer.label.clone(), offer.enabled))
                .collect::<Vec<_>>(),
            [(
                format!("{{card {FORTUNE}}}: give a unit Ganking this turn (exhaust)"),
                true
            )]
        );
    }

    #[test]
    fn the_grant_lets_the_unit_march_battlefield_to_battlefield_until_the_turn_ends() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        assert_eq!(
            walks_between_battlefields(&ctx, DECKHAND),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        activate::activate(&mut ctx, 0, FORTUNE, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                "cancel".to_string(),
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {DECKHAND}}}"),
            ],
            "every unit on the board, hers and theirs"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {DECKHAND}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.card(FORTUNE).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == FORTUNE
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(DECKHAND)]);
        assert!(
            !ctx.has_keyword(DECKHAND, Keyword::Ganking),
            "nothing until it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.has_keyword(DECKHAND, Keyword::Ganking));
        assert_eq!(walks_between_battlefields(&ctx, DECKHAND), Ok(()));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {DECKHAND}}} has Ganking this turn")));
        assert_eq!(
            activate::activate(&mut ctx, 0, FORTUNE, 0),
            Err(Refusal::Exhausted),
            "she is spent for the turn"
        );
        expiry::at_expiration(&mut ctx);
        assert!(
            !ctx.has_keyword(DECKHAND, Keyword::Ganking),
            "the grant is for the turn"
        );
        assert_eq!(
            walks_between_battlefields(&ctx, DECKHAND),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_unit_can_be_given_ganking_too() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, FORTUNE, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.has_keyword(fixtures::SPRITE, Keyword::Ganking));
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::SPRITE,
                Location::Battlefield(fixtures::BF2),
                Location::Battlefield(fixtures::BF1)
            ),
            Ok(())
        );
    }

    #[test]
    fn a_cancelled_activation_leaves_her_ready_and_grants_nothing() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, FORTUNE, 0).unwrap();
        let item = match ctx.blob.why {
            Some(PromptWhy::Target { item, .. }) => item,
            other => panic!("{other:?}"),
        };
        ctx.blob.close_prompt();
        play::cancel(&mut ctx, item);
        assert!(!ctx.card(FORTUNE).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.is_empty());
        assert!(!ctx.has_keyword(DECKHAND, Keyword::Ganking));
    }

    #[test]
    fn the_activation_is_refused_for_the_wrong_seat_an_exhausted_legend_and_a_busy_chain() {
        let mut fixture = harbour();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, FORTUNE, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, FORTUNE, 1),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert!(activate::offers(&ctx, 1).is_empty());
        drop(ctx);
        let mut spent = harbour();
        spent.table.card_mut(FORTUNE).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, FORTUNE, 0),
            Err(Refusal::Exhausted)
        );
        assert!(activate::offers(&ctx, 0).is_empty());
        drop(ctx);
        let mut busy = harbour();
        let mut ctx = busy.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            activate::activate(&mut ctx, 0, FORTUNE, 0),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
        assert!(!ctx.card(FORTUNE).unwrap().exhausted);
    }
}
