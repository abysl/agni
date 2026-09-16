use super::prelude::{a_card, card_target, done, play, spell, GEAR};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

fn crumble(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(gear) = card_target(ctx, item, 0) {
        if ctx.mark_temporary(gear) {
            ctx.narrate(format!("{{card {gear}}} is Temporary"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Turn to Dust",
    &[],
    &[play(&[a_card(GEAR, "a gear")], crumble)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, Trigger, IMPLICIT_TEMPORARY};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, play as play_engine, triggers};
    use crate::rules::COUNTER_TEMPORARY;
    use crate::state::{ItemKind, Phase, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const DUST: u32 = 90;
    const THEIR_DUST: u32 = 91;
    const MY_GEAR: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const MIND_RUNE: u32 = 100;

    fn dust(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Turn to Dust", 2, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dust(DUST, 0));
        fixture.table.cards.push(dust(THEIR_DUST, 1));
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
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
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

    fn temporary_matches(ctx: &Ctx, seat: u8) -> Vec<u32> {
        triggers::find(ctx, &Event::BeginningPhase { seat })
            .into_iter()
            .filter(|held| held.index == IMPLICIT_TEMPORARY)
            .map(|held| held.source)
            .collect()
    }

    #[test]
    fn the_script_is_a_plain_spell_over_one_gear() {
        assert!(std::ptr::eq(script_of("Turn to Dust").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, GEAR);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn an_enemy_gear_becomes_temporary_and_dies_at_its_controllers_beginning_phase() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DUST).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 93}", "cancel"],
            "every gear on the board and nothing else"
        );
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_GEAR)]);
        assert!(!ctx.is_temporary(THEIR_GEAR));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_temporary(THEIR_GEAR));
        assert!(ctx.on_board(THEIR_GEAR), "742.1 · it dies later, not now");
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(THEIR_GEAR),
            counter: COUNTER_TEMPORARY,
            delta: 1
        }));
        assert!(ctx.blob.log.contains(&"{card 93} is Temporary".to_string()));
        assert_eq!(
            temporary_matches(&ctx, 1),
            [fixtures::SPRITE, THEIR_GEAR],
            "owed to its controller's Beginning Phase beside the Sprite"
        );
        assert!(!temporary_matches(&ctx, 0).contains(&THEIR_GEAR));
        assert_eq!(ctx.card(DUST).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn your_own_gear_is_killed_by_the_next_beginning_phase_before_scoring() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DUST).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_temporary(MY_GEAR));
        assert_eq!(temporary_matches(&ctx, 0), [MY_GEAR]);
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index } if source == MY_GEAR && index == IMPLICIT_TEMPORARY
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(MY_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} is Temporary and dies".to_string()));
        assert!(ctx.on_board(THEIR_GEAR));
    }

    #[test]
    fn a_gear_that_left_the_board_is_left_alone_and_a_unit_is_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DUST)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, DUST).unwrap();
        for wrong in [
            fixtures::VI,
            fixtures::SPRITE,
            fixtures::HAND_GEAR,
            fixtures::GROUNDS,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a gear on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        let trash = ctx.zones.trash.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(THEIR_GEAR, trash, 1), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.is_temporary(THEIR_GEAR));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("is Temporary")));
        assert_eq!(ctx.card(DUST).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }
}
