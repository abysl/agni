use super::prelude::{a_card, card_target, done, play, spell, GEAR, UNIT_AT_BATTLEFIELD};
use super::{Card, Filter, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const UNIT_AT_A_BATTLEFIELD_OR_GEAR: Filter = Filter::Or(&[UNIT_AT_BATTLEFIELD, GEAR]);

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(card) = card_target(ctx, item, 0) {
        if ctx.mark_temporary(card) {
            ctx.narrate(format!("{{card {card}}} is Temporary"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Fading Memories",
    &[],
    &[play(
        &[a_card(
            UNIT_AT_A_BATTLEFIELD_OR_GEAR,
            "a unit at a battlefield or a gear",
        )],
        resolve,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Keyword, Trigger, IMPLICIT_TEMPORARY};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, play as play_engine, priority, triggers};
    use crate::rules::COUNTER_TEMPORARY;
    use crate::state::{ItemKind, Phase, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const FADING: u32 = 90;
    const THEIR_FADING: u32 = 91;
    const MY_GEAR: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const CHAOS_RUNE: u32 = 100;

    fn fading(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Fading Memories", 4, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fading(FADING, 0));
        fixture.table.cards.push(fading(THEIR_FADING, 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
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
    }

    fn temporary_matches(ctx: &Ctx, seat: u8) -> Vec<u32> {
        triggers::find(ctx, &Event::BeginningPhase { seat })
            .into_iter()
            .filter(|held| held.index == IMPLICIT_TEMPORARY)
            .map(|held| held.source)
            .collect()
    }

    #[test]
    fn the_script_is_a_plain_spell_over_a_unit_at_a_battlefield_or_a_gear() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Fading Memories").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Fading Memories");
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, UNIT_AT_A_BATTLEFIELD_OR_GEAR);
    }

    #[test]
    fn a_unit_at_a_battlefield_becomes_temporary_and_dies_at_its_controllers_next_beginning_phase()
    {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FADING).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 92}", "{card 93}", "cancel"],
            "the units at battlefields and every gear, never a unit in a base"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert!(!ctx.is_temporary(fixtures::THEIR_UNIT));
        both_pass(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_temporary(fixtures::THEIR_UNIT));
        assert!(
            ctx.on_board(fixtures::THEIR_UNIT),
            "Temporary kills at the Beginning Phase, not now"
        );
        assert_eq!(
            ctx.table
                .counter(Target::Card(fixtures::THEIR_UNIT), COUNTER_TEMPORARY),
            Some(1),
            "the mirror is stamped so kai draws the glyph"
        );
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(fixtures::THEIR_UNIT),
            counter: COUNTER_TEMPORARY,
            delta: 1
        }));
        assert!(ctx.blob.log.contains(&"{card 81} is Temporary".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(
            temporary_matches(&ctx, 1),
            [fixtures::SPRITE, fixtures::THEIR_UNIT],
            "742.1 · it is owed to its controller's Beginning Phase, beside the Sprite"
        );
        assert!(
            !temporary_matches(&ctx, 0).contains(&fixtures::THEIR_UNIT),
            "not to the caster's"
        );
        assert_eq!(ctx.card(FADING).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_gear_in_a_base_becomes_temporary_and_the_beginning_phase_kills_it_before_scoring() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FADING).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        both_pass(&mut ctx);
        assert!(ctx.is_temporary(MY_GEAR));
        assert!(!ctx.is_temporary(THEIR_GEAR));
        assert_eq!(temporary_matches(&ctx, 0), [MY_GEAR]);
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == MY_GEAR && index == IMPLICIT_TEMPORARY
        ));
        assert!(ctx.on_board(MY_GEAR), "742.1.b · it dies on resolution");
        both_pass(&mut ctx);
        assert_eq!(ctx.card(MY_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} is Temporary and dies".to_string()));
        assert!(ctx.on_board(THEIR_GEAR));
    }

    #[test]
    fn a_target_that_left_the_board_is_left_alone_and_a_temporary_one_is_marked_once() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FADING).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let base = ctx.zones.base.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::THEIR_UNIT, base, 1), 1)
            .unwrap();
        both_pass(&mut ctx);
        assert!(
            !ctx.is_temporary(fixtures::THEIR_UNIT),
            "back in its base it is no longer a legal target"
        );
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("is Temporary")));
        assert_eq!(ctx.card(FADING).unwrap().zone, Some(fixtures::TRASH));
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FADING).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        both_pass(&mut ctx);
        assert!(ctx.is_temporary(fixtures::SPRITE));
        assert_eq!(
            ctx.table
                .counter(Target::Card(fixtures::SPRITE), COUNTER_TEMPORARY),
            Some(1),
            "a printed Temporary is stamped once, never twice"
        );
        assert_eq!(
            temporary_matches(&ctx, 1),
            [fixtures::SPRITE],
            "one implicit Temporary trigger, not two"
        );
    }

    #[test]
    fn units_in_a_base_are_refused_and_the_spell_is_the_turn_players() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_FADING)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, FADING).unwrap();
        for wrong in [
            fixtures::VI,
            fixtures::GROUNDS,
            fixtures::HAND_GEAR,
            fixtures::LEGEND_CARD,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is neither a unit at a battlefield nor a gear on the board"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(FADING).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
