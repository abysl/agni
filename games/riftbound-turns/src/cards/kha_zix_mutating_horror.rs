use super::prelude::{
    done, enemy_alone_at, gain_xp, might_this_turn, on_attack, on_defend, unit, when,
};
use super::{Card, Flow, Item, Keyword, Source, Stage};
use crate::engine::ctx::{Ctx, Event};

pub const MIGHT: i16 = 2;
pub const XP: u8 = 2;

fn an_enemy_is_alone_here(ctx: &Ctx, _: &Event, source: Source) -> bool {
    enemy_alone_at(ctx, source.card)
}

fn mutate(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if ctx.on_board(me) {
        might_this_turn(ctx, item, me, MIGHT, None);
        ctx.narrate(format!("{{card {me}}} gets +{MIGHT} this turn"));
    }
    gain_xp(ctx, item.controller, XP);
    done()
}

pub static CARD: Card = unit(
    "Kha'Zix - Mutating Horror",
    &[Keyword::Ambush],
    &[
        when(on_attack(&[], mutate), an_enemy_is_alone_here),
        when(on_defend(&[], mutate), an_enemy_is_alone_here),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, triggers};
    use crate::state::ItemKind;

    const HORROR: u32 = 90;
    const SECOND: u32 = 91;

    const PLAIN: u32 = 54;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        fixture.table.cards.push(fixtures::unit(
            HORROR,
            fixtures::BF2,
            0,
            "Kha'Zix - Mutating Horror",
            4,
        ));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_prints_ambush_and_two_designation_triggers() {
        assert!(std::ptr::eq(
            script_of("Kha'Zix - Mutating Horror").unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Ambush));
        assert!(!CARD.has_static(crate::cards::Static::AmbushIntoEnemies));
        assert_eq!(CARD.abilities.len(), 2);
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.ambush_locations(0, HORROR),
            [Location::Battlefield(fixtures::BF2)]
        );
    }

    #[test]
    fn attacking_against_one_enemy_gains_two_might_and_two_xp_once() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: HORROR });
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        let top = ctx
            .blob
            .chain
            .last()
            .expect("the trigger waits on the chain");
        assert!(matches!(
            top.kind,
            ItemKind::Trigger { source, index: 0 } if source == HORROR
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(HORROR), 4 + i32::from(MIGHT));
        assert_eq!(ctx.xp(0), i32::from(XP));
        ctx.raise(Event::Defends { card: HORROR });
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "the defend trigger is its own ability"
        );
    }

    #[test]
    fn two_enemies_or_none_leave_the_triggers_silent() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::unit(SECOND, fixtures::BF2, 1, "Jinx", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.raise(Event::Attacks { card: HORROR });
        assert_eq!(triggers::collect(&mut ctx), 0);
        let mut alone = armed();
        alone.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BASE);
        alone.table.card_mut(fixtures::SPRITE).unwrap().seat = 1;
        alone.resolve();
        let mut ctx = alone.ctx();
        ctx.raise(Event::Defends { card: HORROR });
        assert_eq!(triggers::collect(&mut ctx), 0);
        assert_eq!(ctx.xp(0), 0);
    }
}
