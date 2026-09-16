use super::jax_unrelenting::an_equipment_you_attached_to_me;
use super::prelude::{a_friendly_unit, buff, channel_exhausted, done, ready_runes, unit};
use super::{Card, Flow, Item, Source, Stage, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const READY_RUNES: usize = 2;
pub const CHANNELED: usize = 1;
pub const READY: u8 = 0;
pub const CHANNEL: u8 = 1;
pub const BUFF: u8 = 2;
pub const MODES: [u8; 3] = [READY, CHANNEL, BUFF];
pub const UNIT_TO_BUFF: TargetSpec = a_friendly_unit("a friendly unit to buff");

pub fn an_equipment_attached_to_me(ctx: &Ctx, gear: u32, wearer: u32, source: Source) -> bool {
    an_equipment_you_attached_to_me(ctx, gear, wearer, source)
}

pub fn modes_chosen_this_turn(_: &Ctx, _: u32) -> Vec<u8> {
    Vec::new()
}

pub fn modes_left(ctx: &Ctx, me: u32) -> Vec<u8> {
    let chosen = modes_chosen_this_turn(ctx, me);
    MODES
        .iter()
        .copied()
        .filter(|mode| !chosen.contains(mode))
        .collect()
}

pub fn ready_two_runes(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let readied = ready_runes(ctx, seat, READY_RUNES);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} readies {readied} runes",
        item.kind.source()
    ));
    done()
}

pub fn channel_one_exhausted(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let channeled = channel_exhausted(ctx, seat, CHANNELED);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} channels {channeled} rune exhausted",
        item.kind.source()
    ));
    done()
}

pub fn buff_a_friendly_unit(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(TargetRef::Card(unit)) = item.targets.first().copied() else {
        return done();
    };
    if !ctx.is_unit(unit) || !ctx.on_board(unit) || ctx.controller(unit) != item.controller {
        return done();
    }
    if buff(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is buffed"));
    }
    done()
}

