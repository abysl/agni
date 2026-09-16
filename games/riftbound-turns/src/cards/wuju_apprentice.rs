use super::prelude::{done, draw, play, unit, when};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::engine::statics;

pub const HUNT: u8 = 1;
pub const LEVEL: u8 = 6;
const DRAWS: usize = 1;

pub fn leveled(ctx: &Ctx, _: &Event, source: Source) -> bool {
    statics::level_active(ctx, source.card, LEVEL)
}

fn study(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = unit(
    "Wuju Apprentice",
    &[Keyword::Hunt(HUNT)],
    &[when(play(&[], study), leveled)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const APPRENTICE: u32 = 90;

    fn apprentice(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: None,
            domain: vec!["Calm".into()],
            ..fixtures::unit(APPRENTICE, zone, seat, "Wuju Apprentice", 2)
        }
    }

    fn dojo(xp: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(apprentice(fixtures::HAND, 0));
        fixture.set_xp(0, xp);
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(APPRENTICE).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_prints_hunt_one_and_a_play_trigger_conditioned_on_level_six() {
        assert!(std::ptr::eq(script_of("Wuju Apprentice").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Hunt(1)]);
        assert_eq!(CARD.hunt(), 1);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.condition.is_some(), "the Level gates the trigger");
        assert!(CARD.statics.is_empty());
        assert_eq!(LEVEL, 6);
    }

    #[test]
    fn played_at_six_xp_the_trigger_draws_one_when_it_resolves() {
        let mut fixture = dojo(6);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, APPRENTICE).unwrap();
        assert!(ctx.on_board(APPRENTICE));
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == APPRENTICE
        ));
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1,
            "the draw waits on the chain"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand);
        assert_eq!(ctx.hand_of(1).len(), 1, "only its controller draws");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_at_five_xp_nothing_triggers_and_the_opponents_six_does_not_count() {
        let mut fixture = dojo(5);
        fixture.set_xp(1, 6);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, APPRENTICE).unwrap();
        assert!(ctx.on_board(APPRENTICE));
        assert!(
            ctx.blob.chain.is_empty(),
            "824.1.c · below Level 6 the play trigger does not exist"
        );
        assert_eq!(ctx.hand_of(0).len(), hand - 1);
        assert!(ctx.fault.is_none());
    }
}
