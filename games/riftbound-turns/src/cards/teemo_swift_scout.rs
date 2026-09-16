use super::prelude::{
    a_card, activated, bounce, card_target, done, exhausting_self, legend, named, ONE_ENERGY,
};
use super::{Card, Cost, Filter, Flow, Item, Keyword, Stage, Timing, KIND_UNIT};
use crate::engine::ctx::Ctx;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const HIDE_FOR: Cost = ONE_ENERGY;
pub const PRICE: Cost = ONE_ENERGY;
pub const TEEMO: &str = "Teemo";

pub const TEEMO_UNIT_YOU_OWN: Filter = Filter::And(&[
    Filter::Owned,
    Filter::Champion(TEEMO),
    Filter::Or(&[
        Filter::Unit,
        Filter::And(&[Filter::InChampionZone, Filter::Kind(KIND_UNIT)]),
    ]),
]);

pub fn alternative_hide_cost(ctx: &Ctx, seat: u8, legend: u32, card: u32) -> Option<Cost> {
    let mine = ctx.is_legend(legend) && ctx.controller(legend) == seat;
    (mine && ctx.has_keyword(card, Keyword::Hidden)).then_some(HIDE_FOR)
}

fn to_hand(ctx: &mut Ctx, seat: u8, card: u32) -> bool {
    if ctx.on_board(card) {
        return bounce(ctx, card);
    }
    let (Some(champion), Some(hand)) = (ctx.zones.champion, ctx.zones.hand) else {
        return false;
    };
    if ctx.card(card).and_then(|held| held.zone) != Some(champion) || ctx.owner(card) != seat {
        return false;
    }
    ctx.emit(Effect::Move {
        card,
        zone: hand,
        seat,
        index: TOP,
    });
    ctx.narrate(format!(
        "{{card {card}}} leaves the Champion Zone for {{seat {seat}}}'s hand"
    ));
    true
}

fn recall(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    match card_target(ctx, item, 0) {
        Some(card) => {
            to_hand(ctx, seat, card);
        }
        None => ctx.narrate(format!(
            "{{seat {seat}}} has no Teemo unit to put into hand"
        )),
    }
    done()
}

