use super::prelude::{buff, done, legion, play, unit, when};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub fn legion_on_entry(ctx: &Ctx, _: &Event, source: Source) -> bool {
    legion(ctx, ctx.controller(source.card))
}

fn seek_glory(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if buff(ctx, me) {
        ctx.narrate(format!("{{card {me}}} is buffed"));
    }
    done()
}

pub static CARD: Card = unit(
    "Trifarian Gloryseeker",
    &[Keyword::Legion],
    &[when(play(&[], seek_glory), legion_on_entry)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::Trigger;
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::ItemKind;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::Target;

    const SEEKER: u32 = 90;
    const SPARE_RUNE: u32 = 46;

    fn camp() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut seeker = fixtures::unit(SEEKER, fixtures::HAND, 0, "Trifarian Gloryseeker", 2);
        seeker.domain = vec!["Order".into()];
        seeker.energy = Some(2);
        fixture.table.cards.push(seeker);
        fixture
            .table
            .cards
            .push(fixtures::rune(SPARE_RUNE, 0, "Fury", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn buffs(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_BUFFED)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_legion_unit_whose_play_trigger_is_gated_on_the_seat_having_played() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Trifarian Gloryseeker").unwrap(),
            &CARD
        ));
        let fixture = camp();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SEEKER).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Legion]);
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_some(), "812.1.b.1 · the Legion gate");
    }

    #[test]
    fn played_after_a_spell_this_turn_the_seeker_buffs_itself_when_the_trigger_resolves() {
        let mut fixture = camp();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(legion(&ctx, 0), "Spark resolved: Legion is on for seat 0");
        assert!(!legion(&ctx, 1));
        fixtures::play_from_hand(&mut ctx, 0, SEEKER).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == SEEKER
        ));
        assert_eq!(buffs(&ctx, SEEKER), 0, "the buff waits for the chain");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(SEEKER));
        assert_eq!(buffs(&ctx, SEEKER), 1);
        assert_eq!(ctx.current_might(SEEKER), 3, "703 · a buff is +1 Might");
        assert!(ctx.effects.contains(&Effect::Counter {
            target: Target::Card(SEEKER),
            counter: COUNTER_BUFFED,
            delta: 1
        }));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {SEEKER}}} is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn as_the_first_card_of_the_turn_the_seeker_triggers_nothing_and_stays_at_two() {
        let mut fixture = camp();
        let mut ctx = fixture.ctx();
        assert!(!legion(&ctx, 0));
        fixtures::play_from_hand(&mut ctx, 0, SEEKER).unwrap();
        assert!(ctx.on_board(SEEKER));
        assert!(
            legion(&ctx, 0),
            "812.1.c · the seeker itself is a card played this turn, for the next card"
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "the seeker's own play is not another card"
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx.is_buffed(SEEKER));
        assert_eq!(ctx.current_might(SEEKER), 2);
        assert!(!ctx.blob.log.iter().any(|line| line.contains("is buffed")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_opponents_play_this_turn_does_not_satisfy_my_legion() {
        let mut fixture = camp();
        fixture.blob.seat_mut(1).played_main = true;
        let mut ctx = fixture.ctx();
        assert!(legion(&ctx, 1));
        assert!(!legion(&ctx, 0));
        fixtures::play_from_hand(&mut ctx, 0, SEEKER).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.is_buffed(SEEKER));
    }

    #[test]
    fn an_already_buffed_seeker_gains_no_second_buff_from_its_trigger() {
        let mut fixture = camp();
        fixture.blob.seat_mut(0).played_main = true;
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SEEKER).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.buff(SEEKER));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(buffs(&ctx, SEEKER), 1, "702.3 · one buff at a time");
        assert_eq!(ctx.current_might(SEEKER), 3);
        assert!(!ctx
            .blob
            .log
            .contains(&format!("{{card {SEEKER}}} is buffed")));
    }
}
