use super::prelude::{done, legion, play, ready, unit, when, with_statics};
use super::{Card, Flow, Grant, Item, Keyword, Scope, Source, Stage, Static};
use crate::engine::ctx::{Ctx, Event};

pub const BONUS: i16 = 1;

pub fn legion_on_entry(ctx: &Ctx, _: &Event, source: Source) -> bool {
    legion(ctx, ctx.controller(source.card))
}

fn execute(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    done()
}

fn another_friendly_unit(ctx: &Ctx, source: u32, unit: u32) -> bool {
    unit != source && ctx.controller(unit) == ctx.controller(source)
}

pub static CARD: Card = with_statics(
    unit(
        "Darius - Executioner",
        &[Keyword::Legion],
        &[when(play(&[], execute), legion_on_entry)],
    ),
    &[Static::Aura {
        scope: Scope::UnitsHere,
        when: another_friendly_unit,
        grants: &[Grant::Might(BONUS)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::{Event, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use crate::state::ItemKind;

    const DARIUS: u32 = 90;
    const RAIDER: u32 = 91;
    const SPARE_RUNES: [u32; 3] = [46, 47, 48];

    fn darius(zone: u16) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::unit(DARIUS, zone, 0, "Darius - Executioner", 6);
        card.domain = vec!["Order".into()];
        card.energy = Some(6);
        card.power = Some(1);
        card
    }

    fn arena(legion_on: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(darius(fixtures::HAND));
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.seat_mut(0).played_main = legion_on;
        fixture.resolve();
        fixture
    }

    fn fielded() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(darius(fixtures::BF1));
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DARIUS).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_legion_unit_with_a_gated_play_trigger_and_an_aura_over_units_here() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Darius - Executioner").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Legion]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_some(), "812.1.b.1 · the Legion gate");
        assert!(CARD.has_aura());
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(
            CARD.statics[0],
            Static::Aura {
                scope: Scope::UnitsHere,
                grants: [Grant::Might(1)],
                ..
            }
        ));
    }

    #[test]
    fn with_legion_on_he_enters_exhausted_and_his_trigger_readies_him() {
        let mut fixture = arena(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DARIUS).unwrap();
        assert!(ctx.on_board(DARIUS));
        assert!(
            ctx.card(DARIUS).unwrap().exhausted,
            "a played unit enters exhausted"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DARIUS
        ));
        assert!(ctx.blob.prompt.is_none());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(DARIUS).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, .. } if *card == DARIUS
        )));
        assert!(ctx.blob.log.contains(&format!("{{card {DARIUS}}} readies")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn as_the_first_card_of_the_turn_he_stays_exhausted() {
        let mut fixture = arena(false);
        let mut ctx = fixture.ctx();
        assert!(!legion(&ctx, 0));
        fixtures::play_from_hand(&mut ctx, 0, DARIUS).unwrap();
        assert!(ctx.on_board(DARIUS));
        assert!(ctx.blob.chain.is_empty(), "no Legion, no trigger");
        assert!(ctx.card(DARIUS).unwrap().exhausted);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { card, .. } if *card == DARIUS)));
        drop(ctx);

        let mut fixture = arena(false);
        fixture.blob.seat_mut(1).played_main = true;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DARIUS).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "the opponent's play is not my Legion"
        );
        assert!(ctx.card(DARIUS).unwrap().exhausted);
    }

    #[test]
    fn a_darius_readied_in_response_readies_nothing_again_when_the_trigger_resolves() {
        let mut fixture = arena(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DARIUS).unwrap();
        assert!(ctx.ready(DARIUS));
        let readied = ctx
            .events
            .iter()
            .filter(|event| matches!(event, Event::Readied { card, .. } if *card == DARIUS))
            .count();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(DARIUS).unwrap().exhausted);
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Readied { card, .. } if *card == DARIUS))
                .count(),
            readied,
            "already ready: nothing to ready"
        );
        assert!(!ctx.blob.log.contains(&format!("{{card {DARIUS}}} readies")));
    }

    #[test]
    fn other_friendly_units_at_his_location_read_plus_one_and_enemies_and_darius_do_not() {
        let mut fixture = fielded();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(DARIUS), 6, "never himself");
        assert_eq!(ctx.current_might(RAIDER), 2, "never an enemy");
        assert_eq!(ctx.current_might(fixtures::VI), 3, "Vi in base is not here");
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(ctx.current_might(fixtures::VI), 4, "here now: +1");
        assert!(matches!(
            statics::grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::Might(1)]
        ));
        assert!(statics::grants_on(&ctx, DARIUS).is_empty());
        assert!(statics::grants_on(&ctx, RAIDER).is_empty());
        assert_eq!(
            ctx.move_unit(DARIUS, Location::Base(0), MoveCause::Effect),
            Moved::Moved
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "he left the battlefield: the bonus goes with him"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_aura_reaches_friendly_units_in_base_and_lapses_when_he_leaves_play() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(darius(fixtures::BASE));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "198.1 · a base is a location, so it is here too"
        );
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert!(ctx.bounce(DARIUS));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(statics::grants_on(&ctx, fixtures::VI).is_empty());
    }
}
