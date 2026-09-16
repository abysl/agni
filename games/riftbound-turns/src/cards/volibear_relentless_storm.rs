use super::prelude::{
    channel_exhausted, done, exhausting_self, is_mighty, legend, optional, triggered, when,
};
use super::{Card, Flow, Item, Source, Stage, Trigger, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};
use crate::state::Origin;

pub const CHANNELS: usize = 1;

pub fn a_mighty_unit_you_played(ctx: &Ctx, event: &Event, _: Source) -> bool {
    let Event::Played {
        card, origin, kind, ..
    } = event
    else {
        return false;
    };
    *origin != Origin::Board && kind == KIND_UNIT && is_mighty(ctx, *card)
}

fn thunder(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    channel_exhausted(ctx, item.controller, CHANNELS);
    done()
}

pub static CARD: Card = legend(
    "Volibear - Relentless Storm",
    &[],
    &[when(
        optional(exhausting_self(triggered(
            Trigger::YouPlayCard,
            &[],
            thunder,
        ))),
        a_mighty_unit_you_played,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::MIGHTY;
    use crate::cards::{script_of, SelfCost};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts};
    use crate::state::{ItemKind, PromptWhy, SLOT_TRIGGER_COST};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const VOLIBEAR: u32 = fixtures::LEGEND_CARD;
    const URSINE: u32 = 90;
    const THEIR_URSINE: u32 = 91;

    fn ursine(id: u32, seat: u8, might: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Body".into()],
            ..fixtures::unit(id, fixtures::HAND, seat, "Storm Bear", might)
        }
    }

    fn storm(might: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(VOLIBEAR).unwrap().name = CARD.name.into();
        fixture.table.cards.push(ursine(URSINE, 0, might));
        fixture.resolve();
        fixture
    }

    fn runes_in_pool(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_POOL, seat).count()
    }

    fn play_the_ursine(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, URSINE).unwrap();
        if ctx.blob.why == Some(PromptWhy::PlayLocation { item: 1 }) {
            fixtures::choose(ctx, 0, "your base").unwrap();
        }
    }

    #[test]
    fn the_legend_has_one_may_trigger_on_your_unit_plays_gated_on_mighty_and_paid_by_exhausting() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlayCard);
        assert!(ability.optional, "you may exhaust me");
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!(CHANNELS, 1);
        assert_eq!(MIGHTY, 5);
        let mut fixture = storm(MIGHTY as u8);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(VOLIBEAR).unwrap(), &CARD));
    }

    #[test]
    fn the_condition_reads_the_played_unit_on_the_board_and_ignores_spells_and_enemy_plays() {
        let mut fixture = storm(MIGHTY as u8);
        fixture.table.card_mut(URSINE).unwrap().zone = Some(fixtures::BASE);
        fixture.table.cards.push(CardInfo {
            zone: Some(fixtures::BASE),
            ..ursine(THEIR_URSINE, 1, 6)
        });
        fixture.resolve();
        let ctx = fixture.ctx();
        let me = Source {
            card: VOLIBEAR,
            ability: 0,
        };
        let played = |card: u32, controller: u8, kind: &str| Event::Played {
            card,
            controller,
            kind: kind.into(),
            origin: Origin::Hand,
            paid_additional: false,
        };
        assert!(a_mighty_unit_you_played(
            &ctx,
            &played(URSINE, 0, KIND_UNIT),
            me
        ));
        assert!(
            !a_mighty_unit_you_played(&ctx, &played(fixtures::VI, 0, KIND_UNIT), me),
            "Vi's 3 Might is not Mighty"
        );
        assert!(
            !a_mighty_unit_you_played(&ctx, &played(URSINE, 0, "Spell"), me),
            "a spell is not a unit"
        );
        assert!(
            !a_mighty_unit_you_played(
                &ctx,
                &Event::Played {
                    card: URSINE,
                    controller: 0,
                    kind: KIND_UNIT.into(),
                    origin: Origin::Board,
                    paid_additional: false,
                },
                me
            ),
            "a unit already on the board was not played"
        );
        assert!(
            crate::engine::triggers::matches(
                &ctx,
                Trigger::YouPlayCard,
                &played(THEIR_URSINE, 1, KIND_UNIT),
                VOLIBEAR
            )
            .is_none(),
            "an enemy play never reaches the condition · YouPlayCard is the controller's own"
        );
    }

    #[test]
    fn playing_a_mighty_unit_asks_to_exhaust_him_and_a_yes_channels_one_rune_exhausted() {
        let mut fixture = storm(MIGHTY as u8);
        let mut ctx = fixture.ctx();
        let pool = runes_in_pool(&ctx, 0);
        let deck = ctx.top_of(fixtures::RUNE_DECK, 0, 3).len();
        assert_eq!(deck, 3);
        play_the_ursine(&mut ctx);
        assert_eq!(ctx.card(URSINE).unwrap().zone, Some(fixtures::BASE));
        assert!(ctx.is_unit(URSINE));
        assert!(is_mighty(&ctx, URSINE));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_TRIGGER_COST as u8
            }),
            "383.3.a · the may is the cost confirm"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("exhaust {{card {VOLIBEAR}}} for the {{card {VOLIBEAR}}} trigger · {{card {URSINE}}}?")
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the may is his controller's"
        );
        assert!(!ctx.card(VOLIBEAR).unwrap().exhausted);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(VOLIBEAR).unwrap().exhausted,
            "exhausting him is the cost"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == VOLIBEAR
        ));
        assert_eq!(runes_in_pool(&ctx, 0), pool, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(runes_in_pool(&ctx, 0), pool + 1);
        let arrived: Vec<&CardInfo> = ctx
            .table
            .held(fixtures::RUNE_POOL, 0)
            .filter(|rune| (30..=32).contains(&rune.id))
            .collect();
        assert_eq!(arrived.len(), 1, "one rune came from the rune deck");
        assert!(arrived[0].exhausted, "it arrives exhausted");
        assert_eq!(ctx.top_of(fixtures::RUNE_DECK, 0, 3).len(), 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_removes_the_trigger_and_leaves_him_ready() {
        let mut fixture = storm(MIGHTY as u8 + 2);
        let mut ctx = fixture.ctx();
        let pool = runes_in_pool(&ctx, 0);
        play_the_ursine(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(VOLIBEAR).unwrap().exhausted);
        assert_eq!(runes_in_pool(&ctx, 0), pool);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {VOLIBEAR}}} trigger is removed · its cost is declined"
        )));
    }

    #[test]
    fn a_unit_short_of_five_might_does_not_trigger_him_and_a_spent_legend_is_not_asked() {
        let mut fixture = storm(MIGHTY as u8 - 1);
        let mut ctx = fixture.ctx();
        play_the_ursine(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "4 Might is not Mighty");
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(VOLIBEAR).unwrap().exhausted);
        drop(ctx);
        let mut spent = storm(MIGHTY as u8);
        spent.table.card_mut(VOLIBEAR).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        let pool = runes_in_pool(&ctx, 0);
        play_the_ursine(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "no cost confirm for a cost that can't be paid"
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(runes_in_pool(&ctx, 0), pool);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {VOLIBEAR}}} trigger is removed · its source is exhausted"
        )));
    }
}
