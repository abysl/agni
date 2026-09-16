use super::prelude::{card_target, done, kill, play, spawn_gold, spell, target};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::{Ctx, Killed};

pub const MIGHT_LIMIT: u8 = 2;
pub const GOLD_FOR_AN_ENEMY: usize = 1;
pub const GOLD_FOR_A_FRIEND: usize = 2;
pub const GOLD_ARRIVES_READY: bool = false;

pub const VICTIM: TargetSpec = target(
    Filter::And(&[
        Filter::Unit,
        Filter::AtBattlefield,
        Filter::MightAtMost(MIGHT_LIMIT),
    ]),
    1,
    1,
    TargetKind::Card,
    "a unit at a battlefield with 2 Might or less",
);

pub fn bounty_for(ctx: &Ctx, seat: u8, unit: u32) -> usize {
    if ctx.controller(unit) == seat {
        GOLD_FOR_A_FRIEND
    } else {
        GOLD_FOR_AN_ENEMY
    }
}

fn collect(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let seat = item.controller;
    let bounty = bounty_for(ctx, seat, unit);
    if kill(ctx, item, unit) != Killed::Yes {
        return done();
    }
    ctx.narrate(format!("{{card {unit}}} dies"));
    for _ in 0..bounty {
        spawn_gold(ctx, seat, GOLD_ARRIVES_READY);
    }
    done()
}

pub static CARD: Card = spell(
    "Blood Money",
    &[Keyword::Action],
    &[play(&[VICTIM], collect)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{replaces, unit, with_replacement, Location};
    use crate::cards::{script_of, Trigger, TOKEN_GOLD};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const MONEY: u32 = 90;
    const THEIR_MONEY: u32 = 91;
    const SCOUT: u32 = 92;
    const ORDER_A: u32 = 100;
    const ORDER_B: u32 = 101;
    const FIRST_GOLD: u32 = 200;

    static PHOENIX: Card = with_replacement(
        unit("Phoenix", &[], &[]),
        replaces(
            |_, _, _| true,
            |ctx, would, _| {
                ctx.recall(would.unit, true);
            },
        ),
    );

    fn money(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Blood Money", 2, 2);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(money(MONEY, 0));
        fixture.table.cards.push(money(THEIR_MONEY, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_A, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_B, 0, "Order", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
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

    fn golds_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && ctx.controller(card.id) == seat)
            .map(|card| card.id)
            .collect()
    }

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_is_an_action_over_a_small_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Blood Money").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, [VICTIM]);
        assert_eq!((VICTIM.min, VICTIM.max), (1, 1));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(bounty_for(&ctx, 0, fixtures::THEIR_UNIT), 1);
        assert_eq!(bounty_for(&ctx, 0, SCOUT), 2);
    }

    #[test]
    fn an_enemy_unit_dies_for_one_exhausted_gold() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONEY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "{card 92}", "cancel"],
            "Vi has 3 Might, the Sprite is at a battlefield with 3"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(
            golds_of(&ctx, 0).is_empty(),
            "nothing happens before it resolves"
        );
        both_pass(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, unit: true, .. } if *card == fixtures::THEIR_UNIT
        )));
        let golds = golds_of(&ctx, 0);
        assert_eq!(golds, [FIRST_GOLD]);
        let gold = ctx.card(FIRST_GOLD).unwrap();
        assert_eq!(gold.zone, Some(fixtures::BASE));
        assert_eq!(gold.owner, 0);
        assert!(gold.exhausted, "played exhausted");
        assert!(ctx.is_token(FIRST_GOLD));
        assert!(ctx.blob.log.contains(&"{card 81} dies".to_string()));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(MONEY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_friendly_unit_dies_for_two_exhausted_golds() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONEY).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::TRASH));
        let golds = golds_of(&ctx, 0);
        assert_eq!(golds, [FIRST_GOLD, FIRST_GOLD + 1]);
        assert!(golds.iter().all(|gold| ctx.card(*gold).unwrap().exhausted));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| *line == "{seat 0} gains a Gold")
                .count(),
            2
        );
    }

    #[test]
    fn a_kill_that_is_replaced_pays_nothing_and_a_unit_that_grew_is_left_alone() {
        let mut fixture = armed();
        fixture.scripts = fixture.scripts.clone().with_script(SCOUT, &PHOENIX);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONEY).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(
            ctx.location(SCOUT),
            Some(Location::Base(0)),
            "the replacement recalled it instead"
        );
        assert!(
            golds_of(&ctx, 0).is_empty(),
            "it was not killed, so no bounty"
        );
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONEY).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let spell = ctx.blob.chain[0].clone();
        crate::cards::prelude::might_this_turn(&mut ctx, &spell, fixtures::THEIR_UNIT, 1, None);
        both_pass(&mut ctx);
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "356.3.e · at 3 Might it no longer matches"
        );
        assert!(golds_of(&ctx, 0).is_empty());
    }

    #[test]
    fn units_in_a_base_or_above_two_might_are_refused_and_the_action_waits_for_its_turn() {
        let mut fixture = armed();
        fixture.table.card_mut(SCOUT).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_MONEY)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, MONEY).unwrap();
        for wrong in [SCOUT, fixtures::VI, fixtures::SPRITE, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base, too mighty or not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(MONEY).unwrap().zone, Some(fixtures::HAND));
    }
}
