use super::prelude::{
    a_unit_at_a_battlefield, ask_discard, card_target, deal, discarded, done, play, spell,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

const STAGE_DEAL: u8 = 1;

pub fn energy_cost_of(ctx: &Ctx, card: u32) -> u8 {
    ctx.card(card).and_then(|held| held.energy).unwrap_or(0)
}

fn excite(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 != STAGE_DEAL {
        return match ask_discard(ctx, item, STAGE_DEAL) {
            Some(ask) => Flow::Ask(ask),
            None => {
                ctx.narrate(format!(
                    "{{card {}}} · nothing to discard, nothing to deal",
                    item.kind.source()
                ));
                done()
            }
        };
    }
    let Some(card) = discarded(ctx) else {
        return done();
    };
    let amount = energy_cost_of(ctx, card);
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if amount == 0 {
        ctx.narrate(format!(
            "{{card {card}}} costs no energy · {{card {unit}}} takes nothing"
        ));
        return done();
    }
    if deal(ctx, item, unit, amount) {
        ctx.narrate(format!(
            "{{card {unit}}} takes {amount} · the Energy cost of {{card {card}}}"
        ));
    }
    done()
}

pub static CARD: Card = spell(
    "Get Excited!",
    &[Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        excite,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::PromptWhy;
    use agni_plugin_sdk::table::CardInfo;

    const EXCITED: u32 = 90;
    const BRUTE: u32 = 91;
    const PRICEY: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            EXCITED,
            fixtures::HAND,
            0,
            "Get Excited!",
            2,
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture.table.cards.push(CardInfo {
            power: Some(3),
            ..fixtures::unit(PRICEY, fixtures::HAND, 0, "Pricey", 6)
        });
        fixture.table.card_mut(PRICEY).unwrap().energy = Some(5);
        fixture.resolve();
        fixture
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, EXCITED).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_an_action_that_chooses_at_a_battlefield_then_asks_for_the_discard() {
        assert!(std::ptr::eq(script_of("Get Excited!").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item: 1,
                stage: STAGE_DEAL
            }),
            "the discard is asked as the spell resolves"
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(ctx.damage_on(BRUTE), 0, "no damage before the discard");
    }

    #[test]
    fn the_discarded_cards_energy_cost_is_the_damage_and_its_power_is_ignored() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        fixtures::choose(&mut ctx, 0, &format!("{{card {PRICEY}}}")).unwrap();
        assert_eq!(ctx.card(PRICEY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: 5,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(BRUTE), "five energy kills five Might");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(EXCITED).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
        let mut cheap = armed();
        let mut ctx = cheap.ctx();
        cast_at(&mut ctx, BRUTE);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_UNIT)).unwrap();
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: 2,
            source: Cause::Item(1)
        }));
        assert!(
            ctx.on_board(BRUTE),
            "a two-energy discard leaves it standing"
        );
        assert_eq!(ctx.damage_on(BRUTE), 2);
    }

    #[test]
    fn with_an_empty_hand_nothing_is_discarded_and_nothing_is_dealt() {
        let mut fixture = armed();
        fixture.table.cards.retain(|card| {
            card.zone != Some(fixtures::HAND) || card.id == EXCITED || card.owner != 0
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EXCITED).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "no card to discard, no prompt");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} · nothing to discard, nothing to deal".to_string()));
    }

    #[test]
    fn a_target_that_left_the_battlefield_still_costs_the_discard_but_takes_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, fixtures::SPRITE);
        ctx.recall(fixtures::SPRITE, false);
        fixtures::choose(&mut ctx, 0, &format!("{{card {PRICEY}}}")).unwrap();
        assert_eq!(ctx.card(PRICEY).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
    }
}
