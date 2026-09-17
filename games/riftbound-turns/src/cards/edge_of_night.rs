use super::prelude::{
    asking, attach_gear, done, equip, gear, hiding_battlefield, on_play_from_facedown,
    while_attached, with_candidates, with_statics, Location, CHAOS,
};
use super::{Card, Flow, Grant, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const MIGHT_BONUS: i16 = 2;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

const WEARER: u8 = 1;

fn wearers_here(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let Some(zone) = hiding_battlefield(ctx, item) else {
        return Vec::new();
    };
    ctx.units_at(Location::Battlefield(zone))
        .into_iter()
        .filter(|unit| ctx.controller(*unit) == item.controller)
        .map(TargetRef::Card)
        .collect()
}

fn attach_here(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let gear = item.kind.source();
    if !ctx.on_board(gear) {
        return done();
    }
    if stage.0 != WEARER {
        if wearers_here(ctx, item, stage).is_empty() {
            ctx.narrate(format!(
                "{{card {gear}}} finds no unit of its owner's to attach to"
            ));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, WEARER, 1, 1));
    }
    if let Some(unit) = ctx.picks().first().copied() {
        attach_gear(ctx, gear, unit);
    }
    done()
}

pub static CARD: Card = with_statics(
    gear(
        "Edge of Night",
        &[Keyword::Hidden, Keyword::Equip(CHAOS)],
        &[
            asking(
                with_candidates(on_play_from_facedown(&[], attach_here), wearers_here),
                "a unit you control here to wear it",
            ),
            equip(CHAOS),
        ],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, is_attached, FRIENDLY_UNIT};
    use crate::cards::{SelfCost, Static, TargetKind, Timing, Trigger};
    use crate::engine::ctx::{Event, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{act, activate, play as play_engine, priority, prompts, resume, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, FLAG_FROM_FACEDOWN};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const EDGE: u32 = 90;
    const WAILER: u32 = 95;
    const FACEDOWN_PLAY: u8 = 0;
    const EQUIP: u8 = 1;
    const ENERGY: u8 = 3;
    const TRIGGER: u16 = 2;

    fn edge(zone: u16) -> CardInfo {
        let mut card = fixtures::gear(EDGE, zone, 0, "Edge of Night", ENERGY);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn chaos_runes(fixture: &mut Fixture) {
        for rune in [fixtures::RUNE_A, 41] {
            let held = fixture.table.card_mut(rune).unwrap();
            held.domain = vec!["Chaos".into()];
            held.name = "Chaos Rune".into();
        }
    }

    fn held() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.cards.push(edge(fixtures::HAND));
        chaos_runes(&mut fixture);
        fixture.resolve();
        fixture
    }

    fn face(fixture: &mut Fixture, shown: bool) {
        {
            let card = fixture.table.card_mut(EDGE).unwrap();
            if shown {
                card.name = "Edge of Night".into();
                card.kind = Some("Gear".into());
                card.energy = Some(ENERGY);
                card.domain = vec!["Chaos".into()];
            } else {
                card.name = String::new();
                card.kind = None;
                card.energy = None;
                card.domain.clear();
            }
        }
        fixture.resolve();
    }

    fn hide_now(fixture: &mut Fixture, zone: u16) -> Result<(), Refusal> {
        let action = fixtures::move_action(EDGE, zone, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.unwrap();
        let intent = legal::classify(&ctx, 0, &entry)?;
        assert_eq!(intent, legal::Intent::Hide { card: EDGE, zone });
        let done = act(&mut ctx, 0, intent);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        done
    }

    fn hidden_at_bf1() -> Fixture {
        let mut fixture = held();
        face(&mut fixture, false);
        hide_now(&mut fixture, fixtures::BF1)
            .expect("737.1.b · the [A] hide at a held battlefield");
        assert_eq!(
            fixture.blob.card_state(EDGE).and_then(|row| row.hidden_at),
            Some(fixtures::BF1)
        );
        fixture.blob.core_mut().unwrap().turn += 1;
        face(&mut fixture, true);
        fixture
    }

    fn play_from_facedown(ctx: &mut Ctx) {
        let intent = legal::classify(ctx, 0, &ctx.entry.unwrap()).unwrap();
        assert_eq!(intent, legal::Intent::PlayFromFacedown { card: EDGE });
        act(ctx, 0, intent).unwrap();
        settle(ctx).unwrap();
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? else {
            return settle(ctx);
        };
        match (answered.why, answered.answer) {
            (PromptWhy::Target { item, .. }, Answer::Cancel) => play_engine::cancel(ctx, item),
            (PromptWhy::Target { item, spec }, _) => {
                play_engine::choose_targets(ctx, item, spec, &answered.prompt.picked)?
            }
            (PromptWhy::Resume { .. }, _) => resume(ctx, &answered)?,
            (why, answer) => panic!("Edge of Night never asks this: {why:?} {answer:?}"),
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    fn might_counters(ctx: &Ctx, card: u32) -> i32 {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Counter {
                    target: Target::Card(held),
                    counter,
                    delta,
                } if *held == card && *counter == COUNTER_MIGHT => Some(*delta),
                _ => None,
            })
            .sum()
    }

    fn chosen(ctx: &Ctx) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Chosen { .. }))
            .count()
    }

    #[test]
    fn the_script_is_a_hidden_equipment_whose_facedown_play_attaches_without_targeting() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Edge of Night").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Edge of Night");
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Equip(CHAOS)));
        assert_eq!(CARD.equip_cost(), Some(CHAOS));
        assert_eq!(CARD.abilities.len(), 2);
        let facedown = &CARD.abilities[usize::from(FACEDOWN_PLAY)];
        assert_eq!(facedown.trigger, Trigger::PlayFromFacedown);
        assert!(
            facedown.targets.is_empty(),
            "421.3 · attaching chooses no target"
        );
        assert!(facedown.candidates.is_some());
        assert_eq!(
            facedown.question,
            Some("a unit you control here to wear it")
        );
        assert!(!facedown.optional);
        let equip = &CARD.abilities[usize::from(EQUIP)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(CHAOS));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets.len(), 1);
        assert_eq!(equip.targets[0].kind, TargetKind::Card);
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(
            matches!(CARD.attached_grants(), [Grant::Might(2)]),
            "the Might Bonus is printed; any Effect Text is the unconfirmed seam"
        );
        assert_eq!(MIGHT_BONUS, 2);
    }

    #[test]
    fn played_from_facedown_it_lands_in_the_base_then_attaches_to_a_chosen_unit_of_yours_here() {
        let mut fixture = hidden_at_bf1();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(WAILER, fixtures::BF1, 0, "Wailer", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(96, fixtures::BASE, 0, "Wailer", 2));
        fixture.resolve();
        let action = fixtures::move_action(EDGE, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let ready = ctx.ready_runes_of(0).len();
        play_from_facedown(&mut ctx);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready,
            "737.1.b · played from facedown for nothing"
        );
        assert_eq!(
            ctx.location(EDGE),
            Some(Location::Base(0)),
            "a gear played from hidden lands in the base first"
        );
        assert!(ctx.has_flag(EDGE, FLAG_FROM_FACEDOWN));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the play trigger waits on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger {
                source: EDGE,
                index: FACEDOWN_PLAY
            }
        ));
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        assert!(
            !is_attached(&ctx, EDGE),
            "nothing until the trigger resolves"
        );
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: TRIGGER,
                stage: WEARER
            }),
            "the play took item 1; its trigger is item 2"
        );
        assert_eq!(
            labels(&ctx),
            ["{card 50}", "{card 95}"],
            "only your units at the hiding battlefield; not Jinx there, not the Wailer at home"
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Resume {
                    item: TRIGGER,
                    stage: WEARER
                }
            ),
            "{card 90}: choose a unit you control here to wear it (0 of 1)"
        );
        pick(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, EDGE), Some(WAILER));
        assert_eq!(
            ctx.location(EDGE),
            Some(Location::Battlefield(fixtures::BF1)),
            "421.4 · the gear's location becomes the wearer's"
        );
        assert_eq!(
            ctx.current_might(WAILER),
            4,
            "136.3 · +2 Might while attached"
        );
        assert_eq!(might_counters(&ctx, WAILER), 2);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(
            chosen(&ctx),
            0,
            "421.3 · the facedown attach is not a target, so nothing was chosen"
        );
        assert!(ctx
            .blob
            .card_state(EDGE)
            .is_some_and(|row| row.hidden_at.is_none()));
        assert_eq!(
            activate::activate(&mut ctx, 0, EDGE, EQUIP),
            Err(Refusal::Illegal(Reason::Attached)),
            "134.4 · the attached gear's own Equip is inactive"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_single_unit_of_yours_here_is_dressed_without_a_prompt() {
        let mut fixture = hidden_at_bf1();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let action = fixtures::move_action(EDGE, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_from_facedown(&mut ctx);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "one candidate answers itself");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, EDGE), Some(fixtures::VI));
        assert_eq!(
            ctx.location(EDGE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert!(!is_attached(&ctx, fixtures::THEIR_UNIT));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} is attached to {card 50}".to_string()));
    }

    #[test]
    fn with_no_unit_of_yours_here_the_play_still_happens_and_the_gear_stays_loose_in_the_base() {
        let mut fixture = hidden_at_bf1();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let action = fixtures::move_action(EDGE, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_from_facedown(&mut ctx);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, EDGE));
        assert_eq!(
            ctx.location(EDGE),
            Some(Location::Base(0)),
            "Vi at home is not 'here', and an enemy is never yours"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} finds no unit of its owner's to attach to".to_string()));
        assert!(
            activate::offers(&ctx, 0)
                .iter()
                .any(|offer| offer.source == EDGE && offer.enabled),
            "the loose gear can still be equipped for a Chaos rune later"
        );
    }

    #[test]
    fn played_from_hand_it_does_not_attach_itself_but_equips_for_a_chaos_rune() {
        let mut fixture = held();
        fixture.resolve();
        let action = fixtures::move_action(EDGE, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        assert!(matches!(intent, legal::Intent::Play { .. }), "{intent:?}");
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(EDGE), Some(Location::Base(0)));
        assert!(
            ctx.blob.chain.is_empty(),
            "the facedown trigger does not fire on a play from hand"
        );
        assert!(!is_attached(&ctx, EDGE));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "three energy paid from hand, unlike the free facedown play"
        );
        drop(ctx);

        let mut fixture = held();
        fixture.table.card_mut(EDGE).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let offers = activate::offers(&ctx, 0);
        let offer = offers
            .iter()
            .find(|offer| offer.source == EDGE)
            .unwrap_or_else(|| panic!("the loose gear offers its Equip: {offers:?}"));
        assert!(offer.enabled);
        assert_eq!(offer.index, EQUIP);
        assert_eq!(offer.label, "{card 90}: equip (1 Chaos power)");
        activate::activate(&mut ctx, 0, EDGE, EQUIP).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(labels(&ctx), ["{card 50}", "cancel"]);
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(chosen(&ctx), 1, "744.1.b.1 · Equip's choice is a target");
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, EDGE), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(might_counters(&ctx, fixtures::VI), 2);
        assert_eq!(ctx.runes_of(0).len(), 3, "the Chaos rune was recycled");
        ctx.detach(EDGE);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(might_counters(&ctx, fixtures::VI), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn hiding_needs_a_held_battlefield_the_facedown_play_waits_a_turn_and_equip_refuses_the_enemy()
    {
        let mut unheld = Fixture::enforced();
        unheld.table.cards.push(edge(fixtures::HAND));
        chaos_runes(&mut unheld);
        face(&mut unheld, false);
        assert_eq!(
            hide_now(&mut unheld, fixtures::BF1),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "737.1.b · a battlefield you control"
        );
        drop(unheld);

        let mut fixture = held();
        face(&mut fixture, false);
        hide_now(&mut fixture, fixtures::BF1).unwrap();
        face(&mut fixture, true);
        {
            let action = fixtures::move_action(EDGE, fixtures::CHAIN, 0);
            let ctx = fixture.ctx_for(0, &action);
            assert_eq!(
                legal::classify(&ctx, 0, &ctx.entry.unwrap()),
                Err(Refusal::Illegal(Reason::HiddenThisTurn)),
                "737.1.b · the grant starts on the next turn"
            );
            assert!(
                activate::offers(&ctx, 0)
                    .iter()
                    .all(|offer| offer.source != EDGE),
                "408.3 · a facedown card offers no Equip"
            );
            let mut ctx = ctx;
            assert_eq!(
                activate::activate(&mut ctx, 0, EDGE, EQUIP),
                Err(Refusal::Illegal(Reason::Facedown)),
                "408.3 · nor does it activate when asked directly"
            );
        }
        drop(fixture);

        let mut fixture = held();
        fixture.table.card_mut(EDGE).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, EDGE, EQUIP),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, EDGE, FACEDOWN_PLAY),
            Err(Refusal::Illegal(Reason::NoSuchAbility)),
            "the facedown trigger is not an activation"
        );
        activate::activate(&mut ctx, 0, EDGE, EQUIP).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "744.1.c.2 · only a unit you control"
        );
        let cancel = labels(&ctx).len() as u16 - 1;
        pick(&mut ctx, 0, cancel).unwrap();
        assert!(!is_attached(&ctx, EDGE));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "a cancelled Equip pays nothing"
        );
    }
}
