use super::prelude::{done, play, spell, this_turn};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::{Amount, DamageSource};

fn prevent_all(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let until = this_turn(ctx);
    ctx.prevent(DamageSource::SpellOrAbility, Amount::All, until);
    ctx.narrate(format!(
        "{{card {}}}: all spell and ability damage is prevented this turn",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = spell(
    "Unyielding Spirit",
    &[Keyword::Reaction],
    &[play(&[], prevent_all)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, a_unit, deal};
    use crate::cards::{Card, Trigger, Who};
    use crate::engine::ctx::{Cause, EntryMove, Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cleanup, legal, phases, prevent, priority, settle, showdown};
    use crate::state::{Expiry, Prevention};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const SPIRIT: u32 = 90;
    const THEIR_SPIRIT: u32 = 91;
    const BLAST: u32 = 92;
    const WATCHER: u32 = 93;
    const STRIKER: u32 = 94;

    static BLAST_CARD: Card = spell(
        "Blast",
        &[Keyword::Reaction],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = prelude::card_target(ctx, item, 0) {
                deal(ctx, item, unit, 6);
            }
            Flow::Done
        })],
    );

    static WATCHER_CARD: Card = prelude::unit(
        "Watcher",
        &[],
        &[prelude::triggered(
            Trigger::Damaged(Who::Me),
            &[],
            |ctx, item, _| {
                prelude::draw(ctx, item.controller, 1);
                Flow::Done
            },
        )],
    );

    fn spirit(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Unyielding Spirit", 1, 1);
        card.domain = vec!["Body".into()];
        card
    }

    fn damage_of(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(spirit(SPIRIT, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::spell(BLAST, fixtures::HAND, 0, "Blast", 1, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(WATCHER, fixtures::BF1, 0, "Watcher", 4));
        fixture.table.card_mut(42).unwrap().domain = vec!["Body".into()];
        fixture.table.card_mut(42).unwrap().name = "Body Rune".into();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BLAST, &BLAST_CARD)
            .with_script(WATCHER, &WATCHER_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SPIRIT).unwrap(),
            &CARD
        ));
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

    fn cast_and_resolve(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, SPIRIT).unwrap();
        assert!(ctx.blob.prompt.is_none(), "the spirit targets nothing");
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_spirit_is_a_targetless_reaction_that_registers_a_this_turn_prevention() {
        assert_eq!(CARD.name, "Unyielding Spirit");
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_and_resolve(&mut ctx);
        assert_eq!(
            ctx.blob.preventions,
            [Prevention {
                unit: None,
                source: DamageSource::SpellOrAbility,
                value: Amount::All,
                until: Expiry::EndOfTurn(1),
            }]
        );
        assert_eq!(ctx.card(SPIRIT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SPIRIT}}}: all spell and ability damage is prevented this turn"
        )));
        assert!(prevent::active(&ctx, Cause::Item(9)));
        assert!(!prevent::active(&ctx, Cause::Combat));
    }

    #[test]
    fn a_blast_after_the_spirit_marks_nothing_and_the_damaged_trigger_stays_silent() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_and_resolve(&mut ctx);
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, BLAST).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {WATCHER}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert_eq!(
            damage_of(&ctx, WATCHER),
            0,
            "437.4 · six damage, none marked"
        );
        assert!(ctx.on_board(WATCHER));
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::DamageDealt { card, .. } if *card == WATCHER)),
            "no DamageDealt for a wholly prevented hit"
        );
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1,
            "Blast left the hand and the Damaged trigger drew nothing"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {WATCHER}}}: the damage is prevented")));
        assert_eq!(ctx.blob.preventions.len(), 1, "All is never used up");
    }

    #[test]
    fn combat_damage_in_the_same_turn_is_dealt_and_the_prevention_is_gone_next_turn() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::unit(STRIKER, fixtures::BF1, 1, "Striker", 3));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BLAST, &BLAST_CARD)
            .with_script(WATCHER, &WATCHER_CARD);
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        let mut ctx = fixture.ctx();
        cast_and_resolve(&mut ctx);
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.as_ref().is_some_and(|held| held.combat));
        showdown::pass(&mut ctx, 1).unwrap();
        showdown::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert!(
            ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Counter { target: Target::Card(card), counter, delta }
                    if *card == WATCHER && *counter == COUNTER_DAMAGE && *delta == 3
            )),
            "combat damage is not spell or ability damage: {:?}",
            ctx.effects
        );
        assert_eq!(ctx.card(STRIKER).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.blob.preventions.len(), 1);
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the Damaged trigger fired for combat damage"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.turn(), 2);
        assert!(ctx.blob.preventions.is_empty(), "this turn only");
        assert_eq!(prevent::amount(&ctx, WATCHER, 6, Cause::Item(1)), 6);
    }

    #[test]
    fn the_other_seat_cannot_play_it_and_a_seat_without_body_power_is_refused() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(spirit(THEIR_SPIRIT, fixtures::HAND, 1));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, SPIRIT)),
            Err(Refusal::Illegal(Reason::NotYourCard)),
            "seat 1 cannot play seat 0's spirit"
        );
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SPIRIT)),
            Err(Refusal::NotYourTurn),
            "a Reaction on the opponent's turn needs an open window"
        );
        drop(ctx);
        let mut broke = armed();
        broke.table.card_mut(42).unwrap().domain = vec!["Calm".into()];
        broke.table.card_mut(42).unwrap().name = "Calm Rune".into();
        let mut ctx = broke.ctx();
        assert_eq!(
            fixtures::play_from_hand(&mut ctx, 0, SPIRIT),
            Err(Refusal::NoPowerOf),
            "no Body rune, no Body power"
        );
        assert!(ctx.blob.preventions.is_empty());
        assert!(ctx.blob.chain.is_empty());
    }
}
