use super::prelude::{at_battlefield, done, ready_runes, triggered, unit, when};
use super::{Card, Flow, Item, Source, Stage, Trigger};
use crate::engine::ctx::{Ctx, Event};

pub const RUNES: usize = 4;

fn at_a_battlefield(ctx: &Ctx, _: &Event, source: Source) -> bool {
    at_battlefield(ctx, source.card)
}

fn harmonize(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let readied = ready_runes(ctx, seat, RUNES);
    ctx.narrate(format!(
        "{{card {}}} readies {readied} of {{seat {seat}}}'s runes",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = unit(
    "Sona - Harmonious",
    &[],
    &[when(
        triggered(Trigger::EndOfTurn, &[], harmonize),
        at_a_battlefield,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::phases;
    use crate::state::{ItemKind, Phase};

    const SONA: u32 = 90;
    const FIFTH_RUNE: u32 = 46;

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(fixtures::unit(SONA, zone, 0, "Sona - Harmonious", 4));
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(FIFTH_RUNE, 0, "Calm", true));
        fixture.table.card_mut(44).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn exhausted_runes(ctx: &crate::engine::ctx::Ctx, seat: u8) -> usize {
        ctx.runes_of(seat)
            .into_iter()
            .filter(|rune| rune.exhausted)
            .count()
    }

    #[test]
    fn at_a_battlefield_she_readies_four_friendly_runes_when_her_controllers_turn_ends() {
        assert!(std::ptr::eq(script_of("Sona - Harmonious").unwrap(), &CARD));
        assert_eq!(CARD.abilities[0].trigger, Trigger::EndOfTurn);
        let mut fixture = armed(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(exhausted_runes(&ctx, 0), 5);
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.blob.phase(), Some(Phase::Ending));
        assert!(
            matches!(
                ctx.blob.chain.last().map(|top| top.kind),
                Some(ItemKind::Trigger { source, index: 0 }) if source == SONA
            ),
            "383.2.a.1 · the condition holds in the Ending Step, so the trigger goes on the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 90} readies 4 of {seat 0}'s runes"));
        assert_eq!(
            (ctx.turn(), ctx.turn_player()),
            (2, 1),
            "the turn ends once her trigger has resolved"
        );
        assert_eq!(
            exhausted_runes(&ctx, 0),
            1,
            "four of the five exhausted friendly runes are readied"
        );
        assert!(ctx.fault.is_none());
        let mut short = armed(fixtures::BF1);
        for rune in [42, 43, FIFTH_RUNE] {
            short.table.card_mut(rune).unwrap().exhausted = false;
        }
        let mut ctx = short.ctx();
        assert_eq!(exhausted_runes(&ctx, 0), 2);
        phases::end_turn(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        crate::engine::settle(&mut ctx).unwrap();
        assert!(
            ctx.blob
                .log
                .iter()
                .any(|line| line == "{card 90} readies 2 of {seat 0}'s runes"),
            "the enemy's exhausted rune is not friendly: {:?}",
            ctx.blob.log
        );
        assert_eq!(exhausted_runes(&ctx, 0), 0);
    }

    #[test]
    fn at_base_or_on_an_opponents_turn_the_ending_step_readies_nothing() {
        let mut home = armed(fixtures::BASE);
        let mut ctx = home.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert!(
            !ctx.blob.chain.iter().any(|item| item.kind.source() == SONA),
            "383.2.a.1 · at base the condition fails and nothing is placed on the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!((ctx.turn(), ctx.turn_player()), (2, 1));
        assert_eq!(exhausted_runes(&ctx, 0), 5);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("{card 90} readies")));
        let mut theirs = armed(fixtures::BF1);
        theirs.blob = crate::state::GameBlob::start(2, 1, crate::state::Mode::Enforced);
        theirs.blob.set_phase(Phase::Action);
        theirs.blob.seats = vec![Default::default(); 2];
        theirs.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = theirs.ctx();
        phases::end_turn(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            (ctx.turn(), ctx.turn_player()),
            (2, 0),
            "the end of an opponent's turn is not the end of yours: the turn passes without a trigger"
        );
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.starts_with("{card 90} readies")));
    }
}
