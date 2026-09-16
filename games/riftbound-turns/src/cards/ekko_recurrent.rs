use super::prelude::{deathknell, done, ready_runes, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub fn recycle_me_as_the_base_cost_until_self_cost_recycle_self_pays_it_at_finalization(
    ctx: &mut Ctx,
    me: u32,
) -> bool {
    if !ctx.in_trash(me) {
        return false;
    }
    ctx.recycle_to_bottom(me);
    ctx.narrate(format!("{{card {me}}} is recycled"));
    true
}

fn rewind(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    if !recycle_me_as_the_base_cost_until_self_cost_recycle_self_pays_it_at_finalization(ctx, me) {
        ctx.narrate(format!(
            "{{card {me}}} is not in the trash · the runes stay as they are"
        ));
        return done();
    }
    let count = ctx.runes_of(seat).len();
    let readied = ready_runes(ctx, seat, count);
    ctx.narrate(format!("{{seat {seat}}} readies {readied} runes"));
    done()
}

pub static CARD: Card = unit(
    "Ekko - Recurrent",
    &[Keyword::Accelerate, Keyword::Deathknell],
    &[deathknell(&[], rewind)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::ctx::{Cause, Event, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle};
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const EKKO: u32 = 90;
    const MY_RUNES: [u32; 4] = [fixtures::RUNE_A, 41, 42, 43];

    fn ekko(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(EKKO, zone, 0, "Ekko - Recurrent", 5);
        card.energy = Some(5);
        card.power = Some(1);
        card.domain = vec!["Mind".into()];
        card
    }

    fn spent() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ekko(fixtures::BASE));
        for rune in [41, 42] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.table.card_mut(45).unwrap().exhausted = true;
        fixture.resolve();
        fixture
    }

    fn resolve(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn deck_bottom(ctx: &Ctx, seat: u8) -> Option<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .next()
    }

    #[test]
    fn the_script_prints_accelerate_and_deathknell_with_one_untargeted_death_ability() {
        assert!(std::ptr::eq(script_of("Ekko - Recurrent").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Accelerate, Keyword::Deathknell]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Death);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert_eq!(
            ability.self_cost,
            SelfCost::Auto,
            "no SelfCost recycles the source · the script pays it on resolution"
        );
        let mut fixture = spent();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(EKKO).unwrap(), &CARD));
        assert!(ctx.has_keyword(EKKO, Keyword::Accelerate));
    }

    #[test]
    fn a_dead_ekko_is_recycled_to_the_bottom_of_the_deck_and_every_rune_of_his_controller_readies()
    {
        let mut fixture = spent();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert_eq!(ctx.kill(EKKO, Cause::Rule), Killed::Yes);
        assert!(ctx.in_trash(EKKO));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == EKKO
        ));
        assert!(ctx.in_trash(EKKO), "the recycle waits for the chain");
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.in_trash(EKKO));
        assert_eq!(ctx.card(EKKO).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert_eq!(deck_bottom(&ctx, 0), Some(EKKO));
        assert!(ctx.effects.contains(&Effect::Move {
            card: EKKO,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert_eq!(ctx.ready_runes_of(0).len(), MY_RUNES.len());
        for rune in [fixtures::RUNE_A, 41, 42] {
            assert!(ctx.events.contains(&Event::Readied { card: rune, by: 0 }));
        }
        assert!(
            ctx.card(45).unwrap().exhausted,
            "the opponent's runes are not yours"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {EKKO}}} is recycled")));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} readies 3 runes".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_ekko_banished_out_of_the_trash_in_response_cannot_pay_and_readies_nothing() {
        let mut fixture = spent();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(EKKO, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert!(ctx.banish(EKKO));
        assert!(ctx.in_banishment(EKKO));
        resolve(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_banishment(EKKO), "it stays banished");
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { .. })));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {EKKO}}} is not in the trash · the runes stay as they are"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_seam_refuses_a_card_that_is_not_in_the_trash() {
        let mut fixture = spent();
        let mut ctx = fixture.ctx();
        assert!(
            !recycle_me_as_the_base_cost_until_self_cost_recycle_self_pays_it_at_finalization(
                &mut ctx, EKKO
            ),
            "on the board is not in the trash"
        );
        assert!(ctx.on_board(EKKO));
        assert!(ctx.effects.is_empty());
        ctx.trash(EKKO);
        assert!(
            recycle_me_as_the_base_cost_until_self_cost_recycle_self_pays_it_at_finalization(
                &mut ctx, EKKO
            )
        );
        assert_eq!(deck_bottom(&ctx, 0), Some(EKKO));
    }

    #[test]
    #[ignore = "engine gap · 383.3.b makes 'recycle me' the base cost of the trigger, paid at finalization by a SelfCost::RecycleSelf the engine lacks; the script pays it on resolution instead"]
    fn the_recycle_is_paid_when_the_trigger_is_finalized_not_when_it_resolves() {
        let mut fixture = spent();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(EKKO, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.card(EKKO).unwrap().zone,
            Some(fixtures::MAIN_DECK),
            "finalized with its cost paid · nothing can banish him in response"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "the effect still waits");
        resolve(&mut ctx);
        assert_eq!(ctx.ready_runes_of(0).len(), MY_RUNES.len());
    }
}
