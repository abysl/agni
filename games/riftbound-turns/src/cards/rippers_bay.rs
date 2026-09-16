use super::prelude::{battlefield, channel_exhausted, done, ONE_ENERGY};
use super::{Card, Cost, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const PRICE: Cost = ONE_ENERGY;
pub const RUNES: usize = 1;

pub fn a_unit_here_was_returned_to_a_players_hand(_: &Ctx, _: &Event, _: Source) -> bool {
    false
}

pub fn channel_one_rune_exhausted(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let channelled = channel_exhausted(ctx, seat, RUNES);
    ctx.narrate(format!(
        "{{seat {seat}}} channels {channelled} rune exhausted · {{card {}}}",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = battlefield("Ripper's Bay", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{bounce, on_unit_played_here, optional, with_cost, Location};
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle, triggers};
    use crate::state::{ItemKind, PromptWhy, SLOT_TRIGGER_COST};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const BAY: u32 = fixtures::GROUNDS;
    const JINX: u32 = 90;

    static WIRED_TO_UNITS_PLAYED_HERE_FOR_THE_TEST: Card = battlefield(
        "Ripper's Bay",
        &[],
        &[optional(with_cost(
            on_unit_played_here(&[], channel_one_rune_exhausted),
            PRICE,
        ))],
    );

    fn bay() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(BAY).unwrap().name = "Ripper's Bay".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        let mut jinx = fixtures::unit(JINX, fixtures::HAND, 0, "Jinx", 2);
        jinx.energy = Some(1);
        fixture.table.cards.push(jinx);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(BAY).unwrap(), &CARD));
        fixture
    }

    fn wired() -> Fixture {
        let mut fixture = bay();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BAY, &WIRED_TO_UNITS_PLAYED_HERE_FOR_THE_TEST);
        fixture
    }

    fn bay_items(ctx: &Ctx) -> Vec<u16> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == BAY))
            .map(|item| item.id)
            .collect()
    }

    fn pool(ctx: &Ctx, seat: u8) -> Vec<(u32, bool)> {
        ctx.table
            .held(fixtures::RUNE_POOL, seat)
            .map(|rune| (rune.id, rune.exhausted))
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
    fn the_stub_is_the_pool_name_and_the_return_to_hand_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Ripper's Bay").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(PRICE, ONE_ENERGY);
        assert_eq!(RUNES, 1);
        let wired = &WIRED_TO_UNITS_PLAYED_HERE_FOR_THE_TEST.abilities[0];
        assert_eq!(wired.trigger, Trigger::UnitPlayedHere);
        assert!(wired.optional, "that player may pay");
        assert_eq!(wired.cost, Some(PRICE));
        assert!(wired.targets.is_empty());
    }

    #[test]
    fn today_a_unit_returned_to_hand_here_raises_no_event_and_the_bay_stays_quiet() {
        let mut fixture = bay();
        let mut ctx = fixture.ctx();
        let events = ctx.events.len();
        assert!(bounce(&mut ctx, fixtures::VI));
        assert!(!ctx.on_board(fixtures::VI));
        assert_eq!(
            ctx.events.len(),
            events,
            "Ctx::bounce raises nothing a trigger could hear"
        );
        assert!(triggers::find(
            &ctx,
            ctx.events.last().unwrap_or(&Event::EndingStep { seat: 0 })
        )
        .iter()
        .all(|found| found.source != BAY));
        settle(&mut ctx).unwrap();
        assert!(bay_items(&ctx).is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(pool(&ctx, 0).len(), 4);
        let source = Source {
            card: BAY,
            ability: 0,
        };
        assert!(!a_unit_here_was_returned_to_a_players_hand(
            &ctx,
            &Event::EndingStep { seat: 0 },
            source
        ));
    }

    #[test]
    fn wired_that_player_is_asked_for_one_energy_and_yes_channels_a_rune_exhausted() {
        let mut fixture = wired();
        let mut ctx = fixture.ctx();
        assert_eq!(pool(&ctx, 0).len(), 4);
        play_here(&mut ctx, 0, JINX);
        let items = bay_items(&ctx);
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
            format!("pay 1 energy for the {{card {BAY}}} trigger · {{card {JINX}}}?")
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
            "one rune pays the energy"
        );
        assert_eq!(pool(&ctx, 0).len(), 4, "not before the trigger resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let after = pool(&ctx, 0);
        assert_eq!(after.len(), 5);
        assert_eq!(
            after.iter().filter(|(_, exhausted)| *exhausted).count(),
            4,
            "Jinx's energy, the trigger's energy, the fixture's exhausted rune and the channelled one"
        );
        assert!(
            after
                .iter()
                .any(|(rune, exhausted)| *rune == 32 && *exhausted),
            "the top of the rune deck arrives exhausted: {after:?}"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} channels 1 rune exhausted · {{card {BAY}}}"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn wired_no_channels_nothing_and_an_empty_rune_deck_channels_nothing_for_the_energy() {
        let mut fixture = wired();
        let mut ctx = fixture.ctx();
        play_here(&mut ctx, 0, JINX);
        let ready = ctx.ready_runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.ready_runes_of(0).len(), ready);
        assert_eq!(pool(&ctx, 0).len(), 4);
        drop(ctx);
        let mut dry = wired();
        dry.table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::RUNE_DECK) && card.owner == 0));
        dry.resolve();
        dry.scripts = dry
            .scripts
            .clone()
            .with_script(BAY, &WIRED_TO_UNITS_PLAYED_HERE_FOR_THE_TEST);
        let mut ctx = dry.ctx();
        play_here(&mut ctx, 0, JINX);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(pool(&ctx, 0).len(), 4);
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} channels 0 rune exhausted · {{card {BAY}}}"
        )));
    }

    #[test]
    #[ignore = "engine gap · missing trigger subject: no Event for a unit returned to a player's hand (Ctx::bounce raises nothing), so no Trigger can hear it; the primitive is Event::ReturnedToHand { card, from, owner } raised by Ctx::bounce and a Trigger arm that hands the owner over as the controller, with rippers_bay::channel_one_rune_exhausted as the run"]
    fn a_unit_returned_to_hand_from_here_lets_its_owner_pay_one_to_channel_a_rune_exhausted() {
        let mut fixture = bay();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(bounce(&mut ctx, fixtures::VI));
        settle(&mut ctx).unwrap();
        assert_eq!(bay_items(&ctx).len(), 1);
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(pool(&ctx, 0).len(), 5);
    }
}
