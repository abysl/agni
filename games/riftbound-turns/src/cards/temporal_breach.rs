use super::prelude::{a_unit, banish_by, card_target, done, play, spell, Location, HIDDEN};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::play as play_engine;
use crate::state::Origin;

fn breach(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let Some(there) = ctx.location(unit) else {
        return done();
    };
    let owner = ctx.owner(unit);
    let token = ctx.is_token(unit);
    if !banish_by(ctx, unit, item.controller) {
        return done();
    }
    if token {
        return done();
    }
    if let Location::Battlefield(zone) = there {
        if !ctx.units_played_here(zone) {
            ctx.narrate(format!(
                "{{card {unit}}} stays banished · units can't be played at {{zone {zone}}}"
            ));
            return done();
        }
    }
    let _ = play_engine::begin(ctx, owner, unit, Origin::Banishment, Some(there));
    done()
}

pub static CARD: Card = spell(
    "Temporal Breach",
    HIDDEN,
    &[play(&[a_unit("a unit to banish and replay")], breach)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{stun, unit as unit_card};
    use crate::cards::{script_of, Keyword};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{hide, priority, settle};
    use crate::state::{ItemKind, PromptWhy};

    const BREACH: u32 = 90;
    const VEXLIKE: u32 = 91;

    static STUNNER: Card = unit_card(
        "Stunner",
        &[],
        &[crate::cards::prelude::on_opponent_plays_unit(
            &[],
            |ctx, item, _| {
                if let Some(played) = crate::cards::prelude::trigger_subject(item) {
                    stun(ctx, played);
                }
                Flow::Done
            },
        )],
    );

    const PLAIN: u32 = 54;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        let mut breach = fixtures::spell(BREACH, fixtures::HAND, 0, "Temporal Breach", 0, 0);
        breach.domain = vec!["Mind".into()];
        fixture.table.cards.push(breach);
        fixture.table.card_mut(fixtures::SPRITE).unwrap().name = "Jinx".into();
        fixture.table.card_mut(fixtures::SPRITE).unwrap().energy = Some(0);
        fixture.table.tokens.clear();
        fixture.resolve();
        fixture
    }

    #[test]
    fn a_unit_at_a_held_battlefield_is_banished_and_its_owner_replays_it_there_exhausted() {
        assert!(std::ptr::eq(script_of("Temporal Breach").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Hidden));
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::SPRITE).unwrap().exhausted = false;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREACH).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.log.contains(&"{card 60} is banished".to_string()));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{seat 1} plays {card 60} to {zone 10}"));
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(ctx.card(fixtures::SPRITE).unwrap().exhausted);
        assert!(ctx.banished_of(1).is_empty());
        assert!(
            ctx.events
                .iter()
                .all(|event| !matches!(event, crate::engine::ctx::Event::Moved { .. })),
            "a replay is a play, not a move"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF2), Some(1));
        assert_eq!(ctx.blob.contester(fixtures::BF2), None);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            crate::engine::ctx::Event::Played { card, controller: 1, origin: Origin::Banishment, .. }
                if *card == fixtures::SPRITE
        )));
        assert_eq!(ctx.blob.seat(1).cards_played, 1);
        assert_eq!(ctx.card(BREACH).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_banished_token_ceases_to_exist_and_is_not_replayed() {
        let mut fixture = armed();
        fixture.table.tokens.push(fixtures::SPRITE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREACH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        assert!(ctx.card(fixtures::SPRITE).is_none(), "the token is gone");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.queue.is_empty());
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("plays {card 60}")));
        assert_eq!(ctx.blob.seat(1).cards_played, 0);
        assert_eq!(ctx.card(BREACH).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn the_replay_fires_play_triggers_again_and_rockfall_path_leaves_the_unit_banished() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::unit(VEXLIKE, fixtures::BF1, 0, "Stunner", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(VEXLIKE, &STUNNER);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREACH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert!(
            ctx.is_stunned(fixtures::SPRITE),
            "the opponent's watcher sees a new play"
        );
        let mut rockfall = armed();
        rockfall.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF2);
        rockfall.resolve();
        let mut ctx = rockfall.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREACH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.banished_of(1), [fixtures::SPRITE]);
        assert!(ctx.blob.queue.is_empty());
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
    }

    #[test]
    fn from_facedown_only_the_hiding_battlefield_is_in_reach_and_a_lone_attacker_is_replayed_contesting(
    ) {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        hide::hide(&mut ctx, 0, BREACH, fixtures::BF1).unwrap();
        assert_eq!(
            hide::play_legal(&ctx, 0, BREACH),
            Err(crate::Refusal::Illegal(
                crate::engine::legal::Reason::HiddenThisTurn
            ))
        );
        ctx.blob.card_state_mut(BREACH).hidden_since = 0;
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        hide::play_from_facedown(&mut ctx, 0, BREACH).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "cancel"],
            "Vi at base and the enemy at base are out of reach from the hiding battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.contester(fixtures::BF1), Some(1));
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Spell { card }) if card == fixtures::HAND_SPELL
        ));
    }
}
