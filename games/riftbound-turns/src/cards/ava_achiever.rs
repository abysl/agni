use super::prelude::{
    asking, done, forget_revealing, location_of, on_attack, optional, unit, when, with_candidates,
    with_cost, Location,
};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::play;
use crate::state::{Origin, TargetRef, FLAG_REVEALING};

pub const MIND: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Mind)],
};
pub const QUESTION: &str = "a card with Hidden in your hand to play here";
pub const PICKED: u8 = 1;
pub const REVEALED: u8 = 2;

pub fn hidden_in_hand(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.hand_of(seat)
        .into_iter()
        .filter(|card| ctx.has_keyword(*card, Keyword::Hidden))
        .collect()
}

fn picked_in_hand(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.has(FLAG_REVEALING))
        .map(|row| row.id)
        .find(|card| ctx.owner(*card) == seat && ctx.in_hand(*card))
}

pub fn play_hidden_from_hand_here_ignoring_cost(ctx: &mut Ctx, item: &Item, card: u32) -> bool {
    let me = item.kind.source();
    let seat = item.controller;
    let Some(here @ Location::Battlefield(zone)) = location_of(ctx, me) else {
        ctx.narrate(format!(
            "{{card {me}}} is no longer at a battlefield · {{card {card}}} stays in hand"
        ));
        return false;
    };
    if !ctx.has_keyword(card, Keyword::Hidden) {
        ctx.narrate(format!("{{card {card}}} has no Hidden · it stays in hand"));
        return false;
    }
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {card}}} from hand at {{zone {zone}}}, ignoring its cost"
    ));
    play::begin(ctx, seat, card, Origin::Banishment, Some(here)).is_ok()
}

fn a_card_in_hand(ctx: &Ctx, _: &Event, source: Source) -> bool {
    !ctx.hand_of(ctx.controller(source.card)).is_empty()
}

