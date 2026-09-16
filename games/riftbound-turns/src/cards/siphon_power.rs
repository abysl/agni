use super::prelude::{a_battlefield, done, might_this_turn, play, spell, zone_target, Location};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const FRIENDLY_MIGHT: i16 = 1;
pub const ENEMY_MIGHT: i16 = -1;
pub const FLOOR: i32 = 1;

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(zone) = zone_target(item, 0) else {
        return done();
    };
    if !ctx.zones.is_battlefield(zone) {
        return done();
    }
    let seat = item.controller;
    for unit in ctx.units_at(Location::Battlefield(zone)) {
        if ctx.controller(unit) == seat {
            might_this_turn(ctx, item, unit, FRIENDLY_MIGHT, None);
            ctx.narrate(format!(
                "{{card {unit}}} gets +{FRIENDLY_MIGHT} might this turn"
            ));
        } else {
            let before = ctx.current_might(unit);
            might_this_turn(ctx, item, unit, ENEMY_MIGHT, Some(FLOOR));
            let after = ctx.current_might(unit);
            ctx.narrate(format!(
                "{{card {unit}}} gets {} might this turn",
                after - before
            ));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Siphon Power",
    &[Keyword::Reaction],
    &[play(&[a_battlefield("a battlefield")], resolve)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Filter, TargetKind, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts};
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const SIPHON: u32 = 90;
    const THEIR_SIPHON: u32 = 91;
    const ALLY: u32 = 92;
    const RECRUIT: u32 = 93;
    const ORDER_RUNE: u32 = 46;
    const THEIR_MIND: u32 = 47;

    fn siphon(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Siphon Power", 2, 1);
        card.domain = vec!["Mind".into(), "Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(siphon(SIPHON, 0));
        fixture.table.cards.push(siphon(THEIR_SIPHON, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(THEIR_MIND, 1, "Mind", false));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture.table.cards.push(fixtures::unit(
            ALLY,
            fixtures::BF2,
            0,
            "Legion Rearguard",
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(RECRUIT, fixtures::BF2, 1, "Recruit", 1));
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_reaction_that_chooses_one_battlefield() {
        assert!(std::ptr::eq(script_of("Siphon Power").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let spec = CARD.abilities[0].targets[0];
        assert_eq!(spec.kind, TargetKind::Zone);
        assert_eq!(spec.filter, Filter::AtBattlefield);
        assert_eq!((spec.min, spec.max), (1, 1));
    }

    #[test]
    fn friendly_units_there_gain_one_and_enemy_units_lose_one_to_a_floor_of_one_for_the_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SIPHON).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "355.10.b · the battlefields in play are the targets, the units are not"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a battlefield (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Zone(fixtures::BF2)]);
        assert_eq!(
            ctx.card(ORDER_RUNE).unwrap().zone,
            Some(fixtures::RUNE_DECK),
            "one power of either domain, paid with the Order rune"
        );
        assert_eq!(
            might_counter(&ctx, fixtures::VI),
            0,
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 4, "3 + 1");
        assert_eq!(ctx.current_might(ALLY), 2, "1 + 1");
        assert_eq!(ctx.current_might(fixtures::SPRITE), 2, "3 - 1");
        assert_eq!(
            ctx.current_might(RECRUIT),
            1,
            "477.3.b · a 1-Might enemy is floored at 1 and the -1 snapshots to 0"
        );
        assert_eq!(might_counter(&ctx, RECRUIT), 0);
        assert!(
            ctx.state_of(RECRUIT).is_none_or(|row| row.might.is_empty()),
            "a zero delta leaves no modifier behind"
        );
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            2,
            "an enemy unit elsewhere is untouched"
        );
        assert_eq!(
            ctx.state_of(fixtures::SPRITE).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +1 might this turn".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} gets -1 might this turn".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 93} gets 0 might this turn".to_string()));
        assert_eq!(ctx.card(SIPHON).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_reacts_on_the_same_battlefield_and_the_sides_flip() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        fixtures::play_from_hand(&mut ctx, 1, THEIR_SIPHON).unwrap();
        fixtures::choose(&mut ctx, 1, "{zone 10}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            4,
            "seat 1's units gain"
        );
        assert_eq!(ctx.current_might(RECRUIT), 2);
        assert_eq!(ctx.current_might(fixtures::VI), 2, "seat 0's units lose");
        assert_eq!(ctx.current_might(ALLY), 1, "floored at 1");
        assert_eq!(ctx.card(THEIR_SIPHON).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn an_empty_battlefield_resolves_to_nothing_and_a_base_or_a_unit_is_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SIPHON).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[u32::from(fixtures::BASE)]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a base is not a battlefield"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit is not a zone"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.blob
                .log
                .iter()
                .any(|line| line.contains("might this turn")),
            "nobody was there"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(SIPHON).unwrap().zone, Some(fixtures::TRASH));
    }
}
