use super::bullet_time::enemy_units_at;
use super::prelude::{a_battlefield, a_card, card_target, deal, done, play, spell, zone_target};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 4;
pub const SPLASH: u8 = 1;

pub const ENEMY_UNIT_THERE: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::SameLocationAs(0)]);
pub const STRUCK: TargetSpec = a_card(ENEMY_UNIT_THERE, "an enemy unit there");

fn strike(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(zone) = zone_target(item, 0) else {
        return done();
    };
    let struck = card_target(ctx, item, 1);
    if let Some(unit) = struck {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    let others: Vec<u32> = enemy_units_at(ctx, item.controller, zone)
        .into_iter()
        .filter(|unit| Some(*unit) != struck)
        .collect();
    for unit in others {
        if deal(ctx, item, unit, SPLASH) {
            ctx.narrate(format!("{{card {unit}}} takes {SPLASH}"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Crescent Strike",
    &[Keyword::Action],
    &[play(&[a_battlefield("a battlefield"), STRUCK], strike)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Cause, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const STRIKE: u32 = 90;
    const THEIR_STRIKE: u32 = 91;
    const BRUTE: u32 = 92;
    const SCOUT: u32 = 93;
    const MY_GUARD: u32 = 94;
    const MIND_RUNE: u32 = 100;

    fn strike_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Crescent Strike", 3, 1);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(strike_card(STRIKE, 0));
        fixture.table.cards.push(strike_card(THEIR_STRIKE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 1, "Scout", 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(MY_GUARD, fixtures::BF1, 0, "Guard", 2));
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

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt {
                    card,
                    n,
                    source: Cause::Item(1),
                } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_is_an_action_choosing_a_battlefield_then_an_enemy_unit_there() {
        assert!(std::ptr::eq(script_of("Crescent Strike").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].kind, TargetKind::Zone);
        assert_eq!(ability.targets[0].filter, Filter::AtBattlefield);
        assert_eq!(ability.targets[1], STRUCK);
        assert_eq!((STRUCK.min, STRUCK.max), (1, 1));
        assert_eq!((DAMAGE, SPLASH), (4, 1));
    }

    #[test]
    fn four_lands_on_the_chosen_enemy_and_one_on_each_other_enemy_there_sparing_my_own() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "the two battlefields in play are offered first"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 93}", "cancel"],
            "the enemy units at that battlefield: not my Guard, not the Sprite elsewhere"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Zone(fixtures::BF1), TargetRef::Card(BRUTE)]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(damage_events(&ctx), [(BRUTE, DAMAGE), (SCOUT, SPLASH)]);
        assert_eq!(ctx.damage_on(BRUTE), 4);
        assert!(ctx.on_board(BRUTE));
        assert!(!ctx.on_board(SCOUT), "one kills the 1-Might Scout");
        assert_eq!(ctx.damage_on(MY_GUARD), 0);
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(ctx.blob.log.contains(&"{card 92} takes 4".to_string()));
        assert!(ctx.blob.log.contains(&"{card 93} takes 1".to_string()));
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_struck_unit_that_left_takes_nothing_while_the_others_there_still_take_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.recall(BRUTE, false);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(damage_events(&ctx), [(SCOUT, SPLASH)]);
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_friendly_unit_a_unit_elsewhere_and_a_base_are_refused_and_the_action_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STRIKE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[u32::from(fixtures::BASE)]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a base is not a battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        for wrong in [MY_GUARD, fixtures::SPRITE, fixtures::THEIR_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is friendly, at another battlefield or in a base"
            );
        }
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
