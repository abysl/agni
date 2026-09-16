use super::prelude::{
    a_play_location, asking, done, play, ready, remember_card, remembered_cards, spawn, spell,
    with_candidates, zone_target, Location, Token,
};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;
use crate::engine::{cost, pay};
use crate::state::TargetRef;

pub const ORDER: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Order)],
};
pub const QUESTION: &str = "the Sand Soldier to ready for an Order rune";
pub const READY_FOR_ORDER: u8 = 1;
pub const SOLDIER_ARRIVES_READY: bool = false;

pub fn order_cost() -> cost::Cost {
    cost::of_script(&ORDER, &[])
}

pub fn readiable_soldier(ctx: &Ctx, item: &Item) -> Option<u32> {
    remembered_cards(item)
        .into_iter()
        .find(|soldier| ctx.on_board(*soldier) && ctx.is_unit(*soldier))
        .filter(|_| pay::affordable(ctx, item.controller, &order_cost()))
}

fn soldier_to_ready(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != READY_FOR_ORDER {
        return Vec::new();
    }
    readiable_soldier(ctx, item)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub fn pay_order_to_ready(ctx: &mut Ctx, seat: u8, soldier: u32) -> bool {
    let total = order_cost();
    let Ok(plan) = pay::plan(ctx, seat, &total) else {
        ctx.narrate(format!(
            "{{card {soldier}}} stays exhausted · the Order rune can't be paid"
        ));
        return false;
    };
    pay::pay(ctx, seat, &plan);
    ctx.narrate(format!("{{seat {seat}}} pays {}", total.label()));
    ready(ctx, soldier)
}

fn guards(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 == READY_FOR_ORDER {
        let picked = ctx.picks().first().copied();
        match (picked, readiable_soldier(ctx, item)) {
            (Some(picked), Some(soldier)) if picked == soldier => {
                pay_order_to_ready(ctx, seat, soldier);
            }
            (_, Some(soldier)) => {
                ctx.narrate(format!("{{card {soldier}}} stays exhausted"));
            }
            _ => {}
        }
        return done();
    }
    let at = zone_target(item, 0)
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .unwrap_or(Location::Base(seat));
    let Some(soldier) = spawn(ctx, seat, Token::SandSoldier, at, SOLDIER_ARRIVES_READY) else {
        return done();
    };
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {soldier}}} to {}",
        describe(at)
    ));
    if !pay::affordable(ctx, seat, &order_cost()) {
        return done();
    }
    remember_card(ctx, soldier);
    Flow::Ask(ctx.ask_resume(item, READY_FOR_ORDER, 0, 1))
}

pub static CARD: Card = spell(
    "Guards!",
    &[Keyword::Hidden],
    &[asking(
        with_candidates(
            play(
                &[a_play_location("where the Sand Soldier is played")],
                guards,
            ),
            soldier_to_ready,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger, TOKEN_SAND_SOLDIER};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, prompts, settle};
    use crate::state::{Origin, Priority, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const GUARDS: u32 = 90;
    const ORDER_RUNE: u32 = 46;

    fn guards_card(seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Order".into()],
            ..fixtures::spell(GUARDS, fixtures::HAND, seat, "Guards!", 3, 0)
        }
    }

    fn palace(order_runes: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(guards_card(0));
        for id in (ORDER_RUNE..).take(order_runes) {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Order", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(GUARDS).unwrap(),
            &CARD
        ));
        fixture
    }

    fn soldiers_of<'c>(ctx: &'c Ctx, seat: u8) -> Vec<&'c CardInfo> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SAND_SOLDIER && card.owner == seat)
            .collect()
    }

    fn cast_to(ctx: &mut Ctx, zone: u16) {
        fixtures::play_from_hand(ctx, 0, GUARDS).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{zone {zone}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Zone(zone)]);
        assert!(soldiers_of(ctx, 0).is_empty(), "nothing before it resolves");
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_a_hidden_spell_that_picks_a_play_location_and_asks_about_the_rune() {
        assert!(std::ptr::eq(script_of("Guards!").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].kind, TargetKind::Zone);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(Token::SandSoldier.face().might, Some(2));
    }

    #[test]
    fn the_soldier_is_played_exhausted_where_chosen_and_an_order_rune_readies_it() {
        let mut fixture = palace(1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, GUARDS).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1),
                "cancel".to_string()
            ],
            "the base and the held battlefield"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let soldier = soldiers_of(&ctx, 0);
        assert_eq!(soldier.len(), 1);
        let soldier = soldier[0].id;
        assert_eq!(
            ctx.location(soldier),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.card(soldier).unwrap().exhausted, "played exhausted");
        assert_eq!(ctx.current_might(soldier), 2);
        assert!(ctx.is_token(soldier));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: READY_FOR_ORDER
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {soldier}}}"), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {GUARDS}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {soldier}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(soldier).unwrap().exhausted, "readied");
        assert!(
            ctx.effects.contains(&Effect::Move {
                card: ORDER_RUNE,
                zone: fixtures::RUNE_DECK,
                seat: 0,
                index: agni_plugin_sdk::decide::BOTTOM
            }),
            "the Order rune is recycled for its power: {:?}",
            ctx.effects
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} pays 1 Order power".to_string()));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == soldier
        )));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, origin: Origin::Board, .. } if *card == soldier
        )));
        assert_eq!(ctx.card(GUARDS).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_leaves_the_soldier_exhausted_and_the_rune_untouched() {
        let mut fixture = palace(1);
        let mut ctx = fixture.ctx();
        cast_to(&mut ctx, fixtures::BASE);
        let soldier = soldiers_of(&ctx, 0)[0].id;
        assert_eq!(ctx.location(soldier), Some(Location::Base(0)));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.card(soldier).unwrap().exhausted);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {soldier}}} stays exhausted")));
        assert!(!ctx.card(ORDER_RUNE).unwrap().exhausted, "nothing is paid");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_an_order_rune_nothing_is_asked() {
        let mut fixture = palace(0);
        let mut ctx = fixture.ctx();
        cast_to(&mut ctx, fixtures::BASE);
        assert!(ctx.blob.prompt.is_none(), "no rune to pay: no question");
        assert!(ctx.blob.chain.is_empty());
        let soldier = soldiers_of(&ctx, 0);
        assert_eq!(soldier.len(), 1);
        assert!(soldier[0].exhausted);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_facedown_the_soldier_is_played_at_the_hiding_battlefield_for_free() {
        let mut fixture = palace(1);
        fixture.table.card_mut(GUARDS).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(GUARDS).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let chain = ctx.zones.chain.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(GUARDS, chain, 0), 0)
            .unwrap();
        play_engine::begin(
            &mut ctx,
            0,
            GUARDS,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{zone {}}}", fixtures::BF1), "cancel".to_string()],
            "737.1.d · only the hiding battlefield"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert!(
            ctx.runes_of(0)
                .iter()
                .all(|rune| rune.exhausted == (rune.id == fixtures::RUNE_A)),
            "a hidden card reacts for free"
        );
        fixtures::pass_until_open(&mut ctx);
        let soldier = soldiers_of(&ctx, 0);
        assert_eq!(soldier.len(), 1);
        assert_eq!(soldier[0].zone, Some(fixtures::BF1));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: READY_FOR_ORDER
            })
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_hand_it_is_a_sorcery_refused_while_the_chain_is_closed() {
        let mut fixture = palace(1);
        fixture.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: GUARDS,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "Hidden alone is not Reaction from hand"
        );
    }
}
