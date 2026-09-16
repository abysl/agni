use super::flame_chompers::you_discarded_me_until_a_discarded_event_exists_and_the_trash_is_a_trigger_source;
use super::prelude::{a_friendly_unit, done, might_this_turn, unit, ONE_ENERGY};
use super::{Card, Cost, Flow, Item, Source, Stage, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const EMBOLDEN: Cost = ONE_ENERGY;
pub const MIGHT: i16 = 2;
pub const TARGET: TargetSpec = a_friendly_unit("a friendly unit to give +2 Might this turn");

pub fn you_discarded_me(ctx: &Ctx, discarded: u32, by: u8, source: Source) -> bool {
    you_discarded_me_until_a_discarded_event_exists_and_the_trash_is_a_trigger_source(
        ctx, discarded, by, source,
    )
}

pub fn embolden(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(TargetRef::Card(unit)) = item.targets.first().copied() else {
        ctx.narrate(format!("{{card {me}}} chooses no unit"));
        return done();
    };
    if !ctx.is_unit(unit) || !ctx.on_board(unit) || ctx.controller(unit) != item.controller {
        ctx.narrate(format!(
            "{{card {unit}}} is not a friendly unit on the board · no Might"
        ));
        return done();
    }
    might_this_turn(ctx, item, unit, MIGHT, None);
    ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} this turn"));
    done()
}

pub static CARD: Card = unit("Mask Mother", &[], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const MOTHER: u32 = 90;

    fn mother(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(MOTHER, zone, seat, "Mask Mother", 3)
        }
    }

    fn binned() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mother(fixtures::TRASH, 0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MOTHER).unwrap(),
            &CARD
        ));
        fixture
    }

    fn source() -> Source {
        Source {
            card: MOTHER,
            ability: 0,
        }
    }

    fn trigger(target: Option<u32>) -> Item {
        let mut item = Item::new(
            9,
            ItemKind::Trigger {
                source: MOTHER,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.targets = target.map(TargetRef::Card).into_iter().collect();
        item
    }

    #[test]
    fn the_stub_is_the_pool_name_with_no_keywords_and_the_discard_trigger_is_an_engine_seam() {
        assert!(std::ptr::eq(script_of("Mask Mother").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(EMBOLDEN, ONE_ENERGY);
        assert_eq!(MIGHT, 2);
        assert_eq!(TARGET.filter, crate::cards::prelude::FRIENDLY_UNIT);
        assert_eq!((TARGET.min, TARGET.max), (1, 1));
    }

    #[test]
    fn the_condition_reads_her_owner_discarding_her_into_the_trash_and_nothing_else() {
        let mut fixture = binned();
        let ctx = fixture.ctx();
        assert!(you_discarded_me(&ctx, MOTHER, 0, source()));
        assert!(
            !you_discarded_me(&ctx, fixtures::HAND_UNIT, 0, source()),
            "another card's discard is not hers"
        );
        assert!(
            !you_discarded_me(&ctx, MOTHER, 1, source()),
            "an opponent's discard is not you discarding me"
        );
        drop(ctx);
        fixture.table.card_mut(MOTHER).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(
            !you_discarded_me(&ctx, MOTHER, 0, source()),
            "383.2.c.1 · she must sit in the trash right after the discard"
        );
    }

    #[test]
    fn the_effect_gives_the_chosen_friendly_unit_two_might_this_turn() {
        let mut fixture = binned();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(
            embolden(&mut ctx, &trigger(Some(fixtures::VI)), Stage(0)),
            Flow::Done
        );
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} gets +2 this turn", fixtures::VI)));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_effect_refuses_an_enemy_unit_a_unit_off_the_board_and_no_unit_at_all() {
        let mut fixture = binned();
        let mut ctx = fixture.ctx();
        assert_eq!(
            embolden(&mut ctx, &trigger(Some(fixtures::THEIR_UNIT)), Stage(0)),
            Flow::Done
        );
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert_eq!(
            embolden(&mut ctx, &trigger(Some(fixtures::HAND_UNIT)), Stage(0)),
            Flow::Done
        );
        assert!(ctx.state_of(fixtures::HAND_UNIT).is_none());
        assert_eq!(embolden(&mut ctx, &trigger(None), Stage(0)), Flow::Done);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {MOTHER}}} chooses no unit")));
        assert!(ctx.effects.is_empty());
    }

    #[test]
    #[ignore = "engine gap · no Discarded event and triggers::sources lists in-play cards only (the Flame Chompers off-the-board row); with them the script is optional(with_cost(triggered(Discarded(Who::Me), &[TARGET], embolden), EMBOLDEN)) heard from the hand into the trash"]
    fn discarding_her_offers_one_energy_to_give_a_friendly_unit_two_might() {
        let mut fixture = binned();
        fixture.table.card_mut(MOTHER).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.trash(MOTHER);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::Target { .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(crate::state::PromptWhy::OptionalCost { cost: 1, .. })
        ));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
    }
}
