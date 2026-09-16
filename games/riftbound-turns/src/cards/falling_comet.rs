use super::prelude::{a_unit_at_a_battlefield, card_target, deal, done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 6;

fn comet(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Falling Comet",
    &[Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        comet,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play;
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const COMET: u32 = 90;
    const COLOSSUS: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::spell(COMET, fixtures::HAND, 0, "Falling Comet", 5, 0)
        });
        fixture
            .table
            .cards
            .push(fixtures::unit(COLOSSUS, fixtures::BF1, 1, "Colossus", 7));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Mind", false));
        fixture.resolve();
        fixture
    }

    #[test]
    fn falling_comet_is_an_action_that_deals_six_to_a_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Falling Comet").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, COMET).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 91}", "cancel"],
            "the units at battlefields, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: COLOSSUS,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(COLOSSUS), 6);
        assert!(
            ctx.on_board(COLOSSUS),
            "six damage is not lethal to seven Might"
        );
        assert_eq!(ctx.card(COMET).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_in_a_base_is_refused() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, COMET).unwrap();
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(
                crate::engine::legal::Reason::NotALegalTarget
            ))
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(fixtures::SPRITE), "six kills the Sprite");
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
    }
}