fn hand_cards(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != PICKED {
        return Vec::new();
    }
    ctx.hand_of(item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn overachieve(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        PICKED => {
            let Some(card) = ctx
                .picks()
                .first()
                .copied()
                .filter(|card| ctx.hand_of(seat).contains(card))
            else {
                ctx.narrate(format!("{{seat {seat}}} plays nothing"));
                return done();
            };
            ctx.set_flag(card, FLAG_REVEALING, true);
            if ctx.table.is_revealed(card) {
                return overachieve(ctx, item, Stage(REVEALED));
            }
            ctx.narrate(format!("{{seat {seat}}} reveals {{card {card}}}"));
            Flow::Ask(ctx.await_faces(item, &[card], REVEALED))
        }
        REVEALED => {
            let Some(card) = picked_in_hand(ctx, seat) else {
                return done();
            };
            forget_revealing(ctx, card);
            play_hidden_from_hand_here_ignoring_cost(ctx, item, card);
            done()
        }
        _ => {
            if ctx.hand_of(seat).is_empty() {
                ctx.narrate(format!("{{seat {seat}}} has no card in hand"));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Ava Achiever",
    &[],
    &[asking(
        with_candidates(
            when(
                optional(with_cost(on_attack(&[], overachieve), MIND)),
                a_card_in_hand,
            ),
            hand_cards,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, Who, KIND_SPELL, KIND_UNIT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{chain, prompts, settle, triggers};
    use crate::state::{ItemKind, ItemStatus, PromptWhy};
    use agni_plugin_sdk::decide::{Action, Effect};
    use agni_plugin_sdk::table::Face;

    const AVA: u32 = 90;
    const BACK_OFF: u32 = 91;
    const TIDETURNER: u32 = 92;
    const BRUTE: u32 = 93;
    const MIND_RUNE: u32 = 46;

    fn face_of(id: u32) -> Face {
        match id {
            BACK_OFF => Face::named("Back Off")
                .with_kind(KIND_SPELL)
                .with_domain(vec!["Calm".into()]),
            TIDETURNER => Face::named("Tideturner")
                .with_kind(KIND_UNIT)
                .with_might(Some(2))
                .with_domain(vec!["Chaos".into()]),
            _ => Face::named("Brute")
                .with_kind(KIND_UNIT)
                .with_might(Some(3))
                .with_domain(vec!["Fury".into()]),
        }
    }

    fn classroom() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut ava = fixtures::unit(AVA, fixtures::BF1, 0, "Ava Achiever", 4);
        ava.domain = vec!["Mind".into()];
        ava.energy = Some(5);
        fixture.table.cards.push(ava);
        fixture
            .table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::HAND) && card.seat == 0));
        for id in [BACK_OFF, TIDETURNER, BRUTE] {
            fixture
                .table
                .cards
                .push(fixtures::hidden(id, fixtures::HAND, 0));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(AVA).unwrap(), &CARD));
        fixture
    }

    fn paid_with_the_mind_rune(ctx: &Ctx) -> bool {
        ctx.effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::Move {
                    card: MIND_RUNE,
                    zone: fixtures::RUNE_DECK,
                    seat: 0,
                    ..
                }
            )
        })
    }

    fn attacks(ctx: &mut Ctx) -> usize {
        ctx.raise(Event::Attacks { card: AVA });
        let found = triggers::collect(ctx);
        chain::proceed(ctx);
        found
    }

    fn pay_and_pick(fixture: &mut Fixture, card: u32) {
        let mut ctx = fixture.ctx();
        assert_eq!(attacks(&mut ctx), 1);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {card}}}")).unwrap();
        let parked = &ctx.blob.chain[0];
        assert_eq!(
            (parked.status, parked.stage),
            (ItemStatus::Resolving, REVEALED)
        );
        assert_eq!(parked.awaiting, [card]);
        assert!(ctx.blob.prompt.is_none(), "the host's reveal is awaited");
        assert!(ctx.effects.contains(&Effect::Reveal { card }));
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    fn reveal(fixture: &mut Fixture, card: u32) {
        let action = Action::Reveal {
            card,
            face: face_of(card),
        };
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(chain::face_arrived(&mut ctx, card).unwrap());
        settle(&mut ctx).unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
    }

    #[test]
    fn the_script_is_a_unit_with_an_optional_mind_costed_attack_trigger_that_asks_for_a_hidden_card(
    ) {
        assert!(std::ptr::eq(script_of("Ava Achiever").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Attacks(Who::Me));
        assert!(ability.optional);
        assert_eq!(ability.cost, Some(MIND));
        assert!(ability.targets.is_empty());
        assert!(ability.condition.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn paying_mind_puts_the_trigger_on_the_chain_and_it_offers_every_card_of_the_blank_hand() {
        let mut fixture = classroom();
        let mut ctx = fixture.ctx();
        assert!(
            hidden_in_hand(&ctx, 0).is_empty(),
            "the public fold carries no hand faces, so Hidden cannot be read before a reveal"
        );
        assert_eq!(attacks(&mut ctx), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "pay 1 Mind power for the {card 90} trigger?"
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            paid_with_the_mind_rune(&ctx),
            "the Mind rune pays the trigger: {:?}",
            ctx.effects
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == AVA
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICKED
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BACK_OFF}}}"),
                format!("{{card {TIDETURNER}}}"),
                format!("{{card {BRUTE}}}"),
                "skip".to_string()
            ],
            "every hand card is offered · kai shows the faces it holds"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} plays nothing".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_revealed_hidden_card_is_played_at_avas_battlefield_for_free() {
        let mut fixture = classroom();
        let runes_before: Vec<(u32, bool)> = fixture
            .ctx()
            .runes_of(0)
            .into_iter()
            .filter(|rune| rune.id != MIND_RUNE)
            .map(|rune| (rune.id, rune.exhausted))
            .collect();
        pay_and_pick(&mut fixture, TIDETURNER);
        reveal(&mut fixture, TIDETURNER);
        let ctx = fixture.ctx();
        assert!(
            ctx.blob.chain.is_empty(),
            "the trigger and the free play both resolved"
        );
        assert_eq!(
            ctx.location(TIDETURNER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(!ctx.has_flag(TIDETURNER, FLAG_REVEALING));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {TIDETURNER}}} from hand at {{zone {}}}, ignoring its cost",
            fixtures::BF1
        )));
        assert!(
            ctx.card(MIND_RUNE).is_none()
                || ctx.card(MIND_RUNE).unwrap().zone == Some(fixtures::RUNE_DECK),
            "the Mind rune paid the trigger"
        );
        let runes_after: Vec<(u32, bool)> = ctx
            .runes_of(0)
            .into_iter()
            .filter(|rune| rune.id != MIND_RUNE)
            .map(|rune| (rune.id, rune.exhausted))
            .collect();
        assert_eq!(runes_before, runes_after, "nothing pays the Tideturner");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_revealed_card_without_hidden_stays_in_hand() {
        let mut fixture = classroom();
        pay_and_pick(&mut fixture, BRUTE);
        reveal(&mut fixture, BRUTE);
        let ctx = fixture.ctx();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_hand(BRUTE));
        assert!(!ctx.has_flag(BRUTE, FLAG_REVEALING));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BRUTE}}} has no Hidden · it stays in hand"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_face_already_public_skips_the_wait() {
        let mut fixture = classroom();
        fixture
            .table
            .apply_entry(
                &Action::Reveal {
                    card: BACK_OFF,
                    face: face_of(BACK_OFF),
                },
                0,
            )
            .unwrap();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(hidden_in_hand(&ctx, 0), [BACK_OFF]);
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BACK_OFF}}}")).unwrap();
        assert!(
            ctx.blob.chain.iter().all(|held| held.awaiting.is_empty()),
            "nothing to await"
        );
        assert_eq!(
            ctx.card(BACK_OFF).unwrap().zone,
            Some(fixtures::CHAIN),
            "the spell is on the chain, played for nothing"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_removes_the_trigger_and_without_a_mind_rune_or_a_hand_nothing_is_asked() {
        let mut fixture = classroom();
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost is declined".to_string()));
        assert!(!paid_with_the_mind_rune(&ctx));
        drop(ctx);

        let mut poor = classroom();
        poor.table.cards.retain(|card| card.id != MIND_RUNE);
        poor.resolve();
        let mut ctx = poor.ctx();
        assert_eq!(attacks(&mut ctx), 1);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger is removed · its cost can't be paid".to_string()));
        drop(ctx);

        let mut empty = classroom();
        empty
            .table
            .cards
            .retain(|card| ![BACK_OFF, TIDETURNER, BRUTE].contains(&card.id));
        empty.resolve();
        let mut ctx = empty.ctx();
        assert!(ctx.hand_of(0).is_empty());
        assert_eq!(
            attacks(&mut ctx),
            0,
            "an empty hand: the may is not offered"
        );
        assert!(ctx.blob.prompt.is_none());
    }
}
