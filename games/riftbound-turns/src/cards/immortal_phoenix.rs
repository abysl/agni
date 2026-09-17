use super::prelude::{done, unit};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Source, Stage};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::play;
use crate::state::{Leave, Origin};

pub const ASSAULT: u8 = 2;
pub const RISE: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Fury)],
};

pub fn killed_with_your_spell_until_died_names_its_killer_and_the_trash_is_a_trigger_source(
    ctx: &Ctx,
    cause: Cause,
    source: Source,
) -> bool {
    let item = match cause {
        Cause::Item(item)
        | Cause::Cleanup {
            last_item: Some(item),
        } => item,
        _ => return false,
    };
    ctx.chain_item(item).is_some_and(|held| {
        ctx.is_spell(held.kind.source()) && held.controller == ctx.controller(source.card)
    })
}

pub fn rise(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
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

pub static CARD: Card = unit("Immortal Phoenix", &[Keyword::Assault(ASSAULT)], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, ItemStatus, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const PHOENIX: u32 = 90;
    const SPELL_ITEM: u16 = 3;
    const THEIR_SPELL_ITEM: u16 = 4;
    const UNIT_ITEM: u16 = 5;

    fn phoenix(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(PHOENIX, zone, 0, "Immortal Phoenix", 3);
        card.energy = Some(3);
        card.power = Some(1);
        card
    }

    fn ashes() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(phoenix(fixtures::TRASH));
        fixture.resolve();
        fixture
    }

    fn on_the_chain(ctx: &mut Ctx, id: u16, kind: ItemKind, controller: u8) {
        let mut item = ChainItem::new(id, kind, controller, Origin::Hand);
        item.status = ItemStatus::Finalized;
        ctx.blob.chain.push(item);
    }

    fn source() -> Source {
        Source {
            card: PHOENIX,
            ability: 0,
        }
    }

    fn trigger() -> Item {
        Item::new(
            9,
            ItemKind::Trigger {
                source: PHOENIX,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_stub_prints_assault_two_and_the_trash_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Immortal Phoenix").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Assault(ASSAULT)]);
        assert!(CARD.abilities.is_empty());
        assert_eq!(ASSAULT, 2);
        assert_eq!(RISE.energy, 1);
        assert_eq!(RISE.power, &[Power::Domain(Domain::Fury)]);
        let fixture = ashes();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(PHOENIX).unwrap(),
            &CARD
        ));
    }

    #[test]
    fn the_condition_reads_a_kill_whose_cause_is_a_spell_of_the_phoenixs_controller() {
        let mut fixture = ashes();
        let mut ctx = fixture.ctx();
        on_the_chain(
            &mut ctx,
            SPELL_ITEM,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
        );
        on_the_chain(
            &mut ctx,
            THEIR_SPELL_ITEM,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
        );
        on_the_chain(
            &mut ctx,
            UNIT_ITEM,
            ItemKind::Permanent {
                card: fixtures::HAND_UNIT,
            },
            0,
        );
        let seam =
            killed_with_your_spell_until_died_names_its_killer_and_the_trash_is_a_trigger_source;
        assert!(seam(&ctx, Cause::Item(SPELL_ITEM), source()));
        assert!(
            seam(
                &ctx,
                Cause::Cleanup {
                    last_item: Some(SPELL_ITEM)
                },
                source()
            ),
            "428.5.c.1 · lethal damage in the cleanup after the spell is the spell's kill"
        );
        assert!(
            !seam(&ctx, Cause::Item(THEIR_SPELL_ITEM), source()),
            "their spell is not yours"
        );
        assert!(
            !seam(&ctx, Cause::Item(UNIT_ITEM), source()),
            "a unit's play is not a spell"
        );
        assert!(!seam(&ctx, Cause::Combat, source()));
        assert!(!seam(&ctx, Cause::Rule, source()));
        assert!(!seam(&ctx, Cause::Cleanup { last_item: None }, source()));
        assert!(
            !seam(&ctx, Cause::Item(77), source()),
            "an item that already left the chain cannot be read"
        );
    }

    #[test]
    fn the_effect_plays_the_phoenix_from_the_trash_for_its_printed_cost_into_the_only_open_location(
    ) {
        let mut fixture = ashes();
        let mut ctx = fixture.ctx();
        assert!(ctx.in_trash(PHOENIX));
        let ready = ctx.ready_runes_of(0).len();
        assert_eq!(rise(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
        assert!(
            ctx.blob.queue.is_empty(),
            "the base is the only location · no prompt"
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "nothing waits on an empty chain");
        assert!(ctx.ready_runes_of(0).len() < ready, "runes paid for it");
        assert_eq!(ctx.location(PHOENIX), Some(Location::Base(0)));
        assert!(ctx.on_board(PHOENIX));
        assert!(ctx.card(PHOENIX).unwrap().exhausted, "no Accelerate");
        assert!(!ctx.in_trash(PHOENIX));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {PHOENIX}}} to their base"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_effect_leaves_a_phoenix_whose_printed_cost_cannot_be_paid_in_the_trash() {
        let mut fixture = ashes();
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(rise(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.blob.queue.is_empty(),
            "{:?} {:?}",
            ctx.blob.log,
            ctx.blob.queue
        );
        assert!(ctx.in_trash(PHOENIX), "taken back to the trash");
    }

    #[test]
    fn the_effect_refuses_a_phoenix_that_is_not_in_the_trash() {
        let mut fixture = ashes();
        fixture.table.card_mut(PHOENIX).unwrap().zone = Some(fixtures::BANISHMENT);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(rise(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert!(ctx.in_banishment(PHOENIX));
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {PHOENIX}}} is not in the trash · it stays where it is"
        )));
    }

    #[test]
    #[ignore = "engine gap · Died carries no killer, triggers::sources lists in-play cards only, and a play as an effect mid-resolution is unsupported; with them the script is optional(with_cost(triggered(UnitKilledBy(YourSpell), &[], rise), RISE)) sourced from the trash"]
    fn a_spell_of_yours_that_kills_a_unit_offers_the_phoenix_a_paid_return_from_the_trash() {
        let mut fixture = ashes();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(fixtures::THEIR_UNIT), None);
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "may pay 1 energy and a Fury"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(PHOENIX));
    }
}
