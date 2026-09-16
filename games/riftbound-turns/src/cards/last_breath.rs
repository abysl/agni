use super::prelude::{a_card, a_friendly_unit, card_target, deal, done, play, ready, spell};
use super::{Card, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const ENEMY_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::AtBattlefield]);

const STRIKER: usize = 0;
const VICTIM: usize = 1;

fn might_of(ctx: &Ctx, unit: u32) -> u8 {
    u8::try_from(ctx.current_might(unit).max(0)).unwrap_or(u8::MAX)
}

fn breathe(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(striker) = card_target(ctx, item, STRIKER) else {
        return done();
    };
    if ready(ctx, striker) {
        ctx.narrate(format!("{{card {striker}}} is readied"));
    }
    let Some(victim) = card_target(ctx, item, VICTIM) else {
        return done();
    };
    let amount = might_of(ctx, striker);
    ctx.narrate(format!(
        "{{card {striker}}} deals {amount} damage to {{card {victim}}}"
    ));
    deal(ctx, item, victim, amount);
    done()
}

pub static CARD: Card = spell(
    "Last Breath",
    &[Keyword::Action],
    &[play(
        &[
            a_friendly_unit("a friendly unit to ready"),
            a_card(
                ENEMY_UNIT_AT_A_BATTLEFIELD,
                "an enemy unit at a battlefield",
            ),
        ],
        breathe,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::cards::Trigger;
    use crate::engine::ctx::{EntryMove, Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const BREATH: u32 = 90;
    const THEIR_BREATH: u32 = 91;
    const BRUTE: u32 = 92;
    const CALM_RUNE: u32 = 100;
    const CHAOS_RUNE: u32 = 101;

    fn breath(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Last Breath", 3, 2);
        card.domain = vec!["Calm".into(), "Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(breath(BREATH, 0));
        fixture.table.cards.push(breath(THEIR_BREATH, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    fn damage(ctx: &Ctx, unit: u32) -> i32 {
        ctx.table
            .counter(Target::Card(unit), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt { card, n, .. } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_an_action_over_a_friendly_unit_and_an_enemy_unit_at_a_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Last Breath").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Last Breath");
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[STRIKER].filter, FRIENDLY_UNIT);
        assert_eq!(ability.targets[VICTIM].filter, ENEMY_UNIT_AT_A_BATTLEFIELD);
        for spec in ability.targets {
            assert_eq!((spec.min, spec.max), (1, 1));
        }
    }

    #[test]
    fn the_friendly_unit_readies_and_deals_its_might_to_the_enemy_at_a_battlefield() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREATH).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "the friendly units, exhausted or not"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "the enemy units at battlefields, never the one in its base"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(BRUTE)]
        );
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "nothing before it resolves"
        );
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(ctx.effects.contains(&Effect::ready(fixtures::VI)));
        assert!(ctx.events.contains(&Event::Readied {
            card: fixtures::VI,
            by: 0
        }));
        assert_eq!(damage_events(&ctx), [(BRUTE, 3)], "Vi's 3 Might");
        assert_eq!(damage(&ctx, BRUTE), 3);
        assert!(ctx.on_board(BRUTE), "3 on 5 Might is not lethal");
        assert_eq!(
            damage(&ctx, fixtures::VI),
            0,
            "the striker takes nothing back"
        );
        assert!(ctx.blob.log.contains(&"{card 50} is readied".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} deals 3 damage to {card 92}".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(BREATH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_lethal_breath_kills_the_enemy_at_the_cleanup_and_the_battlefield_falls_open() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREATH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(damage_events(&ctx), [(fixtures::SPRITE, 3)]);
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "3 on a 3-Might Sprite is lethal"
        );
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { card, .. } if *card == fixtures::SPRITE)));
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
    }

    #[test]
    fn a_striker_that_left_the_board_deals_nothing_and_a_victim_that_left_still_lets_it_ready() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREATH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::TRASH, 0), 0)
            .unwrap();
        both_pass(&mut ctx);
        assert!(damage_events(&ctx).is_empty());
        assert!(!ctx.effects.contains(&Effect::ready(fixtures::VI)));
        assert_eq!(ctx.card(BREATH).unwrap().zone, Some(fixtures::TRASH));
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BREATH).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        let base = ctx.zones.base.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BRUTE, base, 1), 1)
            .unwrap();
        both_pass(&mut ctx);
        assert!(
            !ctx.card(fixtures::VI).unwrap().exhausted,
            "the ready does not depend on the enemy"
        );
        assert!(
            damage_events(&ctx).is_empty(),
            "back in its base the Brute is no longer a legal target"
        );
        assert_eq!(damage(&ctx, BRUTE), 0);
    }

    #[test]
    fn enemies_in_their_base_and_friendly_victims_are_refused_and_the_action_waits_for_your_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_BREATH)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, BREATH).unwrap();
        for wrong in [fixtures::THEIR_UNIT, BRUTE, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit at a battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(BREATH).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "a cancelled play readies nothing"
        );
        assert!(ctx.blob.is_neutral_open());
    }
}