pub static CARD: Card = unit("Aphelios - Exalted", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, equip, gear, while_attached, with_statics, RAINBOW};
    use crate::cards::{script_of, Grant, Keyword};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const APHELIOS: u32 = 90;
    const GRAVITUM: u32 = 91;
    const TRINKET: u32 = 92;

    static GRAVITUM_CARD: Card = with_statics(
        gear("Gravitum", &[Keyword::Equip(RAINBOW)], &[equip(RAINBOW)]),
        &[while_attached(&[Grant::Might(1)])],
    );

    static TRINKET_CARD: Card = gear("Trinket", &[], &[]);

    fn aphelios(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(APHELIOS, zone, 0, "Aphelios - Exalted", 4)
        }
    }

    fn temple() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(aphelios(fixtures::BASE));
        fixture
            .table
            .cards
            .push(fixtures::gear(GRAVITUM, fixtures::BASE, 0, "Gravitum", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture.table.card_mut(41).unwrap().exhausted = true;
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GRAVITUM, &GRAVITUM_CARD)
            .with_script(TRINKET, &TRINKET_CARD);
        fixture
    }

    fn trigger() -> Item {
        Item::new(
            7,
            ItemKind::Trigger {
                source: APHELIOS,
                index: 0,
            },
            0,
            Origin::Board,
        )
    }

    fn source() -> Source {
        Source {
            card: APHELIOS,
            ability: 0,
        }
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_stub_is_the_pool_name_and_the_attach_trigger_with_its_modes_is_an_engine_seam() {
        assert!(std::ptr::eq(
            script_of("Aphelios - Exalted").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert_eq!(MODES, [READY, CHANNEL, BUFF]);
        assert_eq!(READY_RUNES, 2);
        assert_eq!(CHANNELED, 1);
        let mut fixture = temple();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(APHELIOS).unwrap(), &CARD));
        assert_eq!(
            modes_left(&ctx, APHELIOS),
            MODES,
            "nothing chosen yet · the per-turn memory is the seam"
        );
    }

    #[test]
    fn the_condition_reads_an_equipment_attached_to_him_and_nothing_else() {
        let mut fixture = temple();
        let ctx = fixture.ctx();
        assert!(an_equipment_attached_to_me(
            &ctx,
            GRAVITUM,
            APHELIOS,
            source()
        ));
        assert!(
            !an_equipment_attached_to_me(&ctx, GRAVITUM, fixtures::VI, source()),
            "attached to another unit"
        );
        assert!(
            !an_equipment_attached_to_me(&ctx, TRINKET, APHELIOS, source()),
            "a gear without Equip is not an Equipment"
        );
    }

    #[test]
    fn the_ready_mode_readies_two_exhausted_runes_and_no_more() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "Fury 40 and 41 are exhausted"
        );
        assert_eq!(ready_two_runes(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert_eq!(ctx.ready_runes_of(0).len(), 4);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {APHELIOS}}} · {{seat 0}} readies 2 runes")));
        assert_eq!(ready_two_runes(&mut ctx, &trigger(), Stage(0)), Flow::Done);
        assert_eq!(ctx.ready_runes_of(0).len(), 4, "nothing left to ready");
    }

    #[test]
    fn the_channel_mode_channels_one_rune_exhausted() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        let pool = ctx.table.held(fixtures::RUNE_POOL, 0).count();
        let top = ctx.top_of(fixtures::RUNE_DECK, 0, 1)[0];
        assert_eq!(
            channel_one_exhausted(&mut ctx, &trigger(), Stage(0)),
            Flow::Done
        );
        assert_eq!(ctx.table.held(fixtures::RUNE_POOL, 0).count(), pool + 1);
        assert_eq!(ctx.card(top).unwrap().zone, Some(fixtures::RUNE_POOL));
        assert!(ctx.card(top).unwrap().exhausted, "it arrives exhausted");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {APHELIOS}}} · {{seat 0}} channels 1 rune exhausted"
        )));
    }

    #[test]
    fn the_buff_mode_buffs_its_friendly_target_and_nothing_without_one() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        let mut item = trigger();
        item.targets.push(TargetRef::Card(fixtures::VI));
        item.spec_counts.push(1);
        assert_eq!(buff_a_friendly_unit(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(
            buff_a_friendly_unit(&mut ctx, &trigger(), Stage(0)),
            Flow::Done
        );
        let mut theirs = trigger();
        theirs.targets.push(TargetRef::Card(fixtures::THEIR_UNIT));
        assert_eq!(
            buff_a_friendly_unit(&mut ctx, &theirs, Stage(0)),
            Flow::Done
        );
        assert!(!ctx.is_buffed(fixtures::THEIR_UNIT), "not a friendly unit");
        assert!(!ctx.is_buffed(APHELIOS));
        assert_eq!(UNIT_TO_BUFF.min, 1);
    }

    #[test]
    fn today_equipping_him_queues_nothing() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, GRAVITUM, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {APHELIOS}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, GRAVITUM), Some(APHELIOS));
        assert!(
            ctx.blob.chain.is_empty(),
            "no trigger reaches the chain yet"
        );
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    #[ignore = "engine gap · missing trigger subject and named modes: attach::attach raises no Attached event, Trigger has no Attached(Who::Me), and a modal trigger has no per-turn memory of the modes chosen; with them equipping him offers the three modes, one fewer each time this turn"]
    fn equipping_him_offers_a_mode_not_yet_chosen_this_turn() {
        let mut fixture = temple();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, GRAVITUM, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {APHELIOS}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        let offered = fixtures::labels(&ctx);
        assert_eq!(offered.len(), MODES.len());
        fixtures::choose(&mut ctx, 0, &offered[0]).unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 4);
        assert_eq!(modes_left(&ctx, APHELIOS), [CHANNEL, BUFF]);
    }
}
