use super::prelude::{a_spell, done, item_controller, item_target, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::chain;
use crate::engine::ctx::{CounterDest, Ctx};

pub fn an_opponent_played_another_spell(ctx: &Ctx, item: &Item, target: u16) -> bool {
    let caster = item.controller;
    let target_controller = item_controller(ctx, target);
    (0..ctx.players())
        .filter(|seat| *seat != caster)
        .any(|seat| {
            let played = ctx.blob.seat(seat).spells_played;
            let excluded = u8::from(target_controller == Some(seat));
            played > excluded
        })
}

fn crumble(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(target) = item_target(ctx, item, 0) else {
        return done();
    };
    if !an_opponent_played_another_spell(ctx, item, target) {
        ctx.narrate(format!(
            "{{card {}}} · no opponent played another spell this turn",
            item.kind.source()
        ));
        return done();
    }
    chain::counter(ctx, target, CounterDest::Trash);
    done()
}

pub static CARD: Card = spell(
    "Crumbling Sands",
    &[Keyword::Reaction],
    &[play(&[a_spell("a spell to counter")], crumble)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::PromptWhy;

    const SANDS: u32 = 90;
    const THEIR_SPELL: u32 = 91;
    const THEIR_SECOND: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut sands = fixtures::spell(SANDS, fixtures::HAND, 0, "Crumbling Sands", 1, 1);
        sands.domain = vec!["Calm".into()];
        fixture.table.cards.push(sands);
        for id in [THEIR_SPELL, THEIR_SECOND] {
            fixture
                .table
                .cards
                .push(fixtures::spell(id, fixtures::HAND, 1, "Spark", 0, 0));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Calm", false));
        fixture.blob = crate::state::GameBlob::start(2, 1, crate::state::Mode::Enforced);
        fixture.blob.set_phase(crate::state::Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.resolve();
        fixture
    }

    #[test]
    fn it_counters_the_opponents_second_spell_while_the_first_is_still_on_the_chain() {
        assert!(std::ptr::eq(script_of("Crumbling Sands").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_SPELL).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.blob.seat(1).spells_played, 1);
        fixtures::play_from_hand(&mut ctx, 1, THEIR_SECOND).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, SANDS).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 3, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 92} on the chain").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(THEIR_SECOND).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{card 92} is countered".to_string()));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_opponents_only_spell_is_not_another_one() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_SPELL).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, SANDS).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91} on the chain").unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} · no opponent played another spell this turn".to_string()));
        assert_eq!(ctx.blob.chain.len(), 1, "their spell is still there");
        assert_eq!(ctx.card(THEIR_SPELL).unwrap().zone, Some(fixtures::CHAIN));
    }
}
