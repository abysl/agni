use super::prelude::{
    a_card, card_target, charm_destination, done, move_unit, play, spell, target,
    CHARM_DESTINATION, MOVABLE_ENEMY_UNIT, MOVABLE_FRIENDLY_UNIT,
};
use super::{Card, Filter, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

const FRIENDLY: usize = 0;
const FRIENDLY_TO: usize = 1;
const ENEMY: usize = 2;
const ENEMY_TO: usize = 3;

const ENEMY_DESTINATION: TargetSpec = target(
    Filter::DifferentLocationFrom(ENEMY as u8),
    1,
    1,
    TargetKind::Zone,
    "where the enemy unit goes",
);

fn move_one_then_the_other(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, FRIENDLY) {
        if let Some(to) = charm_destination(ctx, item, unit, FRIENDLY_TO) {
            move_unit(ctx, item, unit, to);
        }
    }
    if let Some(unit) = card_target(ctx, item, ENEMY) {
        if let Some(to) = charm_destination(ctx, item, unit, ENEMY_TO) {
            move_unit(ctx, item, unit, to);
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Void Assault",
    &[],
    &[play(
        &[
            a_card(MOVABLE_FRIENDLY_UNIT, "a friendly unit to move"),
            CHARM_DESTINATION,
            a_card(MOVABLE_ENEMY_UNIT, "an enemy unit to move"),
            ENEMY_DESTINATION,
        ],
        move_one_then_the_other,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{priority, prompts, settle};
    use crate::state::{PromptWhy, Staged, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const ASSAULT: u32 = 90;
    const CHAOS_RUNE: u32 = 46;

    fn assault(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Void Assault", 2, 1);
        card.domain = vec!["Body".into(), "Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(assault(ASSAULT, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ASSAULT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn pass_both(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_moves_a_friendly_unit_then_an_enemy_unit_with_both_destinations_chosen_at_play() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Void Assault").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        let ability = &CARD.abilities[0];
        assert_eq!(ability.targets.len(), 4);
        assert_eq!(ability.targets[FRIENDLY].filter, MOVABLE_FRIENDLY_UNIT);
        assert_eq!(ability.targets[FRIENDLY_TO], CHARM_DESTINATION);
        assert_eq!(ability.targets[ENEMY].filter, MOVABLE_ENEMY_UNIT);
        assert_eq!(
            ability.targets[ENEMY_TO].filter,
            Filter::DifferentLocationFrom(2)
        );
        assert_eq!(ability.targets[ENEMY_TO].kind, TargetKind::Zone);
    }

    #[test]
    fn both_units_land_where_they_were_sent_and_the_caster_attacks_where_its_unit_arrived_first() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ASSAULT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "a unit at its base may go to either battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 2 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 81}", "cancel"]);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 9}", "{zone 10}", "cancel"],
            "the enemy unit's destinations are read from its own location"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Card(fixtures::THEIR_UNIT),
                TargetRef::Zone(fixtures::BF1),
            ],
            "every choice is locked in before anyone may react"
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move { card, zone, .. }
                    if *card == CHAOS_RUNE && Some(*zone) == ctx.zones.rune_deck
            )),
            "the [C] is paid by the Chaos rune: {:?}",
            ctx.effects
        );
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        pass_both(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        let moves: Vec<u32> = ctx
            .effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move { card, zone, .. } if *zone == fixtures::BF1 => Some(*card),
                _ => None,
            })
            .collect();
        assert_eq!(
            moves,
            [fixtures::VI, fixtures::THEIR_UNIT],
            "the friendly unit moves first, then the enemy"
        );
        assert_eq!(ctx.blob.contester(fixtures::BF1), Some(0));
        let showdown = ctx.blob.showdown.clone().expect("the combat opens");
        assert_eq!(
            (
                showdown.zone,
                showdown.attacker,
                showdown.defender,
                showdown.combat
            ),
            (fixtures::BF1, 0, 1, true)
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_sent_alone_to_an_unheld_battlefield_marks_the_contest_for_its_own_seat() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ASSAULT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        pass_both(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.blob.contester(fixtures::BF2), Some(0));
        assert_eq!(
            ctx.blob.contester(fixtures::BF1),
            Some(1),
            "nothing of the caster's marked {{zone 9}}: the enemy seat is its contester"
        );
        let showdown = ctx
            .blob
            .showdown
            .clone()
            .expect("the enemy seat's showdown opens");
        assert_eq!(
            (
                showdown.zone,
                showdown.attacker,
                showdown.defender,
                showdown.combat
            ),
            (fixtures::BF1, 1, 0, false),
            "the seat whose unit marked the contest is the attacker, not the caster"
        );
        assert_eq!(
            ctx.blob.staged,
            [Staged {
                zone: fixtures::BF2,
                combat: true,
                contester: 0
            }],
            "the caster's own contest waits its turn as a combat"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_destination_the_lair_closes_resolves_as_nothing_while_the_other_move_still_happens() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::GROUNDS).unwrap().name = "Vilemaw's Lair".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ASSAULT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(
            !fixtures::labels(&ctx).contains(&"{zone 8}".to_string()),
            "the base is not offered as a destination from the Lair (358.3.a): {:?}",
            fixtures::labels(&ctx)
        );
        fixtures::choose(&mut ctx, 0, "{zone 10}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        pass_both(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2)),
            "the Lair only closes the way home; another battlefield is open"
        );
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Base(1)),
            "the Sprite at the other battlefield goes home"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::SPRITE,
            zone: fixtures::BASE,
            seat: 1,
            index: TOP
        }));
    }
}
