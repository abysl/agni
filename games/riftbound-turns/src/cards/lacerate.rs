use super::prelude::{a_unit, card_target, disempower, done, is_empowered, kill, play, spell};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::{Ctx, Killed};

pub const MIGHT_LIMIT: i32 = 3;
pub const FLOW: Cost = Cost {
    energy: 4,
    power: &[Power::Domain(Domain::Order), Power::Domain(Domain::Order)],
};

pub fn small_enough_to_die(ctx: &Ctx, unit: u32) -> bool {
    ctx.current_might(unit) <= MIGHT_LIMIT
}

fn lacerate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if is_empowered(ctx, unit) && disempower(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is disempowered"));
    }
    if !small_enough_to_die(ctx, unit) {
        ctx.narrate(format!(
            "{{card {unit}}} has more than {MIGHT_LIMIT} Might and survives"
        ));
        return done();
    }
    if kill(ctx, item, unit) == Killed::Yes {
        ctx.narrate(format!("{{card {unit}}} dies"));
    }
    done()
}

pub static CARD: Card = spell(
    "Lacerate",
    &[Keyword::Flow(FLOW)],
    &[play(
        &[a_unit("a unit to disempower, then kill at 3 Might or less")],
        lacerate,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{might_this_turn, unit as unit_card, with_statics, UNIT};
    use crate::cards::{script_of, Grant, Static, Trigger};
    use crate::engine::ctx::{EntryMove, Event, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{Leave, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const LACERATE: u32 = 90;
    const THEIR_LACERATE: u32 = 91;
    const BRUTE: u32 = 92;
    const ZEALOT: u32 = 93;
    const ORDER_RUNES: [u32; 2] = [100, 101];

    fn empowered_might(ctx: &Ctx, unit: u32) -> bool {
        ctx.is_empowered(unit)
    }

    static ZEALOT_CARD: Card = with_statics(
        unit_card("Zealot", &[], &[]),
        &[Static::While(empowered_might, &[Grant::Might(2)])],
    );

    fn lacerate_card(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Lacerate", 2, 1);
        card.domain = vec!["Order".into()];
        card
    }

    fn arena(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(lacerate_card(LACERATE, zone, 0));
        fixture
            .table
            .cards
            .push(lacerate_card(THEIR_LACERATE, fixtures::HAND, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(ZEALOT, fixtures::BF1, 1, "Zealot", 2));
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(ZEALOT),
            counter: COUNTER_EMPOWERED,
            value: 1,
        });
        fixture.table.counters.sort();
        for rune in ORDER_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ZEALOT, &ZEALOT_CARD);
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, LACERATE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_is_a_flow_sorcery_over_any_unit_with_a_three_might_line() {
        assert!(std::ptr::eq(script_of("Lacerate").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!(MIGHT_LIMIT, 3);
        let mut fixture = arena(fixtures::HAND);
        let ctx = fixture.ctx();
        assert!(small_enough_to_die(&ctx, fixtures::VI));
        assert!(!small_enough_to_die(&ctx, BRUTE));
        assert_eq!(ctx.current_might(ZEALOT), 4, "2 + 2 while Empowered");
        assert!(!small_enough_to_die(&ctx, ZEALOT));
    }

    #[test]
    fn an_empowered_unit_is_disempowered_first_and_dies_on_what_is_left() {
        let mut fixture = arena(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert!(ctx.is_empowered(ZEALOT));
        cast_at(&mut ctx, ZEALOT);
        assert!(ctx.events.contains(&Event::Disempowered { card: ZEALOT }));
        assert!(!ctx.is_empowered(ZEALOT));
        assert_eq!(
            ctx.card(ZEALOT).unwrap().zone,
            Some(fixtures::TRASH),
            "disempowered it reads 2, which is 3 or less"
        );
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { card, .. } if *card == ZEALOT)));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ZEALOT}}} is disempowered")));
        assert!(ctx.blob.log.contains(&format!("{{card {ZEALOT}}} dies")));
        assert_eq!(ctx.card(LACERATE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_plain_unit_of_four_survives_and_a_pump_in_response_saves_a_small_one() {
        let mut fixture = arena(fixtures::HAND);
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        assert!(ctx.on_board(BRUTE));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Disempowered { .. })));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BRUTE}}} has more than 3 Might and survives"
        )));
        drop(ctx);

        let mut pumped = arena(fixtures::HAND);
        let mut ctx = pumped.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LACERATE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let item = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &item, fixtures::THEIR_UNIT, 2, None);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "Jinx reads 4 as the spell resolves"
        );
    }

    #[test]
    fn from_the_trash_the_flow_play_kills_a_small_unit_and_is_banished() {
        let mut fixture = arena(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            LACERATE,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.banished_of(0), [LACERATE]);
        assert!(
            !ctx.trash_of(0).contains(&LACERATE),
            "the spell is banished, Jinx is in the other trash"
        );
    }

    #[test]
    fn a_gear_is_refused_a_unit_gone_before_resolution_is_left_alone_and_the_other_seat_waits() {
        let mut fixture = arena(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_LACERATE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, LACERATE).unwrap();
        for wrong in [fixtures::GROUNDS, fixtures::HAND_UNIT, fixtures::HAND_GEAR] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::HAND, 1),
                1,
            )
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::HAND)
        );
        assert!(!ctx.blob.log.iter().any(|line| line.ends_with(" dies")));
        assert_eq!(ctx.card(LACERATE).unwrap().zone, Some(fixtures::TRASH));
    }
}
