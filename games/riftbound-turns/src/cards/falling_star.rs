use super::prelude::{a_unit, card_target, deal, done, play, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 3;
pub const BOLTS: usize = 2;

fn fall(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for bolt in 0..BOLTS {
        if let Some(unit) = card_target(ctx, item, bolt) {
            if deal(ctx, item, unit, DAMAGE) {
                ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
            }
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Falling Star",
    &[],
    &[play(&[a_unit("a unit"), a_unit("a unit")], fall)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const STAR: u32 = 90;
    const BRUTE: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            STAR,
            fixtures::HAND,
            0,
            "Falling Star",
            2,
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BASE, 1, "Brute", 5));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt {
                    card,
                    n,
                    source: Cause::Item(1),
                } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn falling_star_has_no_timing_keyword_and_chooses_two_units_anywhere() {
        assert!(std::ptr::eq(script_of("Falling Star").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), BOLTS);
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STAR).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "{card 91}", "cancel"],
            "any unit, in a base or at a battlefield"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            damage_events(&ctx),
            [(fixtures::SPRITE, DAMAGE), (fixtures::THEIR_UNIT, DAMAGE)]
        );
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert_eq!(ctx.card(STAR).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn both_bolts_may_name_the_same_unit_for_six() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STAR).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert!(
            fixtures::labels(&ctx).contains(&"{card 91}".to_string()),
            "the second choice is its own and may repeat the first"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(damage_events(&ctx), [(BRUTE, DAMAGE), (BRUTE, DAMAGE)]);
        assert!(!ctx.on_board(BRUTE), "six damage kills the 5-Might Brute");
    }

    #[test]
    fn it_is_refused_on_the_opponents_turn_and_a_gone_target_takes_nothing() {
        let mut fixture = armed();
        fixture.blob = crate::state::GameBlob::start(2, 1, crate::state::Mode::Enforced);
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.set_phase(crate::state::Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: STAR,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotYourTurn),
            "without Action or Reaction it plays only on its controller's turn"
        );
        drop(ctx);
        let mut mine = armed();
        let mut ctx = mine.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STAR).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        ctx.bounce(fixtures::SPRITE);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            damage_events(&ctx),
            [(BRUTE, DAMAGE)],
            "359.3.e.8 · the bolt at the gone unit is ignored, the other lands"
        );
    }
}