pub static CARD: Card = legend(
    "Teemo - Swift Scout",
    &[],
    &[named(
        exhausting_self(activated(
            Timing::Sorcery,
            PRICE,
            &[a_card(
                TEEMO_UNIT_YOU_OWN,
                "a Teemo unit you own on the board or in your Champion Zone",
            )],
            recall,
        )),
        "put a Teemo unit into your hand",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::hide;
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, priority, settle, targets};
    use crate::state::{ChainItem, ItemKind, ItemStatus, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::BOTTOM;
    use agni_plugin_sdk::table::CardInfo;

    const SWIFT: u32 = fixtures::LEGEND_CARD;
    const SCOUT: u32 = fixtures::CHAMPION_CARD;
    const STRATEGIST: u32 = 90;
    const THEIR_TEEMO: u32 = 91;

    fn teemo(id: u32, zone: u16, seat: u8, name: &str) -> CardInfo {
        CardInfo {
            energy: Some(2),
            might: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::card(id, zone, seat, name, KIND_UNIT)
        }
    }

    fn bush() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(SWIFT).unwrap().name = CARD.name.into();
        fixture.table.card_mut(SCOUT).unwrap().name = "Teemo - Scout".into();
        fixture
            .table
            .cards
            .push(teemo(STRATEGIST, fixtures::BF1, 0, "Teemo - Strategist"));
        fixture
            .table
            .cards
            .push(teemo(THEIR_TEEMO, fixtures::BASE, 1, "Teemo - Scout"));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn recycled(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card,
                    zone,
                    index: BOTTOM,
                    ..
                } if Some(*zone) == ctx.zones.rune_deck => Some(*card),
                _ => None,
            })
            .collect()
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_legend_has_one_sorcery_activation_whose_teemo_is_chosen_as_it_is_activated() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(PRICE));
        assert_eq!(PRICE.energy, 1);
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.label, Some("put a Teemo unit into your hand"));
        assert_eq!(ability.targets.len(), 1, "a public-zone object is a target");
        assert_eq!(ability.targets[0].filter, TEEMO_UNIT_YOU_OWN);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert!(ability.candidates.is_none());
        let mut fixture = bush();
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(SWIFT).unwrap(), &CARD));
        assert_eq!(cost::of_activation(&ctx, SWIFT, 0).label(), "1 energy");
        assert_eq!(
            activate::offers(&ctx, 0)
                .iter()
                .map(|offer| (offer.label.clone(), offer.enabled))
                .collect::<Vec<_>>(),
            [(
                format!("{{card {SWIFT}}}: put a Teemo unit into your hand (1 energy, exhaust)"),
                true
            )]
        );
    }

    fn teemo_units_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        let mut item = ChainItem::new(
            7,
            ItemKind::Ability {
                source: SWIFT,
                index: 0,
            },
            seat,
            Origin::Board,
        );
        item.controller = seat;
        targets::candidates(ctx, &item, &CARD.abilities[0].targets[0])
            .into_iter()
            .filter_map(|target| match target {
                TargetRef::Card(card) => Some(card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_candidates_are_the_seats_own_teemo_units_on_the_board_and_in_the_champion_zone() {
        let mut fixture = bush();
        let ctx = fixture.ctx();
        assert_eq!(teemo_units_of(&ctx, 0), [SCOUT, STRATEGIST]);
        assert_eq!(
            teemo_units_of(&ctx, 1),
            [THEIR_TEEMO],
            "the enemy's Teemo is theirs"
        );
        assert!(
            !teemo_units_of(&ctx, 0).contains(&fixtures::VI),
            "Vi is no Teemo"
        );
        assert!(
            !teemo_units_of(&ctx, 0).contains(&SWIFT),
            "the legend is a Legend, not a unit"
        );
        drop(ctx);
        let mut in_hand = bush();
        in_hand.table.card_mut(STRATEGIST).unwrap().zone = Some(fixtures::HAND);
        let ctx = in_hand.ctx();
        assert_eq!(
            teemo_units_of(&ctx, 0),
            [SCOUT],
            "a Teemo already in hand is neither on the board nor in the Champion Zone"
        );
        drop(ctx);
        let mut possessed = bush();
        possessed.resolve();
        let mut ctx = possessed.ctx();
        assert!(ctx.set_controller(STRATEGIST, 1, THEIR_TEEMO));
        assert_eq!(
            teemo_units_of(&ctx, 0),
            [SCOUT, STRATEGIST],
            "a Teemo you own but no longer control is still yours to recall"
        );
        assert_eq!(teemo_units_of(&ctx, 1), [THEIR_TEEMO]);
    }

    #[test]
    fn the_activation_pays_one_energy_exhausts_him_and_asks_which_teemo_comes_to_hand() {
        let mut fixture = bush();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, SWIFT, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {SCOUT}}}"),
                format!("{{card {STRATEGIST}}}"),
                "cancel".to_string()
            ],
            "the Scout in the Champion Zone and the Strategist at the battlefield"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.card(SWIFT).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "one energy, one rune");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == SWIFT
        ));
        assert_eq!(ctx.blob.chain[0].status, ItemStatus::Finalized);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(SCOUT)]);
        assert_eq!(
            ctx.card(SCOUT).unwrap().zone,
            Some(fixtures::CHAMPION),
            "nothing until it resolves"
        );
        resolve_top(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(SCOUT).unwrap().seat, 0);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert!(ctx.effects.contains(&Effect::Move {
            card: SCOUT,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SCOUT}}} leaves the Champion Zone for {{seat 0}}'s hand"
        )));
        assert_eq!(
            ctx.location(STRATEGIST),
            Some(crate::engine::ctx::Location::Battlefield(fixtures::BF1)),
            "the other Teemo stays"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, SWIFT, 0),
            Err(Refusal::Exhausted),
            "he is spent for the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_teemo_on_the_board_returns_to_hand_like_a_bounce() {
        let mut fixture = bush();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, SWIFT, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {STRATEGIST}}}")).unwrap();
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(STRATEGIST).unwrap().zone, Some(fixtures::HAND));
        assert!(!ctx.on_board(STRATEGIST));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {STRATEGIST}}} returns to hand")));
        assert_eq!(
            ctx.card(SCOUT).unwrap().zone,
            Some(fixtures::CHAMPION),
            "the Scout stays in the Champion Zone"
        );
    }

    #[test]
    fn with_one_teemo_it_is_the_only_offer_and_with_none_the_activation_has_no_legal_target() {
        let mut fixture = bush();
        fixture.table.cards.retain(|card| card.id != STRATEGIST);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, SWIFT, 0).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {SCOUT}}}"), "cancel".to_string()]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        resolve_top(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::HAND));
        drop(ctx);
        let mut empty = bush();
        empty
            .table
            .cards
            .retain(|card| card.id != STRATEGIST && card.id != SCOUT);
        empty.resolve();
        let mut ctx = empty.ctx();
        let hand = ctx.hand_of(0).len();
        assert_eq!(
            activate::activate(&mut ctx, 0, SWIFT, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "no Teemo anywhere · the activation is refused before anything is paid"
        );
        assert!(!ctx.card(SWIFT).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
    }

    #[test]
    fn a_teemo_that_left_the_champion_zone_in_response_is_no_longer_there_to_recall() {
        let mut fixture = bush();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, SWIFT, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        ctx.emit(Effect::Move {
            card: SCOUT,
            zone: fixtures::TRASH,
            seat: 0,
            index: TOP,
        });
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(SCOUT).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no Teemo unit to put into hand".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_activation_is_refused_for_the_wrong_seat_an_exhausted_legend_and_a_busy_chain() {
        let mut fixture = bush();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, SWIFT, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, SWIFT, 1),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        drop(ctx);
        let mut spent = bush();
        spent.table.card_mut(SWIFT).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SWIFT, 0),
            Err(Refusal::Exhausted)
        );
        assert!(activate::offers(&ctx, 0).is_empty());
        drop(ctx);
        let mut busy = bush();
        let mut ctx = busy.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            activate::activate(&mut ctx, 0, SWIFT, 0),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
    }

    #[test]
    fn the_alternative_hide_cost_is_one_energy_for_a_hidden_card_of_his_controller() {
        let mut fixture = bush();
        fixture.table.card_mut(STRATEGIST).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(STRATEGIST, Keyword::Hidden));
        assert_eq!(
            alternative_hide_cost(&ctx, 0, SWIFT, STRATEGIST),
            Some(HIDE_FOR)
        );
        assert_eq!(HIDE_FOR, ONE_ENERGY);
        assert_eq!(
            alternative_hide_cost(&ctx, 1, SWIFT, STRATEGIST),
            None,
            "the legend is seat 0's"
        );
        assert_eq!(
            alternative_hide_cost(&ctx, 0, SWIFT, fixtures::HAND_SPELL),
            None,
            "Spark has no Hidden · a card without it hides for the rainbow rune"
        );
        assert_eq!(
            alternative_hide_cost(&ctx, 0, STRATEGIST, STRATEGIST),
            None,
            "only the legend grants it"
        );
    }

    #[test]
    #[ignore = "engine gap · Hidden rules: hide::cost() is the fixed rainbow rune and hide::hide plans against it alone; with the alternative_hide_cost seam consulted, a card with [Hidden] hides for 1 energy and the rune exhausts instead of recycling"]
    fn hiding_a_hidden_card_with_him_pays_one_energy_instead_of_the_rainbow_rune() {
        let mut fixture = bush();
        fixture.table.card_mut(STRATEGIST).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        hide::hide(&mut ctx, 0, STRATEGIST, fixtures::BF1).unwrap();
        assert!(hide::is_facedown(&ctx, STRATEGIST));
        assert!(recycled(&ctx).is_empty(), "no rune is recycled");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "one rune exhausts for the energy"
        );
    }
}
