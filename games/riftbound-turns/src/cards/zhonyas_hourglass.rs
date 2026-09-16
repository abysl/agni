use super::prelude::{gear, recall, replaces, with_replacement};
use super::{Card, Keyword, Source, WouldDie};
use crate::engine::ctx::{Cause, Ctx};

fn a_friendly_unit_would_die(ctx: &Ctx, would: &WouldDie, source: Source) -> bool {
    ctx.is_unit(would.unit)
        && ctx.on_board(would.unit)
        && ctx.controller(would.unit) == ctx.controller(source.card)
}

fn kill_this_instead(ctx: &mut Ctx, would: &WouldDie, source: Source) {
    ctx.kill(source.card, Cause::Replacement);
    recall(ctx, would.unit, true);
    ctx.narrate(format!(
        "{{card {}}} is recalled exhausted instead of dying",
        would.unit
    ));
}

pub static CARD: Card = with_replacement(
    gear("Zhonya's Hourglass", &[Keyword::Hidden], &[]),
    replaces(a_friendly_unit_would_die, kill_this_instead),
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{
        at_battlefield, deathknell, done, draw, on_enemy_unit_dies, spawn_gold, unit, when,
    };
    use crate::cards::{Flow, Item, Stage};
    use crate::engine::ctx::{EntryMove, Event, Killed, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{act, cleanup, play, priority, prompts, settle, showdown};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::CardInfo;

    const GUARDED: u32 = 90;
    const KILLER: u32 = 91;
    const HOURGLASS: u32 = 92;
    const PYKE: u32 = 93;
    const SINGULARITY: u32 = 94;
    const MIND_RUNES: [u32; 6] = [100, 101, 102, 103, 104, 105];
    const DRAWS: usize = 2;

    fn wail(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
        draw(ctx, item.controller, DRAWS);
        done()
    }

    static WAILER: Card = unit("Wailer", &[Keyword::Deathknell], &[deathknell(&[], wail)]);

    fn gold_for_pyke(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
        spawn_gold(ctx, item.controller, false);
        done()
    }

    static PYKE_CARD: Card = unit(
        "Pyke - Returned",
        &[],
        &[when(
            on_enemy_unit_dies(&[], gold_for_pyke),
            |ctx, _, source| at_battlefield(ctx, source.card),
        )],
    );

    fn hourglass(zone: u16) -> CardInfo {
        let mut card = fixtures::gear(HOURGLASS, zone, 0, "Zhonya's Hourglass", 2);
        card.domain = vec!["Calm".into()];
        card
    }

    fn guarded(zone: u16) -> CardInfo {
        fixtures::unit(GUARDED, zone, 0, "Wailer", 3)
    }

    fn watched() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.cards.push(hourglass(fixtures::BASE));
        fixture
    }

    fn scripted(fixture: &mut Fixture) {
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(GUARDED, &WAILER)
            .with_script(PYKE, &PYKE_CARD);
    }

    fn watched_by_pyke() -> Fixture {
        let mut fixture = watched();
        fixture.table.cards.push(guarded(fixtures::BASE));
        fixture
            .table
            .cards
            .push(fixtures::unit(PYKE, fixtures::BF2, 1, "Pyke - Returned", 3));
        scripted(&mut fixture);
        fixture
    }

    fn golds_of(ctx: &Ctx, seat: u8) -> usize {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == "Gold" && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .count()
    }

    fn died(ctx: &Ctx, card: u32) -> bool {
        ctx.events
            .iter()
            .any(|event| matches!(event, Event::Died { card: dead, .. } if *dead == card))
    }

    fn triggers_about(ctx: &Ctx, card: u32) -> usize {
        let queued = ctx.blob.queue.iter().map(|pending| &pending.item);
        ctx.blob
            .chain
            .iter()
            .chain(queued)
            .filter(|item| matches!(item.kind, ItemKind::Trigger { .. }))
            .filter(|item| item.subject == Some(TargetRef::Card(card)))
            .count()
    }

    fn resolve_all(ctx: &mut Ctx) {
        for _ in 0..16 {
            let Some(seat) = priority::holder(ctx) else {
                break;
            };
            priority::pass(ctx, seat).unwrap();
        }
        assert!(ctx.blob.chain.is_empty(), "the chain ran dry");
    }

    #[test]
    fn the_card_is_a_hidden_gear_whose_whole_text_is_one_replacement() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Zhonya's Hourglass").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, &[Keyword::Hidden]);
        assert!(
            CARD.abilities.is_empty(),
            "a replacement is not a triggered ability"
        );
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_some());
    }

    #[test]
    fn it_replaces_a_combat_death_recalls_the_defender_exhausted_and_kills_itself() {
        let mut fixture = watched();
        fixture.table.cards.push(guarded(fixtures::BF1));
        fixture
            .table
            .cards
            .push(fixtures::unit(KILLER, fixtures::BF1, 1, "Jinx", 3));
        scripted(&mut fixture);
        fixture.blob.set_contested(fixtures::BF1, Some(1));
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        cleanup::run(&mut ctx, None);
        assert!(
            ctx.blob.showdown.as_ref().is_some_and(|held| held.combat),
            "two seats at a contested battlefield stage a combat"
        );
        showdown::pass(&mut ctx, 1).unwrap();
        showdown::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.showdown.is_none(), "the combat resolved");
        assert_eq!(
            ctx.location(GUARDED),
            Some(Location::Base(0)),
            "the defender is recalled, not trashed"
        );
        assert!(ctx.card(GUARDED).unwrap().exhausted);
        assert!(!died(&ctx, GUARDED), "the unit never died");
        assert_eq!(
            ctx.card(HOURGLASS).unwrap().zone,
            Some(fixtures::TRASH),
            "the gear is killed in its place"
        );
        assert!(died(&ctx, HOURGLASS));
        assert_eq!(ctx.hand_of(0).len(), hand, "no Deathknell draw");
        assert_eq!(triggers_about(&ctx, GUARDED), 0);
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 92} replaces the death of {card 90}"));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 90} is recalled exhausted instead of dying"));
    }

    #[test]
    fn an_enemy_unit_is_left_alone_and_the_next_time_means_once() {
        let mut fixture = watched();
        fixture.table.cards.push(guarded(fixtures::BASE));
        scripted(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.kill(fixtures::THEIR_UNIT, Cause::Rule),
            Killed::Yes,
            "the gear guards its own controller's units only"
        );
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.on_board(HOURGLASS), "an enemy death does not spend it");
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Replaced);
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(!ctx.on_board(HOURGLASS));
        let hand = ctx.hand_of(0).len();
        assert_eq!(
            ctx.kill(GUARDED, Cause::Rule),
            Killed::Yes,
            "the gear is gone, so the next friendly death is a death"
        );
        assert_eq!(ctx.card(GUARDED).unwrap().zone, Some(fixtures::TRASH));
        settle(&mut ctx).unwrap();
        resolve_all(&mut ctx);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand + DRAWS,
            "the real death runs the Deathknell"
        );
    }

    #[test]
    fn a_friendly_gear_is_not_a_friendly_unit() {
        let mut fixture = watched();
        fixture.table.cards.push(fixtures::gold(95, 0, false));
        scripted(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(95, Cause::Cost), Killed::Yes, "a Gold is a gear");
        assert!(ctx.on_board(HOURGLASS));
        assert_eq!(
            ctx.kill(fixtures::SPRITE, Cause::Rule),
            Killed::Yes,
            "an enemy token unit is nothing of seat 0's"
        );
        assert!(ctx.on_board(HOURGLASS));
        assert_eq!(ctx.kill(HOURGLASS, Cause::Rule), Killed::Yes);
        assert!(
            !ctx.blob
                .log
                .iter()
                .any(|line| line.contains("replaces the death")),
            "the gear does not replace its own death or another gear's"
        );
    }

    #[test]
    fn a_replaced_death_is_no_death_for_pyke_to_count() {
        let mut fixture = watched_by_pyke();
        let mut ctx = fixture.ctx();
        assert_eq!(golds_of(&ctx, 1), 0);
        assert_eq!(ctx.kill(GUARDED, Cause::Rule), Killed::Replaced);
        settle(&mut ctx).unwrap();
        assert_eq!(
            triggers_about(&ctx, GUARDED),
            0,
            "no enemy unit died, so nothing of Pyke's is keyed to the unit"
        );
        assert!(ctx.deaths.is_empty(), "and no Deathknell was queued");
        assert_eq!(ctx.location(GUARDED), Some(Location::Base(0)));
    }

    #[test]
    fn pyke_is_paid_nothing_at_all_when_the_hourglass_dies_in_a_units_place() {
        let mut fixture = watched_by_pyke();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.kill(GUARDED, Cause::Rule), Killed::Replaced);
        settle(&mut ctx).unwrap();
        resolve_all(&mut ctx);
        assert_eq!(golds_of(&ctx, 1), 0);
    }

    #[test]
    fn pyke_is_paid_for_the_death_the_gear_did_not_stop() {
        let mut fixture = watched_by_pyke();
        let mut ctx = fixture.ctx();
        ctx.kill(HOURGLASS, Cause::Rule);
        ctx.events.clear();
        assert_eq!(ctx.kill(GUARDED, Cause::Rule), Killed::Yes);
        settle(&mut ctx).unwrap();
        assert_eq!(triggers_about(&ctx, GUARDED), 2, "a Deathknell and a Gold");
        resolve_all(&mut ctx);
        assert_eq!(golds_of(&ctx, 1), 1);
    }

    fn armed() -> Fixture {
        let mut fixture = watched();
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1);
        for rune in MIND_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        let mut singularity = fixtures::spell(SINGULARITY, fixtures::HAND, 0, "Singularity", 6, 2);
        singularity.domain = vec!["Mind".into()];
        fixture.table.cards.push(singularity);
        fixture.table.cards.push(guarded(fixtures::BASE));
        scripted(&mut fixture);
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32, to: u16) -> EntryMove {
        EntryMove {
            card,
            from: ctx.card(card).and_then(|held| held.zone),
            from_seat: seat,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        let chain = ctx.zones.chain.unwrap_or(0);
        legal::classify(ctx, seat, &entry(ctx, seat, card, chain))?;
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let answered = prompts::answer(ctx, seat, Pick { prompt, option })?;
        if let Some(answered) = answered {
            let PromptWhy::Target { item, spec } = answered.why else {
                panic!("only target prompts here: {:?}", answered.why);
            };
            match answered.answer {
                Answer::Cancel => play::cancel(ctx, item),
                _ => play::choose_targets(ctx, item, spec, &answered.prompt.picked)?,
            }
        }
        settle(ctx)
    }

    #[test]
    fn it_replaces_a_singularity_death_and_the_damage_rides_home_with_the_unit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, SINGULARITY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        choose(&mut ctx, 0, "{card 90}").unwrap();
        choose(&mut ctx, 0, "done").unwrap();
        resolve_all(&mut ctx);
        assert!(ctx.blob.chain.is_empty(), "the spell resolved");
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.iter().any(|line| line
            == &format!("{{card {HOURGLASS}}} replaces the death of {{card {GUARDED}}}")));
        assert_eq!(
            ctx.card(GUARDED).unwrap().zone,
            Some(fixtures::TRASH),
            "321: the recall keeps the damage (436.1), so the repeated cleanup kills it for real"
        );
        assert!(died(&ctx, GUARDED));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1 + DRAWS,
            "the Hourglass only delayed this death, so the Wailer's Deathknell still fired"
        );
    }

    #[test]
    fn a_replaced_death_that_leaves_lethal_damage_is_not_saved_by_the_end_of_turn_heal() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.damage(GUARDED, 6, Cause::Rule);
        assert_eq!(cleanup::dying(&ctx), [GUARDED]);
        cleanup::run(&mut ctx, None);
        assert_eq!(
            ctx.card(HOURGLASS).unwrap().zone,
            Some(fixtures::TRASH),
            "the gear answers the first death"
        );
        assert_eq!(
            ctx.location(GUARDED),
            None,
            "and the second cleanup of the same window kills the unit it recalled"
        );
        assert!(
            cleanup::dying(&ctx).is_empty(),
            "the cleanup repeats until nothing else is dying (321)"
        );
    }

    #[test]
    fn the_gear_on_the_board_is_not_dragged_anywhere() {
        let mut fixture = watched();
        scripted(&mut fixture);
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, HOURGLASS, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::GearStays)),
            "a gear on the board is not dragged anywhere"
        );
    }

    #[test]
    fn hidden_for_a_rainbow_power_and_played_from_facedown_for_free() {
        let mut fixture = watched();
        scripted(&mut fixture);
        {
            let card = fixture.table.card_mut(HOURGLASS).unwrap();
            card.zone = Some(fixtures::HAND);
            card.name = String::new();
            card.kind = None;
            card.energy = None;
            card.domain.clear();
        }
        let runes = {
            let action = fixtures::move_action(HOURGLASS, fixtures::BF1, 0);
            let mut ctx = fixture.ctx_for(0, &action);
            let runes = ctx.runes_of(0).len();
            let hide = legal::classify(&ctx, 0, &ctx.entry.unwrap())
                .expect("a facedown hand card dragged onto a held battlefield is a hide");
            assert_eq!(
                hide,
                legal::Intent::Hide {
                    card: HOURGLASS,
                    zone: fixtures::BF1
                }
            );
            assert_eq!(
                act(&mut ctx, 0, hide),
                Ok(()),
                "M6 charges the [A] rainbow power and puts it face down"
            );
            assert_eq!(
                ctx.blob.card_state(HOURGLASS).and_then(|row| row.hidden_at),
                Some(fixtures::BF1)
            );
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
            scripted(&mut fixture);
            runes
        };
        assert_eq!(
            fixture
                .table
                .cards
                .iter()
                .filter(|card| card.zone == Some(fixtures::RUNE_POOL) && card.owner == 0)
                .count(),
            runes - 1,
            "the rainbow power recycles one rune"
        );
        fixture.blob.core_mut().unwrap().turn += 1;
        {
            let card = fixture.table.card_mut(HOURGLASS).unwrap();
            card.name = "Zhonya's Hourglass".into();
            card.kind = Some("Gear".into());
            card.energy = Some(2);
            card.domain = vec!["Calm".into()];
        }
        scripted(&mut fixture);
        let chain = fixtures::CHAIN;
        let action = fixtures::move_action(HOURGLASS, chain, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let played = legal::classify(&ctx, 0, &ctx.entry.unwrap())
            .expect("a facedown card dragged onto the chain is played from hidden");
        assert_eq!(played, legal::Intent::PlayFromFacedown { card: HOURGLASS });
        assert_eq!(
            act(&mut ctx, 0, played),
            Ok(()),
            "737: it is played from facedown for no energy"
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.location(HOURGLASS), Some(Location::Base(0)));
        assert_eq!(
            ctx.blob.card_state(HOURGLASS).and_then(|row| row.hidden_at),
            None,
            "it is no longer in the facedown zone"
        );
        assert!(ctx.has_flag(HOURGLASS, crate::state::FLAG_FROM_FACEDOWN));
    }

    fn hidden_face(fixture: &mut Fixture, shown: bool) {
        {
            let card = fixture.table.card_mut(HOURGLASS).unwrap();
            if shown {
                card.name = "Zhonya's Hourglass".into();
                card.kind = Some("Gear".into());
                card.energy = Some(2);
                card.domain = vec!["Calm".into()];
            } else {
                card.name = String::new();
                card.kind = None;
                card.energy = None;
                card.domain.clear();
            }
        }
        scripted(fixture);
    }

    fn hide_now(fixture: &mut Fixture, seat: u8, card: u32, zone: u16) -> Result<(), Refusal> {
        let action = fixtures::move_action(card, zone, 0);
        let mut ctx = fixture.ctx_for(seat, &action);
        let entry = ctx.entry.unwrap();
        let intent = legal::classify(&ctx, seat, &entry)?;
        assert_eq!(intent, legal::Intent::Hide { card, zone });
        let done = crate::engine::act(&mut ctx, seat, intent);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        scripted(fixture);
        done
    }

    fn in_hand() -> Fixture {
        let mut fixture = watched();
        fixture.table.card_mut(HOURGLASS).unwrap().zone = Some(fixtures::HAND);
        fixture.table.cards.push(guarded(fixtures::BASE));
        scripted(&mut fixture);
        fixture
    }

    #[test]
    fn from_facedown_the_gear_lands_in_the_base_and_still_guards_a_friendly_death() {
        let mut fixture = in_hand();
        hidden_face(&mut fixture, false);
        hide_now(&mut fixture, 0, HOURGLASS, fixtures::BF1).unwrap();
        assert_eq!(
            fixture
                .blob
                .card_state(HOURGLASS)
                .and_then(|row| row.hidden_at),
            Some(fixtures::BF1)
        );
        fixture.blob.core_mut().unwrap().turn += 1;
        hidden_face(&mut fixture, true);
        let action = fixtures::move_action(HOURGLASS, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        assert_eq!(intent, legal::Intent::PlayFromFacedown { card: HOURGLASS });
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "737.1.c.3 · a gear needs no chain"
        );
        assert_eq!(
            ctx.location(HOURGLASS),
            Some(Location::Base(0)),
            "737.1.d.1 pins a hidden unit to that battlefield; a gear still lands in the base"
        );
        assert!(!ctx.card(HOURGLASS).unwrap().exhausted);
        assert_eq!(
            ctx.blob.card_state(HOURGLASS).and_then(|row| row.hidden_at),
            None
        );
        assert!(ctx.has_flag(HOURGLASS, crate::state::FLAG_FROM_FACEDOWN));
        assert_eq!(
            ctx.kill(GUARDED, Cause::Rule),
            Killed::Replaced,
            "737.2 · the printed replacement is unchanged by the hidden play"
        );
        assert_eq!(ctx.location(GUARDED), Some(Location::Base(0)));
        assert!(ctx.card(GUARDED).unwrap().exhausted);
        assert!(!died(&ctx, GUARDED));
        assert_eq!(ctx.card(HOURGLASS).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.iter().any(|line| line
            == &format!("{{card {HOURGLASS}}} replaces the death of {{card {GUARDED}}}")));
    }

    #[test]
    fn hiding_the_hourglass_needs_a_held_battlefield_a_free_slot_and_a_ready_rune() {
        let mut unheld = Fixture::enforced();
        unheld.table.cards.push(hourglass(fixtures::HAND));
        scripted(&mut unheld);
        hidden_face(&mut unheld, false);
        assert_eq!(
            hide_now(&mut unheld, 0, HOURGLASS, fixtures::BF1),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "737.1.b · nobody holds that battlefield"
        );
        assert_eq!(
            hide_now(&mut unheld, 0, HOURGLASS, fixtures::BF2),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "737.1.b · and the other seat holds this one"
        );
        drop(unheld);

        let mut broke = in_hand();
        broke
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1);
        scripted(&mut broke);
        hidden_face(&mut broke, false);
        assert!(
            matches!(
                hide_now(&mut broke, 0, HOURGLASS, fixtures::BF1),
                Err(Refusal::NotEnoughRunes { .. })
            ),
            "[A] is a real cost even though the card is put down blind"
        );
        drop(broke);

        let mut fixture = in_hand();
        hidden_face(&mut fixture, false);
        hide_now(&mut fixture, 0, HOURGLASS, fixtures::BF1).unwrap();
        assert_eq!(
            hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1),
            Err(Refusal::Illegal(Reason::OneFacedown)),
            "106.4.b · the slot is taken"
        );
        hidden_face(&mut fixture, true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, HOURGLASS, fixtures::CHAIN)),
            Err(Refusal::Illegal(Reason::HiddenThisTurn)),
            "737.1.b · not on the turn it was hidden"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, HOURGLASS, fixtures::BASE)),
            Err(Refusal::Illegal(Reason::Unrevealed)),
            "a facedown card leaves its zone for the chain or not at all"
        );
        assert_eq!(
            ctx.kill(GUARDED, Cause::Rule),
            Killed::Yes,
            "408.3 · a facedown card's only property is the Hidden grant, even once its face is public"
        );
    }
}
