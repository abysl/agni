use super::prelude::{deal, done, play, spell, units_at_battlefields};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 1;

fn flurry(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for unit in units_at_battlefields(ctx) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Flurry of Blades",
    &[Keyword::Reaction],
    &[play(&[], flurry)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{Amount, DamageSource, Expiry};
    use agni_plugin_sdk::table::CardInfo;

    const FLURRY: u32 = 90;
    const SCOUT: u32 = 91;
    const WEAKLING: u32 = 92;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Body".into()],
            ..fixtures::spell(FLURRY, fixtures::HAND, 0, "Flurry of Blades", 1, 0)
        });
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(WEAKLING, fixtures::BF2, 1, "Weakling", 1));
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
    fn flurry_of_blades_is_a_reaction_that_deals_one_to_every_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Flurry of Blades").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLURRY).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing to choose");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            damage_events(&ctx),
            [
                (SCOUT, DAMAGE),
                (fixtures::SPRITE, DAMAGE),
                (WEAKLING, DAMAGE)
            ],
            "friend and foe alike, battlefield by battlefield"
        );
        assert_eq!(ctx.damage_on(fixtures::VI), 0, "units in a base are spared");
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(!ctx.on_board(WEAKLING), "one kills one Might");
        assert!(ctx.on_board(SCOUT));
        assert!(ctx.on_board(fixtures::SPRITE));
        assert_eq!(ctx.card(FLURRY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn under_full_prevention_nothing_is_marked_and_with_no_unit_out_it_does_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.prevent(
            DamageSource::SpellOrAbility,
            Amount::All,
            Expiry::EndOfTurn(ctx.turn()),
        );
        fixtures::play_from_hand(&mut ctx, 0, FLURRY).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(damage_events(&ctx).is_empty());
        assert!(ctx.on_board(WEAKLING));
        let mut empty = armed();
        empty
            .table
            .cards
            .retain(|card| ![SCOUT, WEAKLING, fixtures::SPRITE].contains(&card.id));
        empty.resolve();
        let mut ctx = empty.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FLURRY).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(damage_events(&ctx).is_empty());
        assert_eq!(ctx.card(FLURRY).unwrap().zone, Some(fixtures::TRASH));
    }
}
