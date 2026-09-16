use super::faithful_manufactor::play_recruits_here;
use super::prelude::{done, legion, play, unit, when};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const RECRUITS: usize = 2;

fn legion_on_entry(ctx: &Ctx, _: &Event, source: Source) -> bool {
    legion(ctx, ctx.controller(source.card))
}

fn muster(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    play_recruits_here(ctx, item, RECRUITS);
    done()
}

pub static CARD: Card = unit(
    "Vanguard Captain",
    &[Keyword::Legion],
    &[when(play(&[], muster), legion_on_entry)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::faithful_manufactor::tests::{order_unit, recruits_of, with_order_runes};
    use crate::cards::faithful_manufactor::RECRUIT_MIGHT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;

    const CAPTAIN: u32 = 90;

    fn barracks(legion_on: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut captain = order_unit(CAPTAIN, fixtures::HAND, 0, "Vanguard Captain", 3);
        captain.power = Some(1);
        fixture.table.cards.push(captain);
        with_order_runes(&mut fixture, 0);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.blob.seat_mut(0).played_main = legion_on;
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(CAPTAIN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn deploy(ctx: &mut Ctx, at: Location) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, CAPTAIN, Origin::Hand, Some(at))?;
        settle(ctx)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_a_legion_unit_whose_play_trigger_is_gated_on_legion() {
        assert!(std::ptr::eq(script_of("Vanguard Captain").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Legion]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_some(), "812.1.b.1 · the Legion gate");
        assert_eq!(RECRUITS, 2);
    }

    #[test]
    fn with_legion_on_the_captain_plays_two_exhausted_recruits_where_she_was_played() {
        let mut fixture = barracks(true);
        let action = fixtures::move_action(CAPTAIN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(legion(&ctx, 0));
        deploy(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CAPTAIN
        ));
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert!(recruits_of(&ctx, 0).is_empty(), "both wait for the trigger");
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let recruits = recruits_of(&ctx, 0);
        assert_eq!(recruits, [next, next + 1]);
        for recruit in &recruits {
            assert_eq!(
                ctx.location(*recruit),
                Some(Location::Battlefield(fixtures::BF1))
            );
            assert!(ctx.is_token(*recruit));
            assert_eq!(ctx.current_might(*recruit), i32::from(RECRUIT_MIGHT));
            assert!(ctx.card(*recruit).unwrap().exhausted);
            assert!(ctx.effects.contains(&Effect::exhaust(*recruit)));
        }
        assert_eq!(
            ctx.effects
                .iter()
                .filter(|effect| matches!(effect, Effect::Spawn { .. }))
                .count(),
            2
        );
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [CAPTAIN, recruits[0], recruits[1]]
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn as_the_first_card_of_the_turn_the_captain_triggers_nothing() {
        let mut fixture = barracks(false);
        let action = fixtures::move_action(CAPTAIN, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert!(!legion(&ctx, 0));
        deploy(&mut ctx, Location::Base(0)).unwrap();
        assert!(ctx.on_board(CAPTAIN));
        assert!(ctx.blob.chain.is_empty(), "no Legion, no trigger");
        assert!(ctx.blob.prompt.is_none());
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(!ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Spawn { .. })));
        assert!(
            legion(&ctx, 0),
            "her own play turns Legion on for whatever follows"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_captain_bounced_in_response_has_no_here_and_musters_nobody() {
        let mut fixture = barracks(true);
        let action = fixtures::move_action(CAPTAIN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        deploy(&mut ctx, Location::Battlefield(fixtures::BF1)).unwrap();
        assert!(ctx.bounce(CAPTAIN));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger outlives its source");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(recruits_of(&ctx, 0).is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {CAPTAIN}}} has left the board · no Recruit"
        )));
    }
}
