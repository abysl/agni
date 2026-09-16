use super::prelude::{a_card, card_target, done, exhaust, play, ready, unit};
use super::{Card, Filter, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const LEGEND: TargetSpec = a_card(Filter::Legend, "a legend to ready or exhaust");

pub fn is_exhausted(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card).is_some_and(|held| held.exhausted)
}

fn escort(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(legend) = card_target(ctx, item, 0) else {
        return done();
    };
    if is_exhausted(ctx, legend) {
        if ready(ctx, legend) {
            ctx.narrate(format!("{{card {legend}}} is readied"));
        }
    } else if exhaust(ctx, legend) {
        ctx.narrate(format!("{{card {legend}}} is exhausted"));
    }
    done()
}

pub static CARD: Card = unit("Royal Entourage", &[], &[play(&[LEGEND], escort)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const ENTOURAGE: u32 = 90;
    const THEIR_LEGEND: u32 = 91;

    fn entourage(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(ENTOURAGE, zone, 0, "Royal Entourage", 4)
        }
    }

    fn court(their_legend_exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(entourage(fixtures::HAND));
        let mut theirs =
            fixtures::card(THEIR_LEGEND, fixtures::LEGEND, 1, "Jinx - Rebel", "Legend");
        theirs.exhausted = their_legend_exhausted;
        fixture.table.cards.push(theirs);
        fixture.resolve();
        fixture
    }

    fn pending(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the play trigger is pending")
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_play_trigger_targets_one_legend() {
        assert!(std::ptr::eq(script_of("Royal Entourage").unwrap(), &CARD));
        assert_eq!(CARD.name, "Royal Entourage");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert_eq!(ability.targets, &[LEGEND]);
        assert_eq!((LEGEND.min, LEGEND.max), (1, 1));
        assert_eq!(LEGEND.kind, TargetKind::Card);
        assert_eq!(LEGEND.filter, Filter::Legend);
    }

    #[test]
    fn playing_it_offers_both_legends_and_an_exhausted_enemy_legend_is_readied() {
        let mut fixture = court(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ENTOURAGE).unwrap();
        assert_eq!(ctx.location(ENTOURAGE), Some(Location::Base(0)));
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::LEGEND_CARD),
                format!("{{card {THEIR_LEGEND}}}"),
            ],
            "either seat's legend, nothing else"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {ENTOURAGE}}}: choose a legend to ready or exhaust (0 of 1)")
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit is not a legend"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::CHAMPION_CARD]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the champion in its zone is not a legend"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_LEGEND}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == ENTOURAGE
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_LEGEND)]);
        assert!(
            ctx.card(THEIR_LEGEND).unwrap().exhausted,
            "the flip waits for the trigger"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(THEIR_LEGEND).unwrap().exhausted);
        assert!(ctx.effects.contains(&Effect::ready(THEIR_LEGEND)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, .. } if *card == THEIR_LEGEND
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {THEIR_LEGEND}}} is readied")));
        assert!(
            !ctx.card(fixtures::LEGEND_CARD).unwrap().exhausted,
            "the other legend is untouched"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ready_legend_is_exhausted_instead() {
        let mut fixture = court(false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ENTOURAGE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::LEGEND_CARD)).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(fixtures::LEGEND_CARD).unwrap().exhausted);
        assert!(ctx
            .effects
            .contains(&Effect::exhaust(fixtures::LEGEND_CARD)));
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::Readied { .. })),
            "exhausting raises no Readied"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is exhausted", fixtures::LEGEND_CARD)));
        assert!(
            !ctx.card(THEIR_LEGEND).unwrap().exhausted,
            "the other legend is untouched"
        );
    }

    #[test]
    fn a_legend_flipped_in_response_is_flipped_back_by_the_resolving_trigger() {
        let mut fixture = court(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ENTOURAGE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_LEGEND}}}")).unwrap();
        ctx.table.card_mut(THEIR_LEGEND).unwrap().exhausted = false;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.card(THEIR_LEGEND).unwrap().exhausted,
            "ready or exhaust reads the legend as it resolves"
        );
        assert!(ctx.effects.contains(&Effect::exhaust(THEIR_LEGEND)));
    }
}
