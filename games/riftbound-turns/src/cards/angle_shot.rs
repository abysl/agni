use super::prelude::{
    a_card, a_unit, attach_gear, attached_to, card_target, detach_gear, done, draw, play, spell,
    Attached,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const DRAWS: usize = 1;

const UNIT: usize = 0;
const GEAR: usize = 1;

pub const AN_EQUIPMENT_OF_THE_SAME_CONTROLLER: Filter = Filter::And(&[
    Filter::Gear,
    Filter::Equipment,
    Filter::SameControllerAs(UNIT as u8),
]);

pub fn same_controller(ctx: &Ctx, unit: u32, gear: u32) -> bool {
    ctx.controller(unit) == ctx.controller(gear)
}

pub fn angle(ctx: &mut Ctx, unit: u32, gear: u32) -> bool {
    if attached_to(ctx, gear) == Some(unit) {
        return detach_gear(ctx, gear);
    }
    attach_gear(ctx, gear, unit) == Attached::Yes
}

fn shoot(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if let Some(unit) = card_target(ctx, item, UNIT) {
        match (card_target(ctx, item, GEAR), item.targets.get(GEAR)) {
            (Some(gear), _) => {
                angle(ctx, unit, gear);
            }
            (None, Some(TargetRef::Card(gear)))
                if ctx.on_board(*gear) && !same_controller(ctx, unit, *gear) =>
            {
                ctx.narrate(format!(
                    "{{card {gear}}} and {{card {unit}}} have different controllers · nothing moves"
                ));
            }
            _ => {}
        }
    }
    draw(ctx, seat, DRAWS);
    done()
}

pub static CARD: Card = spell(
    "Angle Shot",
    &[Keyword::Reaction],
    &[play(
        &[
            a_unit("a unit"),
            a_card(
                AN_EQUIPMENT_OF_THE_SAME_CONTROLLER,
                "an Equipment with the same controller",
            ),
        ],
        shoot,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, equip, gear as gear_card, is_attached, while_attached};
    use crate::cards::{script_of, Grant, TargetKind, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ANGLE_SHOT: u32 = 90;
    const SWORD: u32 = 91;
    const THEIR_SWORD: u32 = 92;
    const BAUBLE: u32 = 93;
    const ALLY: u32 = 94;

    static SWORD_CARD: Card = prelude::with_statics(
        gear_card(
            "Sword",
            &[Keyword::Equip(prelude::ONE_ENERGY)],
            &[equip(prelude::ONE_ENERGY)],
        ),
        &[while_attached(&[Grant::Might(2)])],
    );

    fn sword(id: u32, seat: u8, name: &str) -> CardInfo {
        CardInfo {
            domain: vec!["Fury".into()],
            ..fixtures::gear(id, fixtures::BASE, seat, name, 2)
        }
    }

    fn armory() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Fury".into()],
            ..fixtures::spell(ANGLE_SHOT, fixtures::HAND, 0, "Angle Shot", 2, 0)
        });
        fixture.table.cards.push(sword(SWORD, 0, "Sword"));
        fixture.table.cards.push(sword(THEIR_SWORD, 1, "Sword"));
        fixture.table.cards.push(sword(BAUBLE, 0, "Bauble"));
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BASE, 0, "Ally", 1));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(SWORD, &SWORD_CARD)
            .with_script(THEIR_SWORD, &SWORD_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ANGLE_SHOT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn drew(ctx: &Ctx, seat: u8) -> usize {
        ctx.events
            .iter()
            .filter(|event| matches!(event, Event::Drew { seat: who, .. } if *who == seat))
            .count()
    }

    #[test]
    fn the_script_is_a_reaction_over_a_unit_and_an_equipment() {
        assert!(std::ptr::eq(script_of("Angle Shot").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, prelude::UNIT);
        assert_eq!(
            ability.targets[1].filter,
            AN_EQUIPMENT_OF_THE_SAME_CONTROLLER
        );
        assert!(ability
            .targets
            .iter()
            .all(|spec| (spec.min, spec.max, spec.kind) == (1, 1, TargetKind::Card)));
        assert_eq!(DRAWS, 1);
    }

    #[test]
    fn a_loose_equipment_is_attached_to_the_chosen_unit_and_one_card_is_drawn() {
        let mut fixture = armory();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, ANGLE_SHOT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "{card 94}", "cancel"],
            "any unit on the board"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {SWORD}}}"), "cancel".to_string()],
            "Equipment of Vi's controller only · the Bauble is plain gear and the other Sword is theirs"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SWORD}}}")).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(SWORD)]
        );
        assert!(!is_attached(&ctx, SWORD), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, SWORD), Some(fixtures::VI));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "the Sword's grant is live"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SWORD}}} is attached to {{card {}}}",
            fixtures::VI
        )));
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one drawn");
        assert_eq!(ctx.card(ANGLE_SHOT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_equipment_already_on_the_chosen_unit_is_detached_instead() {
        let mut fixture = armory();
        let mut ctx = fixture.ctx();
        assert_eq!(attach_gear(&mut ctx, SWORD, fixtures::VI), Attached::Yes);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        fixtures::play_from_hand(&mut ctx, 0, ANGLE_SHOT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SWORD}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!is_attached(&ctx, SWORD));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SWORD}}} detaches from {{card {}}}",
            fixtures::VI
        )));
        assert_eq!(drew(&ctx, 0), DRAWS);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_equipment_on_another_unit_moves_over_to_the_chosen_one() {
        let mut fixture = armory();
        let mut ctx = fixture.ctx();
        assert_eq!(attach_gear(&mut ctx, SWORD, fixtures::VI), Attached::Yes);
        fixtures::play_from_hand(&mut ctx, 0, ANGLE_SHOT).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {ALLY}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SWORD}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            attached_to(&ctx, SWORD),
            Some(ALLY),
            "the Sword leaves Vi for the Ally"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(ALLY), 3);
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut theirs = armory();
        let mut ctx = theirs.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ANGLE_SHOT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SWORD}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            attached_to(&ctx, THEIR_SWORD),
            Some(fixtures::THEIR_UNIT),
            "the opponent's Sword goes onto the opponent's Jinx: same controller, not the caster's"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_pair_with_different_controllers_is_left_alone_and_the_draw_still_happens() {
        let mut fixture = armory();
        let mut ctx = fixture.ctx();
        assert!(!same_controller(&ctx, fixtures::VI, THEIR_SWORD));
        assert!(same_controller(&ctx, fixtures::VI, SWORD));
        fixtures::play_from_hand(&mut ctx, 0, ANGLE_SHOT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {SWORD}}}")).unwrap();
        assert!(ctx.set_controller(SWORD, 1, fixtures::THEIR_UNIT));
        assert!(!same_controller(&ctx, fixtures::VI, SWORD));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, SWORD));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SWORD}}} and {{card {}}} have different controllers · nothing moves",
            fixtures::VI
        )));
        assert_eq!(drew(&ctx, 0), DRAWS, "356.3.e.5 · the draw is not a target");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn plain_gear_is_refused_as_the_second_target() {
        let mut fixture = armory();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ANGLE_SHOT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[BAUBLE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit is not an Equipment"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(ANGLE_SHOT).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn an_equipment_of_the_other_controller_is_not_offered_once_the_unit_is_chosen() {
        let mut fixture = armory();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ANGLE_SHOT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {SWORD}}}"), "cancel".to_string()],
            "only Vi's controller's Equipment"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[THEIR_SWORD]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
    }
}
