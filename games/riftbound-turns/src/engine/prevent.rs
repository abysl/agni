use crate::engine::ctx::{Cause, Ctx};
use crate::state::{Amount, DamageSource, Prevention};

pub fn covers(source: DamageSource, cause: Cause) -> bool {
    match source {
        DamageSource::Any => true,
        DamageSource::SpellOrAbility => matches!(
            cause,
            Cause::Item(_) | Cause::Ability(_) | Cause::Cleanup { last_item: Some(_) }
        ),
        DamageSource::Combat => matches!(cause, Cause::Combat),
    }
}

fn shields(prevention: &Prevention, card: u32, cause: Cause) -> bool {
    prevention.unit.is_none_or(|unit| unit == card) && covers(prevention.source, cause)
}

fn reduce(preventions: &mut [Prevention], card: u32, n: u8, cause: Cause) -> u8 {
    let mut left = n;
    for prevention in preventions.iter_mut() {
        if left == 0 {
            break;
        }
        if !shields(prevention, card, cause) {
            continue;
        }
        match prevention.value {
            Amount::All => left = 0,
            Amount::Next => {
                left = 0;
                prevention.value = Amount::N(0);
            }
            Amount::N(held) => {
                let taken = held.min(left);
                left -= taken;
                prevention.value = Amount::N(held - taken);
            }
        }
    }
    left
}

pub fn amount(ctx: &Ctx, card: u32, n: u8, cause: Cause) -> u8 {
    let mut preview = ctx.blob.preventions.clone();
    reduce(&mut preview, card, n, cause)
}

pub fn spend(ctx: &mut Ctx, card: u32, n: u8, cause: Cause) -> u8 {
    let left = reduce(&mut ctx.blob.preventions, card, n, cause);
    ctx.blob
        .preventions
        .retain(|prevention| !prevention.value.spent());
    if left > 0 && left < n {
        ctx.narrate(format!("{{card {card}}}: {} damage prevented", n - left));
    }
    left
}

pub fn unbounded(ctx: &Ctx, card: u32, cause: Cause) -> bool {
    ctx.blob.preventions.iter().any(|prevention| {
        shields(prevention, card, cause) && matches!(prevention.value, Amount::All | Amount::Next)
    })
}

pub fn held(ctx: &Ctx, card: u32, cause: Cause) -> u8 {
    ctx.blob
        .preventions
        .iter()
        .filter(|prevention| shields(prevention, card, cause))
        .map(|prevention| match prevention.value {
            Amount::N(n) => n,
            Amount::All | Amount::Next => 0,
        })
        .fold(0, u8::saturating_add)
}

