use super::ambessa_matriarch_of_war::empower_me;
use super::prelude::{
    a_unit, activated, card_target, disempowering_self, done, grant_this_turn, legend, named,
    on_you_play_card, when,
};
use super::{Card, Cost, Flow, Item, Keyword, Source, Stage, Timing};
use crate::engine::ctx::{Ctx, Event};
use crate::state::Origin;

pub const ASSAULT: u8 = 2;

pub fn from_anywhere_but_your_hand(_: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::Played { origin, .. } if *origin != Origin::Hand)
}

fn lightning_rush(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if grant_this_turn(ctx, unit, Keyword::Assault(ASSAULT)) {
        ctx.narrate(format!("{{card {unit}}} has Assault {ASSAULT} this turn"));
    }
    done()
}

pub static CARD: Card = legend(
    "Yordle, Kennen - Heart of the Tempest",
    &[],
    &[
        when(
            on_you_play_card(&[], empower_me),
            from_anywhere_but_your_hand,
        ),
        named(
            disempowering_self(activated(
                Timing::Action,
                Cost::FREE,
                &[a_unit("a unit to give Assault 2")],
                lightning_rush,
            )),
            "give a unit Assault 2",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, SelfCost, Trigger, KIND_SPELL, KIND_UNIT};
    use crate::engine::ctx::COUNTER_EMPOWERED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, settle};
    use crate::state::{ItemKind, Leave, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CounterInfo, Target};

    const KENNEN: u32 = fixtures::LEGEND_CARD;

    fn storm(empowered: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(KENNEN).unwrap().name = CARD.name.into();
        if empowered {
            fixture.table.counters.push(CounterInfo {
                target: Target::Card(KENNEN),
                counter: COUNTER_EMPOWERED,
                value: 1,
            });
            fixture.table.counters.sort();
        }
        fixture.resolve();
        fixture
    }

    fn played(card: u32, controller: u8, kind: &str, origin: Origin) -> Event {
        Event::Played {
            card,
            controller,
            kind: kind.into(),
            origin,
            paid_additional: false,
        }
    }

    fn source() -> Source {
        Source {
            card: KENNEN,
            ability: 0,
        }
    }

    fn his_triggers(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == KENNEN))
            .count()
    }

    fn resolve_all(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_keeps_the_catalog_name_with_one_play_trigger_and_one_gated_action() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert_eq!(CARD.name, "Yordle, Kennen - Heart of the Tempest");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::YouPlayCard);
        assert!(empower.condition.is_some());
        assert!(!empower.optional);
        assert!(empower.cost.is_none());
        let rush = &CARD.abilities[1];
        assert_eq!(rush.trigger, Trigger::Activated(Timing::Action));
        assert_eq!(rush.cost, Some(Cost::FREE));
        assert_eq!(rush.self_cost, SelfCost::Disempower);
        assert!(rush.usable.is_none());
        assert_eq!(rush.targets.len(), 1);
        assert_eq!(ASSAULT, 2);
    }

    #[test]
    fn the_condition_reads_the_origin_of_a_play_and_refuses_the_hand() {
        let mut fixture = storm(false);
        let ctx = fixture.ctx();
        assert!(from_anywhere_but_your_hand(
            &ctx,
            &played(fixtures::CHAMPION_CARD, 0, KIND_UNIT, Origin::Champion),
            source()
        ));
        assert!(from_anywhere_but_your_hand(
            &ctx,
            &played(
                fixtures::HAND_UNIT,
                0,
                KIND_UNIT,
                Origin::Trash {
                    leave: Leave::Recycle
                }
            ),
            source()
        ));
        assert!(from_anywhere_but_your_hand(
            &ctx,
            &played(
                fixtures::HAND_HIDDEN,
                0,
                KIND_SPELL,
                Origin::Facedown {
                    zone: fixtures::BF1
                }
            ),
            source()
        ));
        assert!(!from_anywhere_but_your_hand(
            &ctx,
            &played(fixtures::HAND_UNIT, 0, KIND_UNIT, Origin::Hand),
            source()
        ));
        assert!(
            !from_anywhere_but_your_hand(
                &ctx,
                &Event::PlayedSpell {
                    item: 1,
                    controller: 0,
                    nth: 1
                },
                source()
            ),
            "a resolved spell carries no origin to read"
        );
    }

    #[test]
    fn your_champion_played_from_its_zone_queues_his_trigger_and_a_hand_play_does_not() {
        let mut fixture = storm(false);
        let mut ctx = fixture.ctx();
        ctx.raise(played(
            fixtures::CHAMPION_CARD,
            0,
            KIND_UNIT,
            Origin::Champion,
        ));
        settle(&mut ctx).unwrap();
        assert_eq!(his_triggers(&ctx), 1, "the trigger waits on the chain");
        resolve_all(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut hand = storm(false);
        let mut ctx = hand.ctx();
        ctx.raise(played(fixtures::HAND_UNIT, 0, KIND_UNIT, Origin::Hand));
        settle(&mut ctx).unwrap();
        assert_eq!(his_triggers(&ctx), 0, "from anywhere other than your hand");
        ctx.raise(played(fixtures::THEIR_UNIT, 1, KIND_UNIT, Origin::Champion));
        settle(&mut ctx).unwrap();
        assert_eq!(
            his_triggers(&ctx),
            0,
            "when you play · the opponent's play is theirs"
        );
    }

    #[test]
    fn empowered_he_gives_the_chosen_unit_assault_two_until_the_turn_ends() {
        let mut fixture = storm(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::offers(&ctx, 0)
                .iter()
                .map(|offer| (offer.label.clone(), offer.enabled))
                .collect::<Vec<_>>(),
            [(
                format!("{{card {KENNEN}}}: give a unit Assault 2 (exhaust)"),
                true
            )]
        );
        activate::activate(&mut ctx, 0, KENNEN, 1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                "cancel".to_string()
            ],
            "a unit · anyone's"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.card(KENNEN).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "no rune is touched");
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Assault(0)));
        resolve_all(&mut ctx);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Assault(ASSAULT)));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "Assault counts while attacking"
        );
        ctx.mark_attacker(fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} has Assault 2 this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Assault(ASSAULT)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn unempowered_he_is_neither_offered_nor_activatable_and_exhausted_he_is_refused() {
        let mut fixture = storm(false);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, KENNEN, 1),
            Err(Refusal::Illegal(Reason::NotEmpowered)),
            "disempower me cannot be paid while not Empowered"
        );
        assert!(!ctx.card(KENNEN).unwrap().exhausted);
        drop(ctx);
        let mut spent = storm(true);
        spent.table.card_mut(KENNEN).unwrap().exhausted = true;
        let mut ctx = spent.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, KENNEN, 1),
            Err(Refusal::Exhausted)
        );
        drop(ctx);
        let mut theirs = storm(true);
        theirs.blob.core_mut().unwrap().player = 1;
        let mut ctx = theirs.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, KENNEN, 1),
            Err(Refusal::NotYourTurn),
            "an Action outside a showdown needs your turn"
        );
    }

    #[test]
    fn a_play_from_the_champion_zone_empowers_him_once_the_trigger_resolves() {
        let mut fixture = storm(false);
        let mut ctx = fixture.ctx();
        ctx.raise(played(
            fixtures::CHAMPION_CARD,
            0,
            KIND_UNIT,
            Origin::Champion,
        ));
        settle(&mut ctx).unwrap();
        resolve_all(&mut ctx);
        assert!(ctx.is_empowered(KENNEN));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Empowered { card, .. } if *card == KENNEN)));
    }

    #[test]
    #[ignore = "engine gap · Event::PlayedSpell carries the chain item id and no origin, and the item is off the chain by then, so a Flow spell played from the trash is invisible to the condition; with the origin on the event, a spell from the trash queues his trigger"]
    fn a_flow_spell_from_the_trash_queues_his_trigger() {
        let mut fixture = storm(false);
        let mut ctx = fixture.ctx();
        ctx.raise(Event::PlayedSpell {
            item: 1,
            controller: 0,
            nth: 1,
        });
        settle(&mut ctx).unwrap();
        assert_eq!(his_triggers(&ctx), 1);
    }

    #[test]
    fn the_activation_disempowers_him_as_its_cost_before_the_chain() {
        let mut fixture = storm(true);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, KENNEN, 1).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(
            !ctx.is_empowered(KENNEN),
            "the cost is paid at finalization"
        );
        resolve_all(&mut ctx);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Assault(ASSAULT)));
        assert!(!ctx.is_empowered(KENNEN));
    }
}
