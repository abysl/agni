use super::prelude::{
    a_friendly_unit, an_enemy_unit, bounce, card_target, chosen_mode, done, might_this_turn, modal,
    mode, spell,
};
use super::{Card, Flow, Item, Keyword, ModeSpec, Stage};
use crate::engine::ctx::Ctx;

pub const SHRINK: i16 = -2;
pub const MODES: &[ModeSpec] = &[
    mode(
        "return a friendly unit to its owner's hand",
        &[a_friendly_unit("a friendly unit to return to hand")],
        return_it,
    ),
    mode(
        "give an enemy unit -2 Might this turn",
        &[an_enemy_unit("an enemy unit to give -2 Might")],
        shrink,
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Return,
    Shrink,
}

pub fn mode_of(item: &Item) -> Option<Mode> {
    match chosen_mode(item)? {
        0 => Some(Mode::Return),
        1 => Some(Mode::Shrink),
        _ => None,
    }
}

fn return_it(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        bounce(ctx, unit);
    }
    done()
}

fn shrink(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, SHRINK, None);
        ctx.narrate(format!("{{card {unit}}} gets {SHRINK} Might this turn"));
    }
    done()
}

pub static CARD: Card = spell("Mesmerize", &[Keyword::Reaction], &[modal(MODES)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{this_turn, ENEMY_UNIT, FRIENDLY_UNIT};
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MESMERIZE: u32 = 90;
    const MIND_RUNE: u32 = 46;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::spell(MESMERIZE, fixtures::HAND, 0, "Mesmerize", 1, 1)
        });
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MESMERIZE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn cast_on(ctx: &mut Ctx, mode: &str, unit: u32) {
        fixtures::play_from_hand(ctx, 0, MESMERIZE).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Mode {
                item: 1,
                execution: 0
            })
        );
        fixtures::choose(ctx, 0, mode).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
    }

    #[test]
    fn the_script_is_a_reaction_over_two_named_modes() {
        assert!(std::ptr::eq(script_of("Mesmerize").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.modes.len(), 2);
        assert_eq!(
            ability.modes[0].label,
            "return a friendly unit to its owner's hand"
        );
        assert_eq!(ability.modes[0].targets.len(), 1);
        assert_eq!(ability.modes[0].targets[0].filter, FRIENDLY_UNIT);
        assert_eq!(
            ability.modes[1].label,
            "give an enemy unit -2 Might this turn"
        );
        assert_eq!(ability.modes[1].targets.len(), 1);
        assert_eq!(ability.modes[1].targets[0].filter, ENEMY_UNIT);
        assert_eq!(SHRINK, -2);
        let mut item = ChainItem::new(1, ItemKind::Spell { card: MESMERIZE }, 0, Origin::Hand);
        assert_eq!(mode_of(&item), None);
        item.set_mode(0, 0);
        assert_eq!(mode_of(&item), Some(Mode::Return));
        item.set_mode(0, 1);
        assert_eq!(mode_of(&item), Some(Mode::Shrink));
        item.set_mode(0, 2);
        assert_eq!(mode_of(&item), None);
    }

    #[test]
    fn a_friendly_unit_returns_to_its_owners_hand() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, MESMERIZE).unwrap();
        fixtures::choose(&mut ctx, 0, "return a friendly unit to its owner's hand").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "friendly units alone in the return mode"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(ctx.on_board(fixtures::VI), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(fixtures::VI).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one returned");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} returns to hand".to_string()));
        assert_eq!(ctx.card(MESMERIZE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_unit_gets_minus_two_this_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_on(
            &mut ctx,
            "give an enemy unit -2 Might this turn",
            fixtures::SPRITE,
        );
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(fixtures::SPRITE), "shrunk, not returned");
        assert_eq!(ctx.current_might(fixtures::SPRITE), 1);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} gets -2 Might this turn".to_string()));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::SPRITE), 3, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_stolen_pick_keeps_its_mode_and_a_gear_or_a_gone_unit_is_left_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_on(
            &mut ctx,
            "give an enemy unit -2 Might this turn",
            fixtures::THEIR_UNIT,
        );
        ctx.blob.chain[0].controller = 1;
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "the mode stays -2 Might; under the other seat's control Jinx is no enemy unit, so the pick is illegal and nothing happens"
        );
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("Might this turn")));
        drop(ctx);
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MESMERIZE).unwrap();
        fixtures::choose(&mut ctx, 0, "give an enemy unit -2 Might this turn").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a friendly unit is the other mode's target"
        );
        for wrong in [
            fixtures::HAND_GEAR,
            fixtures::LEGEND_CARD,
            fixtures::GROUNDS,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.bounce(fixtures::THEIR_UNIT);
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("Might this turn")));
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| line.ends_with("returns to hand"))
                .count(),
            1,
            "only the response's own bounce"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_mode_is_asked_by_name_before_its_target() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MESMERIZE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "return a friendly unit to its owner's hand",
                "give an enemy unit -2 Might this turn",
                "cancel"
            ]
        );
    }
}
