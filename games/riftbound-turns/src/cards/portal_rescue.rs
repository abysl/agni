use super::prelude::{a_friendly_unit, banish_by, card_target, done, play, spell, Location};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::play as play_engine;
use crate::state::Origin;

fn rescue(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let seat = item.controller;
    let token = ctx.is_token(unit);
    if !banish_by(ctx, unit, seat) {
        return done();
    }
    if token {
        return done();
    }
    if ctx.owner(unit) != seat {
        ctx.set_controller(unit, seat, item.kind.source());
    }
    let _ = play_engine::begin(
        ctx,
        seat,
        unit,
        Origin::Banishment,
        Some(Location::Base(seat)),
    );
    done()
}

pub static CARD: Card = spell(
    "Portal Rescue",
    &[Keyword::Action],
    &[play(
        &[a_friendly_unit("a friendly unit to banish and replay")],
        rescue,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{stun, FRIENDLY_UNIT};
    use crate::cards::script_of;
    use crate::cards::the_harrowing::tests::DRAWS;
    use crate::engine::ctx::{Event, MoveCause};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;

    const RESCUE: u32 = 90;
    const THEIR_RESCUE: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        for (id, seat) in [(RESCUE, 0), (THEIR_RESCUE, 1)] {
            let mut rescue = fixtures::spell(id, fixtures::HAND, seat, "Portal Rescue", 0, 0);
            rescue.domain = vec!["Mind".into()];
            fixture.table.cards.push(rescue);
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::VI).unwrap().name = "Crab".into();
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(fixtures::VI, &DRAWS);
        fixture
    }

    #[test]
    fn the_script_is_an_action_over_a_friendly_unit() {
        assert!(std::ptr::eq(script_of("Portal Rescue").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let targets = CARD.abilities[0].targets;
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].filter, FRIENDLY_UNIT);
        assert_eq!((targets[0].min, targets[0].max), (1, 1));
    }

    #[test]
    fn a_stunned_unit_at_a_battlefield_is_banished_and_replayed_to_base_fresh_and_free() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        stun(&mut ctx, fixtures::VI);
        ctx.damage(
            fixtures::VI,
            2,
            crate::engine::ctx::Cause::Cleanup { last_item: None },
        );
        let hand = ctx.hand_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, RESCUE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "cancel".to_string()],
            "only the friendly unit"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {}}} is banished", fixtures::VI)));
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(!ctx.is_stunned(fixtures::VI), "a new object");
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.banished_of(0).is_empty());
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "ignoring its cost");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Banishment, .. } if *card == fixtures::VI
        )));
        assert_eq!(ctx.blob.chain.len(), 1, "its play trigger fires again");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == fixtures::VI
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + 1);
        assert_eq!(ctx.card(RESCUE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_lone_attacker_rescued_mid_showdown_leaves_the_battlefield_uncontested() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().seat = 0;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            fixtures::VI,
            Location::Battlefield(fixtures::BF2),
            MoveCause::Effect,
        );
        crate::engine::cleanup::run(&mut ctx, None);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.showdown.is_some(), "a combat showdown at BF2");
        assert!(ctx.is_attacker(fixtures::VI));
        fixtures::play_from_hand(&mut ctx, 0, RESCUE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(!ctx.is_attacker(fixtures::VI));
        assert_eq!(ctx.blob.contester(fixtures::BF2), None);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_token_is_banished_for_good_and_the_opponent_cannot_play_it_on_my_turn() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::SPRITE).unwrap().seat = 0;
        fixture.table.card_mut(fixtures::SPRITE).unwrap().owner = 0;
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESCUE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.card(fixtures::SPRITE).is_none(), "the token is gone");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        let mut fixture = armed();
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: THEIR_RESCUE,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::NotYourTurn),
            "an Action outside a showdown is the turn player's"
        );
    }
}
