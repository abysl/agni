use super::prelude::{done, draw, friendly_gear, play, unit, when, with_statics};
use super::{Card, Flow, Item, Source, Stage, Static};
use crate::engine::ctx::{Ctx, Event};

pub const OTHER_GEAR: usize = 3;
pub const CARDS: usize = 1;

pub fn other_gear_of(ctx: &Ctx, seat: u8, me: u32) -> usize {
    friendly_gear(ctx, seat)
        .into_iter()
        .filter(|gear| *gear != me)
        .count()
}

fn three_other_gear(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Played {
        card, controller, ..
    } = event
    else {
        return false;
    };
    *card == source.card && other_gear_of(ctx, *controller, source.card) >= OTHER_GEAR
}

fn beep(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, CARDS);
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Patched Porobot",
        &[],
        &[when(play(&[], beep), three_other_gear)],
    ),
    &[Static::EntersExhausted],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ItemKind, Origin};
    use agni_plugin_sdk::table::CardInfo;

    const POROBOT: u32 = 90;
    const GEAR: [u32; 3] = [91, 92, 93];
    const THEIR_GEAR: u32 = 94;
    const GOLD: u32 = 95;

    fn porobot(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Mind".into()],
            ..fixtures::unit(POROBOT, zone, 0, "Patched Porobot", 2)
        }
    }

    fn workshop(gear: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(porobot(fixtures::HAND));
        for id in GEAR.iter().take(gear) {
            fixture
                .table
                .cards
                .push(fixtures::gear(*id, fixtures::BASE, 0, "Trinket", 1));
        }
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Loot", 1));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_enters_exhausted_and_gates_its_draw_on_three_other_gear() {
        assert!(std::ptr::eq(script_of("Patched Porobot").unwrap(), &CARD));
        assert_eq!(CARD.name, "Patched Porobot");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.has_static(Static::EntersExhausted));
        assert_eq!(CARD.statics.len(), 1);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.condition.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!(OTHER_GEAR, 3);
        assert_eq!(CARDS, 1);
    }

    #[test]
    fn the_count_reads_gear_you_control_golds_included_and_never_the_enemys() {
        let mut fixture = workshop(2);
        fixture.table.cards.push(fixtures::gold(GOLD, 0, false));
        fixture.table.tokens.push(GOLD);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            other_gear_of(&ctx, 0, POROBOT),
            3,
            "two Trinkets and a Gold"
        );
        assert_eq!(
            other_gear_of(&ctx, 1, POROBOT),
            1,
            "the enemy's Loot is theirs"
        );
        let played = |controller: u8| Event::Played {
            card: POROBOT,
            controller,
            kind: crate::cards::KIND_UNIT.to_string(),
            origin: Origin::Hand,
            paid_additional: false,
        };
        let source = Source {
            card: POROBOT,
            ability: 0,
        };
        assert!(three_other_gear(&ctx, &played(0), source));
        assert!(!three_other_gear(&ctx, &played(1), source));
        let other = Event::Played {
            card: fixtures::VI,
            controller: 0,
            kind: crate::cards::KIND_UNIT.to_string(),
            origin: Origin::Hand,
            paid_additional: false,
        };
        assert!(!three_other_gear(&ctx, &other, source), "its own play only");
    }

    #[test]
    fn with_three_other_gear_it_enters_exhausted_and_draws_one() {
        let mut fixture = workshop(3);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, POROBOT).unwrap();
        assert_eq!(ctx.location(POROBOT), Some(Location::Base(0)));
        assert!(ctx.card(POROBOT).unwrap().exhausted);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {POROBOT}}} enters exhausted")));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == POROBOT
        ));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_two_other_gear_the_trigger_never_fires() {
        let mut fixture = workshop(2);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, POROBOT).unwrap();
        assert_eq!(ctx.location(POROBOT), Some(Location::Base(0)));
        assert!(ctx.card(POROBOT).unwrap().exhausted);
        assert!(
            ctx.blob.chain.is_empty(),
            "383.2.a.1 · the if is the condition"
        );
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
    }
}
