use super::prelude::{done, draw, unit};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::Ctx;

pub const DRAWS: usize = 1;

pub fn an_equipment_you_attached_to_me(ctx: &Ctx, gear: u32, wearer: u32, source: Source) -> bool {
    wearer == source.card
        && ctx.is_gear(gear)
        && ctx.on_board(gear)
        && ctx.controller(gear) == ctx.controller(source.card)
        && ctx.script(gear).is_some_and(|script| script.is_equipment())
}

pub fn study_the_weapon(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} draws {drawn}",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = unit("Jax - Unrelenting", &[Keyword::Weaponmaster], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::WEAPONMASTER;
    use crate::cards::prelude::{attached_to, equip, gear, while_attached, with_statics, RAINBOW};
    use crate::cards::{script_of, Grant, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, Origin, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const JAX: u32 = 90;
    const LAMP: u32 = 91;
    const THEIR_LAMP: u32 = 92;
    const TRINKET: u32 = 93;

    static LAMP_CARD: Card = with_statics(
        gear("Lamppost", &[Keyword::Equip(RAINBOW)], &[equip(RAINBOW)]),
        &[while_attached(&[Grant::Might(1)])],
    );

    static TRINKET_CARD: Card = gear("Trinket", &[], &[]);

    fn jax(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Body".into()],
            ..fixtures::unit(JAX, zone, 0, "Jax - Unrelenting", 3)
        }
    }

    fn armory(jax_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jax(jax_zone));
        fixture
            .table
            .cards
            .push(fixtures::gear(LAMP, fixtures::BASE, 0, "Lamppost", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_LAMP, fixtures::BASE, 1, "Lamppost", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LAMP, &LAMP_CARD)
            .with_script(THEIR_LAMP, &LAMP_CARD)
            .with_script(TRINKET, &TRINKET_CARD);
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn source() -> Source {
        Source {
            card: JAX,
            ability: 1,
        }
    }

    #[test]
    fn the_script_is_weaponmaster_alone_and_the_attach_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Jax - Unrelenting").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Weaponmaster]);
        assert!(
            CARD.abilities.is_empty(),
            "821 · Weaponmaster is the engine's"
        );
        assert_eq!(WEAPONMASTER.trigger, Trigger::Play);
        assert!(WEAPONMASTER.optional);
        assert_eq!(WEAPONMASTER.label, Some("weaponmaster"));
        assert_eq!(DRAWS, 1);
        let fixture = armory(fixtures::HAND);
        assert!(std::ptr::eq(fixture.scripts.of_card(JAX).unwrap(), &CARD));
    }

    #[test]
    fn the_condition_reads_an_equipment_of_yours_attached_to_him_and_nothing_else() {
        let mut fixture = armory(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(an_equipment_you_attached_to_me(&ctx, LAMP, JAX, source()));
        assert!(
            !an_equipment_you_attached_to_me(&ctx, LAMP, fixtures::VI, source()),
            "attached to another unit"
        );
        assert!(
            !an_equipment_you_attached_to_me(&ctx, TRINKET, JAX, source()),
            "a gear without Equip is not an Equipment"
        );
        assert!(
            !an_equipment_you_attached_to_me(&ctx, THEIR_LAMP, JAX, source()),
            "the opponent's Equipment is not one you attach"
        );
        assert!(!an_equipment_you_attached_to_me(
            &ctx,
            fixtures::HAND_GEAR,
            JAX,
            source()
        ));
    }

    #[test]
    fn the_effect_draws_one_for_his_controller() {
        let mut fixture = armory(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let item = Item::new(
            7,
            ItemKind::Trigger {
                source: JAX,
                index: 1,
            },
            0,
            Origin::Board,
        );
        assert_eq!(study_the_weapon(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
        assert_eq!(ctx.hand_of(1).len(), 1);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {JAX}}} · {{seat 0}} draws 1")));
    }

    #[test]
    fn playing_him_offers_weaponmaster_over_your_equipment_and_attaches_it_for_the_rainbow_less() {
        let mut fixture = armory(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let runes = ctx.runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, JAX).unwrap();
        assert!(ctx.on_board(JAX));
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {LAMP}}}"), "skip".to_string()],
            "821.1.c · your Equipment, not the Trinket nor theirs"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {LAMP}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, LAMP), Some(JAX));
        assert_eq!(ctx.current_might(JAX), 4);
        assert_eq!(
            ctx.runes_of(0).len(),
            runes,
            "821.1.c.3 · the rainbow is struck, nothing is paid"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn today_equipping_him_queues_no_draw() {
        let mut fixture = armory(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, LAMP, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {JAX}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, LAMP), Some(JAX));
        assert!(
            ctx.blob.chain.is_empty(),
            "no trigger reaches the chain yet"
        );
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    #[ignore = "engine gap · missing trigger subject: attach::attach raises no Attached event and Trigger has no Attached(Who::Me); with it the script adds optional(with_cost(triggered(Attached(Me), &[], study_the_weapon), ONE_ENERGY)) and equipping him asks for one energy to draw one"]
    fn equipping_him_asks_for_one_energy_and_pays_it_to_draw_one() {
        let mut fixture = armory(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, LAMP, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {JAX}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost: 1, .. })
        ));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + DRAWS);
    }
}
