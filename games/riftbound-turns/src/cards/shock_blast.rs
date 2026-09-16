use super::prelude::{
    a_unit_at_a_battlefield, card_target, deal, done, is_empowered, play, spell, with_statics,
};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Static};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 4;
pub const DISCOUNT: Cost = Cost {
    energy: 2,
    power: &[],
};

pub fn controls_something_empowered(ctx: &Ctx, seat: u8) -> bool {
    ctx.faces_on_board()
        .filter(|held| ctx.controller(held.id) == seat)
        .any(|held| is_empowered(ctx, held.id))
}

fn discount(ctx: &Ctx, _: u32, seat: u8) -> Cost {
    if controls_something_empowered(ctx, seat) {
        DISCOUNT
    } else {
        Cost::FREE
    }
}

fn blast(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static CARD: Card = with_statics(
    spell(
        "Shock Blast",
        &[Keyword::Action],
        &[play(
            &[a_unit_at_a_battlefield("a unit at a battlefield")],
            blast,
        )],
    ),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, EntryMove, Event, COUNTER_EMPOWERED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{cost, play as play_engine};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const BLAST: u32 = 90;
    const MY_GEAR: u32 = 91;
    const BRUTE: u32 = 92;
    const MIND_RUNE: u32 = 46;
    const PRINTED: u8 = 3;

    fn blast_card() -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::spell(BLAST, fixtures::HAND, 0, "Shock Blast", PRINTED, 1)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(blast_card());
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(BLAST).unwrap(), &CARD));
        fixture
    }

    fn empowered(mut fixture: Fixture, card: u32) -> Fixture {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(card),
            counter: COUNTER_EMPOWERED,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.resolve();
        fixture
    }

    fn item(seat: u8) -> ChainItem {
        ChainItem::new(1, ItemKind::Spell { card: BLAST }, seat, Origin::Hand)
    }

    fn entry(ctx: &Ctx) -> EntryMove {
        EntryMove {
            card: BLAST,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_an_action_with_a_conditional_self_discount_over_a_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Shock Blast").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert_eq!(DISCOUNT.energy, 2);
        assert!(DISCOUNT.power.is_empty());
        assert_eq!(DAMAGE, 4);
    }

    #[test]
    fn it_costs_the_printed_three_with_nothing_empowered_and_one_beside_an_empowered_unit_or_gear()
    {
        let mut plain = armed();
        let ctx = plain.ctx();
        assert!(!controls_something_empowered(&ctx, 0));
        assert_eq!(discount(&ctx, BLAST, 0), Cost::FREE);
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, PRINTED);
        assert_eq!(cost::total(&ctx, BLAST, false).energy, PRINTED);
        drop(ctx);
        for empowered_card in [fixtures::VI, MY_GEAR] {
            let mut fixture = empowered(armed(), empowered_card);
            let ctx = fixture.ctx();
            assert!(controls_something_empowered(&ctx, 0));
            assert_eq!(discount(&ctx, BLAST, 0), DISCOUNT);
            assert_eq!(
                cost::of_item(&ctx, &item(0), None).energy,
                PRINTED - 2,
                "two off beside {empowered_card}"
            );
            assert_eq!(cost::total(&ctx, BLAST, false).energy, PRINTED - 2);
            assert_eq!(
                cost::of_item(&ctx, &item(0), None).power.len(),
                1,
                "the Mind power stays"
            );
        }
        let mut theirs = empowered(armed(), fixtures::THEIR_UNIT);
        let ctx = theirs.ctx();
        assert!(controls_something_empowered(&ctx, 1));
        assert!(
            !controls_something_empowered(&ctx, 0),
            "an opponent's Empowered unit is not something you control"
        );
        assert_eq!(cost::of_item(&ctx, &item(0), None).energy, PRINTED);
    }

    #[test]
    fn played_for_one_beside_an_empowered_unit_it_deals_four_to_a_unit_at_a_battlefield() {
        let mut fixture = empowered(armed(), fixtures::VI);
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "one ready Mind rune");
        fixtures::play_from_hand(&mut ctx, 0, BLAST).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "units at battlefields only"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "the one rune pays one energy and recycles for the Mind"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(BRUTE), "four kills the 4-Might Brute");
        assert!(ctx.blob.log.contains(&"{card 92} takes 4".to_string()));
        assert_eq!(ctx.card(BLAST).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn at_full_price_one_ready_rune_is_refused_and_a_unit_in_a_base_is_not_a_target() {
        let mut broke = armed();
        for id in [41, 42, 43] {
            broke.table.card_mut(id).unwrap().exhausted = true;
        }
        let ctx = broke.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx)),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 1
            }),
            "nothing Empowered: the printed three"
        );
        drop(ctx);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BLAST).unwrap();
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, MY_GEAR] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base or not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(BLAST).unwrap().zone, Some(fixtures::HAND));
    }
}
