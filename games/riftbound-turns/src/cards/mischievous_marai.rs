use super::prelude::{
    a_card, at_battlefield, card_target, deal, done, play, unit, when, ENEMY_UNIT_HERE, HIDDEN,
};
use super::{Card, Event, Flow, Item, Source, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;

pub const VICTIM: TargetSpec = a_card(ENEMY_UNIT_HERE, "an enemy unit here to deal 2 to");

fn played_to_a_battlefield(ctx: &Ctx, _: &Event, source: Source) -> bool {
    at_battlefield(ctx, source.card)
}

fn splash(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        deal(ctx, item, unit, DAMAGE);
    }
    done()
}

pub static CARD: Card = unit(
    "Mischievous Marai",
    HIDDEN,
    &[when(play(&[VICTIM], splash), played_to_a_battlefield)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, TargetKind, Trigger};
    use crate::engine::ctx::{Cause, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{act, hide, legal, play as play_engine, prompts, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Action, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const MARAI: u32 = 90;
    const THEIR_BRUTE: u32 = 91;
    const THEIR_SCOUT: u32 = 92;

    fn marai(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(0),
            domain: vec!["Fury".into()],
            ..fixtures::unit(MARAI, zone, seat, "Mischievous Marai", 2)
        }
    }

    fn reef() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(marai(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SCOUT, fixtures::BF1, 1, "Scout", 1));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn surface(ctx: &mut Ctx, to: Location) {
        play_engine::begin(ctx, 0, MARAI, Origin::Hand, Some(to)).unwrap();
        settle(ctx).unwrap();
    }

    fn target_item(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, spec: 0 }) => item,
            other => panic!("the play asks for an enemy unit here, not {other:?}"),
        }
    }

    #[test]
    fn the_script_is_a_hidden_unit_whose_conditional_play_trigger_targets_an_enemy_unit_here() {
        assert!(std::ptr::eq(script_of("Mischievous Marai").unwrap(), &CARD));
        assert_eq!(CARD.name, "Mischievous Marai");
        assert_eq!(CARD.keywords, [Keyword::Hidden]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(
            ability.condition.is_some(),
            "only when played to a battlefield"
        );
        assert_eq!(ability.targets, [VICTIM]);
        assert_eq!((VICTIM.min, VICTIM.max), (1, 1));
        assert_eq!(VICTIM.kind, TargetKind::Card);
        assert_eq!(VICTIM.filter, ENEMY_UNIT_HERE);
        assert_eq!(DAMAGE, 2);
        assert!(
            !hide::lifts(&[VICTIM], &VICTIM),
            "an enemy unit here can be found at the hiding battlefield"
        );
    }

    #[test]
    fn played_to_a_battlefield_she_offers_the_enemy_units_there_and_deals_two_to_the_pick() {
        let mut fixture = reef();
        let mut ctx = fixture.ctx();
        surface(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert_eq!(
            ctx.location(MARAI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let item = target_item(&ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_BRUTE}}}"),
                format!("{{card {THEIR_SCOUT}}}")
            ],
            "the Sprite and Jinx stand elsewhere"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item, spec: 0 }),
            format!("{{card {MARAI}}}: choose an enemy unit here to deal 2 to (0 of 1)")
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "an enemy unit at another battlefield is refused"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the target is not optional"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SCOUT}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MARAI
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_SCOUT)]);
        let chain_item = ctx.blob.chain[0].id;
        assert_eq!(
            ctx.damage_on(THEIR_SCOUT),
            0,
            "the damage waits for the trigger"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: THEIR_SCOUT,
            n: DAMAGE,
            source: Cause::Item(chain_item)
        }));
        assert!(
            !ctx.on_board(THEIR_SCOUT),
            "1 Might dies to 2 at the cleanup"
        );
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_to_base_the_trigger_never_fires() {
        let mut fixture = reef();
        let mut ctx = fixture.ctx();
        surface(&mut ctx, Location::Base(0));
        assert!(ctx.blob.prompt.is_none(), "no target is asked for");
        assert!(
            ctx.blob.chain.is_empty(),
            "the condition failed at the trigger"
        );
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.location(MARAI), Some(Location::Base(0)));
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 0);
    }

    #[test]
    fn played_from_hidden_she_hits_an_enemy_unit_at_her_battlefield() {
        let mut fixture = reef();
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        {
            let action = Action::Move {
                card: MARAI,
                to: Some(fixtures::BF1),
                seat: 0,
                index: TOP,
                hidden: true,
            };
            let mut ctx = fixture.ctx_for(0, &action);
            let entry = ctx.entry.unwrap();
            let intent = legal::classify(&ctx, 0, &entry).unwrap();
            assert_eq!(
                intent,
                legal::Intent::Hide {
                    card: MARAI,
                    zone: fixtures::BF1
                }
            );
            act(&mut ctx, 0, intent).unwrap();
            assert!(hide::is_facedown(&ctx, MARAI));
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        *fixture.table.card_mut(MARAI).unwrap() = CardInfo {
            zone: Some(fixtures::BF1),
            seat: 0,
            ..marai(fixtures::BF1, 0)
        };
        fixture.blob.core_mut().unwrap().turn += 1;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(hide::is_facedown(&ctx, MARAI));
        hide::play_from_facedown(&mut ctx, 0, MARAI).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(MARAI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        target_item(&ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_BRUTE}}}"),
                format!("{{card {THEIR_SCOUT}}}")
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(THEIR_BRUTE), 2);
        assert!(ctx.on_board(THEIR_BRUTE), "4 Might survives 2");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_enemy_unit_here_the_trigger_fizzles_and_she_still_lands() {
        let mut fixture = reef();
        fixture
            .table
            .cards
            .retain(|card| ![THEIR_BRUTE, THEIR_SCOUT].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        surface(&mut ctx, Location::Battlefield(fixtures::BF1));
        assert!(ctx.blob.prompt.is_none(), "nothing to ask");
        assert!(ctx.blob.chain.is_empty(), "390.3 · the trigger is removed");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MARAI}}} trigger fizzles · no legal target"
        )));
        assert_eq!(
            ctx.location(MARAI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
    }
}
