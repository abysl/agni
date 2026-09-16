use super::prelude::{a_card, a_friendly_unit, card_target, deal, done, play, spell};
use super::{Card, Cost, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const REPEAT: Cost = Cost {
    energy: 3,
    power: &[],
};
pub const ENEMY_UNIT_AT_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::AtBattlefield]);

fn might_as_damage(ctx: &Ctx, unit: u32) -> u8 {
    u8::try_from(ctx.current_might(unit).max(0)).unwrap_or(u8::MAX)
}

fn march(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let mine = card_target(ctx, item, 0).filter(|unit| ctx.on_board(*unit));
    let theirs = card_target(ctx, item, 1).filter(|unit| ctx.on_board(*unit));
    let (Some(mine), Some(theirs)) = (mine, theirs) else {
        return done();
    };
    let to_theirs = might_as_damage(ctx, mine);
    let to_mine = might_as_damage(ctx, theirs);
    ctx.narrate(format!(
        "{{card {mine}}} and {{card {theirs}}} deal {to_theirs} and {to_mine} to each other"
    ));
    deal(ctx, item, theirs, to_theirs);
    deal(ctx, item, mine, to_mine);
    done()
}

pub static CARD: Card = spell(
    "Marching Orders",
    &[Keyword::Action, Keyword::Repeat(REPEAT)],
    &[play(
        &[
            a_friendly_unit("a friendly unit anywhere"),
            a_card(ENEMY_UNIT_AT_BATTLEFIELD, "an enemy unit at a battlefield"),
        ],
        march,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::{Expiry, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const ORDERS: u32 = 90;
    const THEIR_ORDERS: u32 = 91;
    const BRUTE: u32 = 92;
    const ALLY: u32 = 93;
    const MY_EXTRA: [u32; 3] = [46, 47, 48];

    fn orders(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Marching Orders", 3, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(orders(ORDERS, 0));
        fixture.table.cards.push(orders(THEIR_ORDERS, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Vanguard", 6));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
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

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt {
                    card,
                    n,
                    source: Cause::Item(1),
                } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_a_repeatable_action_choosing_a_friendly_unit_then_an_enemy_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Marching Orders").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[1].filter, ENEMY_UNIT_AT_BATTLEFIELD);
    }

    #[test]
    fn the_friendly_unit_may_sit_in_the_base_and_the_two_deal_their_mights_to_each_other() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        ctx.might(fixtures::VI, 2, Expiry::EndOfTurn(1), None, 7);
        fixtures::play_from_hand(&mut ctx, 0, ORDERS).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 93}", "cancel"],
            "a friendly unit anywhere"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "an enemy at a battlefield: Jinx in her base is not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(damage_events(&ctx).is_empty(), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            damage_events(&ctx),
            [(BRUTE, 5), (fixtures::VI, 4)],
            "each takes the other's current Might"
        );
        assert!(!ctx.on_board(BRUTE), "five kills the 4-Might Brute");
        assert!(ctx.on_board(fixtures::VI));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} and {card 92} deal 5 and 4 to each other".to_string()));
        assert_eq!(ctx.card(ORDERS).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_it_orders_two_duels_with_their_own_pairs() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ORDERS).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 2 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 3 }));
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(item.spec_counts, [1, 1, 1, 1]);
        assert_eq!(ctx.ready_runes_of(0).len(), 0, "three energy twice");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            damage_events(&ctx),
            [
                (BRUTE, 6),
                (ALLY, 4),
                (fixtures::SPRITE, 3),
                (fixtures::VI, 3)
            ]
        );
        assert!(!ctx.on_board(BRUTE));
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(!ctx.on_board(fixtures::VI), "3 damage kills a 3-Might Vi");
        assert!(ctx.on_board(ALLY));
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_in_its_base_is_refused_and_a_duel_with_one_side_gone_deals_nothing() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_ORDERS)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, ORDERS).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[BRUTE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy is not friendly"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Jinx sits in her base"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        ctx.bounce(BRUTE);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            damage_events(&ctx).is_empty(),
            "359.3.e.7 · a duel with one side gone does not execute"
        );
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert_eq!(ctx.card(ORDERS).unwrap().zone, Some(fixtures::TRASH));
    }
}