pub fn active(ctx: &Ctx, cause: Cause) -> bool {
    ctx.blob
        .preventions
        .iter()
        .any(|prevention| covers(prevention.source, cause) && !prevention.value.spent())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, deal, play, spell, triggered, unit};
    use crate::cards::{Card, Flow, Keyword, Source, Trigger, Who};
    use crate::engine::ctx::{Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, expiry, phases, priority, settle, showdown};
    use crate::state::{Expiry, When};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::Target;

    static SINGULAR: Card = spell(
        "Singular",
        &[Keyword::Reaction],
        &[play(&[prelude::a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = prelude::card_target(ctx, item, 0) {
                deal(ctx, item, unit, 6);
            }
            Flow::Done
        })],
    );

    static FLINCHER: Card = unit(
        "Flincher",
        &[],
        &[triggered(Trigger::Damaged(Who::Me), &[], |ctx, item, _| {
            prelude::draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    const TARGET: u32 = 90;
    const STRIKER: u32 = 91;

    fn damage_of(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn shielded() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(TARGET, fixtures::BF1, 1, "Flincher", 4));
        fixture.table.card_mut(fixtures::HAND_SPELL).unwrap().name = "Singular".into();
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(TARGET, &FLINCHER)
            .with_script(fixtures::HAND_SPELL, &SINGULAR);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture
    }

    #[test]
    fn a_source_covers_the_causes_the_rules_group_under_it() {
        let item = Cause::Item(3);
        let ability = Cause::Ability(Source {
            card: 9,
            ability: 0,
        });
        let attributed = Cause::Cleanup { last_item: Some(3) };
        let plain = Cause::Cleanup { last_item: None };
        for cause in [item, ability, attributed] {
            assert!(covers(DamageSource::SpellOrAbility, cause), "{cause:?}");
            assert!(!covers(DamageSource::Combat, cause), "{cause:?}");
            assert!(covers(DamageSource::Any, cause), "{cause:?}");
        }
        assert!(covers(DamageSource::Combat, Cause::Combat));
        assert!(!covers(DamageSource::SpellOrAbility, Cause::Combat));
        for cause in [plain, Cause::Rule, Cause::Cost, Cause::Replacement] {
            assert!(!covers(DamageSource::SpellOrAbility, cause), "{cause:?}");
            assert!(!covers(DamageSource::Combat, cause), "{cause:?}");
            assert!(covers(DamageSource::Any, cause), "{cause:?}");
        }
    }

    #[test]
    fn all_prevents_everything_from_a_spell_and_the_damaged_trigger_stays_silent() {
        let mut fixture = shielded();
        let mut ctx = fixture.ctx();
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::All,
            Expiry::EndOfTurn(ctx.turn()),
        );
        assert!(active(&ctx, Cause::Item(1)));
        assert!(!active(&ctx, Cause::Combat));
        assert_eq!(amount(&ctx, TARGET, 6, Cause::Item(1)), 0);
        assert_eq!(amount(&ctx, TARGET, 6, Cause::Combat), 6);
        let hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {TARGET}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert_eq!(damage_of(&ctx, TARGET), 0, "437.4 · nothing is marked");
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::DamageDealt { .. })),
            "no DamageDealt is raised for a wholly prevented hit"
        );
        assert_eq!(
            ctx.hand_of(1).len(),
            hand,
            "the Damaged trigger never fired"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {TARGET}}}: the damage is prevented")));
        assert!(ctx.on_board(TARGET));
        assert_eq!(ctx.blob.preventions.len(), 1, "All is never spent");
    }

    #[test]
    fn combat_damage_in_the_same_turn_is_dealt_and_the_entry_is_gone_next_turn() {
        let mut fixture = shielded();
        fixture
            .table
            .cards
            .push(fixtures::unit(STRIKER, fixtures::BF1, 0, "Striker", 3));
        fixture.resolve();
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let this_turn = Expiry::EndOfTurn(ctx.turn());
        ctx.prevent(DamageSource::SpellOrAbility, Amount::All, this_turn);
        cleanup::run(&mut ctx, None);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert!(
            ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Counter { target: Target::Card(card), counter, delta }
                    if *card == TARGET && *counter == COUNTER_DAMAGE && *delta == 3
            )),
            "combat damage is not spell or ability damage: {:?}",
            ctx.effects
        );
        assert_eq!(ctx.blob.preventions.len(), 1);
        expiry::at_expiration(&mut ctx);
        assert!(
            ctx.blob.preventions.is_empty(),
            "the this-turn entry lapses at Expiration"
        );
        let mut fresh = shielded();
        let mut ctx = fresh.ctx();
        let this_turn = Expiry::EndOfTurn(ctx.turn());
        ctx.prevent(DamageSource::SpellOrAbility, Amount::All, this_turn);
        phases::end_turn(&mut ctx).unwrap();
        assert!(ctx.blob.preventions.is_empty());
        assert_eq!(amount(&ctx, TARGET, 6, Cause::Item(1)), 6);
    }

    #[test]
    fn a_numeric_entry_reduces_five_to_two_and_is_spent() {
        let mut fixture = shielded();
        let mut ctx = fixture.ctx();
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::N(3),
            Expiry::EndOfTurn(ctx.turn()),
        );
        assert_eq!(amount(&ctx, TARGET, 5, Cause::Item(1)), 2);
        assert_eq!(
            ctx.blob.preventions[0].value,
            Amount::N(3),
            "a preview spends nothing"
        );
        assert_eq!(spend(&mut ctx, TARGET, 5, Cause::Item(1)), 2);
        assert!(
            ctx.blob.preventions.is_empty(),
            "437.3 · three points spent, the entry is used up"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {TARGET}}}: 3 damage prevented")));
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::N(3),
            Expiry::EndOfTurn(ctx.turn()),
        );
        assert_eq!(spend(&mut ctx, TARGET, 2, Cause::Item(1)), 0);
        assert_eq!(
            ctx.blob.preventions[0].value,
            Amount::N(1),
            "one point is left for the next hit"
        );
        assert_eq!(spend(&mut ctx, TARGET, 4, Cause::Combat), 4);
        assert_eq!(ctx.blob.preventions[0].value, Amount::N(1));
        ctx.prevent(
            DamageSource::Any,
            Amount::N(1),
            Expiry::EndOfTurn(ctx.turn()),
        );
        assert_eq!(
            spend(&mut ctx, TARGET, 4, Cause::Item(2)),
            2,
            "entries stack in order"
        );
        assert!(ctx.blob.preventions.is_empty());
    }

    #[test]
    fn real_damage_counts_a_numeric_entry_down_instead_of_previewing_it() {
        let mut fixture = shielded();
        let mut ctx = fixture.ctx();
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::N(3),
            Expiry::EndOfTurn(ctx.turn()),
        );
        assert!(!ctx.damage(TARGET, 2, Cause::Item(1)), "fully prevented");
        assert_eq!(damage_of(&ctx, TARGET), 0);
        assert_eq!(ctx.blob.preventions[0].value, Amount::N(1));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TARGET}}}: the damage is prevented")));
        assert!(ctx.damage(TARGET, 3, Cause::Item(1)));
        assert_eq!(
            damage_of(&ctx, TARGET),
            2,
            "437.3: one point left, two get through"
        );
        assert!(ctx.blob.preventions.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {TARGET}}}: 1 damage prevented")));
        assert!(ctx.damage(TARGET, 1, Cause::Item(1)));
        assert_eq!(damage_of(&ctx, TARGET), 3, "the entry is gone");
    }

    #[test]
    fn a_shield_on_one_unit_spends_only_on_that_unit_and_lapses_at_expiration() {
        let mut fixture = shielded();
        fixture
            .table
            .cards
            .push(fixtures::unit(STRIKER, fixtures::BF1, 0, "Striker", 3));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let this_turn = Expiry::EndOfTurn(ctx.turn());
        ctx.prevent_on(TARGET, DamageSource::Any, Amount::Next, this_turn);
        assert!(active(&ctx, Cause::Combat));
        assert_eq!(amount(&ctx, TARGET, 9, Cause::Combat), 0);
        assert_eq!(
            amount(&ctx, STRIKER, 9, Cause::Combat),
            9,
            "437.2 · tracked on the unit"
        );
        assert!(crate::engine::combat::exempt(&ctx, TARGET));
        assert!(!crate::engine::combat::exempt(&ctx, STRIKER));
        assert_eq!(spend(&mut ctx, STRIKER, 2, Cause::Item(1)), 2);
        assert_eq!(
            ctx.blob.preventions.len(),
            1,
            "another unit's hit spends nothing"
        );
        assert!(
            !ctx.damage(TARGET, 5, Cause::Item(1)),
            "the next instance is prevented in full"
        );
        assert!(
            ctx.blob.preventions.is_empty(),
            "437.3.a · Next is used up by one instance"
        );
        assert!(ctx.damage(TARGET, 1, Cause::Item(1)));
        assert_eq!(damage_of(&ctx, TARGET), 1);
        ctx.prevent_on(TARGET, DamageSource::Any, Amount::N(7), this_turn);
        assert_eq!(
            crate::engine::combat::lethal(&ctx, TARGET),
            10,
            "437.5.a · three needed plus seven shielded"
        );
        assert_eq!(amount(&ctx, STRIKER, 4, Cause::Combat), 4);
        assert!(!ctx.damage(TARGET, 4, Cause::Combat));
        assert_eq!(ctx.blob.preventions[0].value, Amount::N(3));
        assert!(ctx.damage(TARGET, 5, Cause::Combat));
        assert_eq!(
            damage_of(&ctx, TARGET),
            3,
            "437.3 · three more prevented, two land"
        );
        assert!(ctx.blob.preventions.is_empty());
        ctx.prevent_on(TARGET, DamageSource::Any, Amount::Next, this_turn);
        ctx.prevent_on(STRIKER, DamageSource::Combat, Amount::N(2), this_turn);
        expiry::at_expiration(&mut ctx);
        assert!(ctx.blob.preventions.is_empty(), "both lapse at Expiration");
    }

    #[test]
    fn a_kill_attributed_to_an_item_under_prevention_marks_nothing_and_fires_nothing_after() {
        let mut fixture = shielded();
        let mut ctx = fixture.ctx();
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::All,
            Expiry::EndOfTurn(ctx.turn()),
        );
        ctx.delay(When::AfterKillsBy(7), fixtures::VI, 0, 0, Vec::new());
        assert!(!ctx.damage(TARGET, 9, Cause::Cleanup { last_item: Some(7) }));
        assert_eq!(damage_of(&ctx, TARGET), 0);
        cleanup::run(&mut ctx, Some(7));
        assert!(ctx.on_board(TARGET));
        assert!(
            ctx.blob.delayed.is_empty(),
            "the after-kills watcher is dropped when the cleanup killed nothing"
        );
        assert!(ctx.blob.queue.is_empty());
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }
}
