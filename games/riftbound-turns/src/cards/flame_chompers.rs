use super::prelude::{done, unit};
use super::{Card, Cost, Domain, Flow, Item, Power, Source, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::play;
use crate::state::{Leave, Origin};

pub const REKINDLE: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};

pub fn you_discarded_me_until_a_discarded_event_exists_and_the_trash_is_a_trigger_source(
    ctx: &Ctx,
    discarded: u32,
    by: u8,
    source: Source,
) -> bool {
    discarded == source.card && by == ctx.owner(source.card) && ctx.in_trash(source.card)
}

pub fn rekindle(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    if !ctx.in_trash(me) {
        ctx.narrate(format!(
            "{{card {me}}} is not in the trash · it stays where it is"
        ));
        return done();
    }
    let origin = Origin::Trash {
        leave: Leave::Banish,
    };
    if let Err(refusal) = play::begin(ctx, seat, me, origin, None) {
        ctx.narrate(format!(
            "{{card {me}}} cannot be played from the trash · {}",
            refusal.label()
        ));
        take_back(ctx, me);
    }
    done()
}

fn take_back(ctx: &mut Ctx, me: u32) {
    let pending: Vec<u16> = ctx
        .blob
        .queue
        .iter()
        .filter(|pending| pending.item.kind.card() == Some(me))
        .map(|pending| pending.item.id)
        .collect();
    for item in pending {
        play::cancel(ctx, item);
    }
}

pub static CARD: Card = unit("Flame Chompers", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const CHOMPERS: u32 = 90;

    fn chompers(zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(CHOMPERS, zone, seat, "Flame Chompers", 3);
        card.energy = Some(3);
        card
    }

    fn binned() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(chompers(fixtures::TRASH, 0));
        fixture.resolve();
        fixture
    }

    fn source() -> Source {
        Source {
            card: CHOMPERS,
            ability: 0,
        }
    }

    fn trigger() -> Item {
        Item::new(
            9,
            ItemKind::Trigger {
                source: CHOMPERS,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_stub_is_the_pool_name_with_no_keywords_and_the_discard_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Flame Chompers").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(REKINDLE.energy, 0);
        assert_eq!(REKINDLE.power, &[Power::Domain(Domain::Fury)]);
        let fixture = binned();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CHOMPERS).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn the_condition_reads_its_owner_discarding_it_into_the_trash_and_nothing_else() {
        let mut fixture = binned();
        let ctx = fixture.ctx();
        let seam =
            you_discarded_me_until_a_discarded_event_exists_and_the_trash_is_a_trigger_source;
        assert!(seam(&ctx, CHOMPERS, 0, source()));
        assert!(
            !seam(&ctx, fixtures::HAND_UNIT, 0, source()),
            "another card's discard is not mine"
        );
        assert!(
            !seam(&ctx, CHOMPERS, 1, source()),
            "an opponent's discard is not you discarding me"
        );
        drop(ctx);
        fixture.table.card_mut(CHOMPERS).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(
            !seam(&ctx, CHOMPERS, 0, source()),
            "383.2.c.1 · it must sit in the trash right after the discard"
        );
    }

    #[test]
    fn the_effect_plays_the_chompers_from_the_trash_for_its_printed_cost() {
        let mut fixture = binned();
        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        assert_eq!(rekindle(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert!(ctx.blob.queue.is_empty(), "the base is the only location");
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "nothing waits on an empty chain");
        assert_eq!(ctx.ready_runes_of(0).len(), ready - 3, "3 energy");
        assert_eq!(ctx.location(CHOMPERS), Some(Location::Base(0)));
        assert!(ctx.on_board(CHOMPERS));
        assert!(ctx.card(CHOMPERS).unwrap().exhausted, "no Accelerate");
        assert!(!ctx.in_trash(CHOMPERS));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {CHOMPERS}}} to their base"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_effect_takes_chompers_whose_cost_cannot_be_paid_back_to_the_trash() {
        let mut fixture = binned();
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(rekindle(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.in_trash(CHOMPERS));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} takes back {{card {CHOMPERS}}}")));
    }

    #[test]
    fn the_effect_refuses_chompers_that_are_not_in_the_trash() {
        let mut fixture = binned();
        fixture.table.card_mut(CHOMPERS).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(rekindle(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert!(ctx.in_hand(CHOMPERS));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {CHOMPERS}}} is not in the trash · it stays where it is"
        )));
    }

    #[test]
    #[ignore = "engine gap · no Discarded event, triggers::sources lists in-play cards only, and a play as an effect mid-resolution is unsupported; with them the script is optional(with_cost(triggered(Discarded(Who::Me), &[], rekindle), REKINDLE)) sourced from the trash"]
    fn discarding_the_chompers_offers_a_fury_to_play_them_from_the_trash() {
        let mut fixture = binned();
        fixture.table.card_mut(CHOMPERS).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.trash(CHOMPERS);
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the discard trigger from the trash"
        );
        assert!(matches!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::OptionalCost { .. })
        ));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(CHOMPERS));
    }
}
