use super::prelude::{
    battlefield, buff, done, on_unit_played_here, optional, trigger_subject, with_cost, ONE_ENERGY,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn buff_the_unit_played(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = trigger_subject(item) else {
        return done();
    };
    if !ctx.on_board(unit) {
        ctx.narrate(format!("{{card {unit}}} has left the board · no buff"));
        return done();
    }
    if buff(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is buffed"));
    } else {
        ctx.narrate(format!("{{card {unit}}} already has a buff"));
    }
    done()
}

pub static CARD: Card = battlefield(
    "Valley of Idols",
    &[],
    &[optional(with_cost(
        on_unit_played_here(&[], buff_the_unit_played),
        ONE_ENERGY,
    ))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle};
    use crate::state::{ItemKind, PromptWhy, SLOT_TRIGGER_COST};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const VALLEY: u32 = fixtures::GROUNDS;
    const JINX: u32 = 90;
    const THEIR_ROOKIE: u32 = 91;

    fn valley() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(VALLEY).unwrap().name = "Valley of Idols".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut jinx = fixtures::unit(JINX, fixtures::HAND, 0, "Jinx", 2);
        jinx.energy = Some(1);
        fixture.table.cards.push(jinx);
        let mut rookie = fixtures::unit(THEIR_ROOKIE, fixtures::HAND, 1, "Their Rookie", 1);
        rookie.energy = Some(1);
        fixture.table.cards.push(rookie);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(VALLEY).unwrap(),
            &CARD
        ));
        fixture
    }

    fn valley_items(ctx: &Ctx) -> Vec<u16> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(
                |item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == VALLEY),
            )
            .map(|item| item.id)
            .collect()
    }

    fn play_here(ctx: &mut Ctx, seat: u8, card: u32) {
        fixtures::play_from_hand(ctx, seat, card).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })) {
            fixtures::choose(ctx, seat, "{zone 9}").unwrap();
        }
        fixtures::pass_until_open(ctx);
        settle(ctx).unwrap();
    }

    #[test]
    fn the_valley_is_one_optional_one_energy_trigger_on_a_unit_played_here() {
        assert!(std::ptr::eq(script_of("Valley of Idols").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::UnitPlayedHere);
        assert!(ability.optional);
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_none());
    }

    #[test]
    fn the_player_who_plays_a_unit_here_is_asked_for_one_energy_and_yes_buffs_it() {
        let mut fixture = valley();
        let mut ctx = fixture.ctx();
        play_here(&mut ctx, 0, JINX);
        assert_eq!(
            ctx.location(JINX),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let items = valley_items(&ctx);
        assert_eq!(items.len(), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: items[0],
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("pay 1 energy for the {{card {VALLEY}}} trigger · {{card {JINX}}}?")
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0);
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 1,
            "one rune exhausts for the energy"
        );
        assert!(!ctx.is_buffed(JINX), "not before the trigger resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(JINX));
        assert_eq!(ctx.current_might(JINX), 3);
        assert!(ctx.blob.log.contains(&format!("{{card {JINX}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn no_pays_nothing_and_buffs_nothing() {
        let mut fixture = valley();
        let mut ctx = fixture.ctx();
        play_here(&mut ctx, 0, JINX);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.ready_runes_of(0).len(), ready);
        assert!(!ctx.is_buffed(JINX));
        assert_eq!(ctx.current_might(JINX), 2);
    }

    #[test]
    fn the_opponent_playing_here_is_asked_too_and_without_a_ready_rune_nobody_is_asked() {
        let mut theirs = valley();
        theirs.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        theirs.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        theirs.blob.set_holder(fixtures::BF1, Some(1));
        theirs.blob.core_mut().unwrap().player = 1;
        theirs.resolve();
        let mut ctx = theirs.ctx();
        play_here(&mut ctx, 1, THEIR_ROOKIE);
        assert_eq!(valley_items(&ctx).len(), 1);
        assert_eq!(ctx.blob.prompt.as_ref().map(|prompt| prompt.seat), Some(1));
        fixtures::choose(&mut ctx, 1, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(THEIR_ROOKIE));
        assert_eq!(ctx.current_might(THEIR_ROOKIE), 2);
        drop(ctx);
        let mut broke = valley();
        for rune in [42, 43] {
            broke.table.card_mut(rune).unwrap().exhausted = true;
        }
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "exactly the unit's energy");
        play_here(&mut ctx, 0, JINX);
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        assert!(
            ctx.blob.prompt.is_none(),
            "no cost confirm for a cost that can't be paid"
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(JINX));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with(&format!("{{card {VALLEY}}} trigger is removed"))));
    }

    #[test]
    fn a_token_played_here_asks_and_a_unit_played_elsewhere_does_not() {
        let mut fixture = valley();
        let mut ctx = fixture.ctx();
        let sprite = spawn(
            &mut ctx,
            0,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            false,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(valley_items(&ctx).len(), 1, "a token is a unit played here");
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(sprite));
        drop(ctx);
        let mut fixture = valley();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, JINX).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })) {
            fixtures::choose(&mut ctx, 0, "your base").unwrap();
        }
        fixtures::pass_until_open(&mut ctx);
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(JINX), Some(Location::Base(0)));
        assert!(valley_items(&ctx).is_empty());
        assert!(ctx.blob.prompt.is_none());
    }
}
