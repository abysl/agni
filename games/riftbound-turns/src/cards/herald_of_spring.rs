use super::prelude::{done, gain_xp, play, unit};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const HUNT: u8 = 1;
pub const XP: u8 = 2;

fn herald(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    gain_xp(ctx, item.controller, XP);
    done()
}

pub static CARD: Card = unit(
    "Herald of Spring",
    &[Keyword::Hunt(HUNT)],
    &[play(&[], herald)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger, IMPLICIT_HUNT};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, settle, triggers};
    use crate::rules::COUNTER_XP;
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const HERALD: u32 = 90;

    fn herald_card(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::unit(HERALD, zone, seat, "Herald of Spring", 4)
        }
    }

    fn grove(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(herald_card(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(100, 0, "Calm", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(HERALD).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_hunt_one_and_one_untargeted_play_trigger() {
        assert!(std::ptr::eq(script_of("Herald of Spring").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hunt(1)]);
        assert_eq!(CARD.hunt(), 1);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_none());
        assert!(CARD.statics.is_empty());
        assert_eq!(XP, 2);
    }

    #[test]
    fn playing_it_puts_the_trigger_on_the_chain_and_two_xp_land_when_it_resolves() {
        let mut fixture = grove(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 4);
        fixtures::play_from_hand(&mut ctx, 0, HERALD).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(ctx.on_board(HERALD));
        assert_eq!(
            ctx.location(HERALD),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.blob.prompt.is_none(),
            "the herald asks nothing of its own"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == HERALD
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(ctx.xp(0), 0, "the XP waits for the trigger to resolve");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 2);
        assert_eq!(ctx.xp(1), 0, "only its controller gains");
        assert!(ctx.effects.contains(&Effect::score(0, COUNTER_XP, 2)));
        assert!(ctx.blob.log.contains(&"{seat 0} gains 2 XP".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn on_three_ready_runes_the_play_is_refused_and_nothing_is_gained() {
        let mut fixture = grove(fixtures::HAND);
        fixture.table.card_mut(100).unwrap().exhausted = true;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, HERALD),
            Err(crate::Refusal::NotEnoughRunes {
                needed: 4,
                ready: 3
            })
        );
        assert!(!ctx.on_board(HERALD));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.xp(0), 0);
    }

    #[test]
    fn holding_hunts_one_xp_and_the_play_trigger_does_not_fire_again() {
        let mut fixture = grove(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.hunt_value(HERALD), 1);
        ctx.raise(Event::Held {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![HERALD],
        });
        assert_eq!(triggers::collect(&mut ctx), 1);
        assert!(matches!(
            ctx.blob.queue[0].item.kind,
            ItemKind::Trigger { source, index } if source == HERALD && index == IMPLICIT_HUNT
        ));
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the hunt alone, not the play trigger"
        );
        resolve_chain(&mut ctx);
        assert_eq!(ctx.xp(0), 1);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }
}
