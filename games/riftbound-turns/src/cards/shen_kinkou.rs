use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit(
    "Shen - Kinkou",
    &[Keyword::Reaction, Keyword::Shield(2), Keyword::Tank],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::combat;
    use crate::engine::ctx::{Ctx, EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::engine::{act, settle};
    use crate::state::{Origin, FLAG_DEFENDER};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const SHEN: u32 = 90;
    const ALLY: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut shen = fixtures::unit(SHEN, fixtures::HAND, 0, "Shen - Kinkou", 3);
        shen.energy = Some(0);
        shen.domain = vec!["Order".into()];
        fixture.table.cards.push(shen);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().name = "Plain Field".into();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: SHEN,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_the_three_printed_keywords_and_nothing_else() {
        assert!(std::ptr::eq(script_of("Shen - Kinkou").unwrap(), &CARD));
        assert_eq!(
            CARD.keywords,
            [Keyword::Reaction, Keyword::Shield(2), Keyword::Tank]
        );
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn a_reaction_unit_is_played_mid_chain_to_a_held_battlefield_or_the_base_and_a_plain_one_is_not(
    ) {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Ok(Intent::Play {
                card: SHEN,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            }),
            "813.3.a · a Reaction unit is played while the chain is closed, to a battlefield you control"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BASE)),
            Ok(Intent::Play {
                card: SHEN,
                origin: Origin::Hand,
                location: Some(Location::Base(0)),
                on_chain: false,
            })
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF2)),
            Err(Refusal::Illegal(Reason::NotHeld)),
            "Reaction widens the timing, not the locations"
        );
        ctx.table
            .apply_entry(&fixtures::move_action(SHEN, fixtures::BF1, 0), 0)
            .unwrap();
        act(
            &mut ctx,
            0,
            Intent::Play {
                card: SHEN,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(SHEN),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.card(SHEN).unwrap().exhausted,
            "no Accelerate: he enters exhausted"
        );
        assert_eq!(ctx.blob.chain.len(), 1, "no play trigger joins the spell");
        let mut plain = armed();
        plain.table.card_mut(SHEN).unwrap().name = "Ally".into();
        plain.resolve();
        let mut ctx = plain.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
    }

    #[test]
    fn shield_counts_only_while_he_defends_and_tank_puts_him_first_in_the_assignment_order() {
        let mut fixture = armed();
        fixture.table.card_mut(SHEN).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 4));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(SHEN), 3);
        ctx.set_flag(SHEN, FLAG_DEFENDER, true);
        assert_eq!(
            ctx.current_might(SHEN),
            5,
            "Shield 2 while he is a defender"
        );
        ctx.set_flag(SHEN, FLAG_DEFENDER, false);
        assert_eq!(ctx.current_might(SHEN), 3);
        assert_eq!(
            combat::ordered(&ctx, &[ALLY, SHEN, fixtures::VI], false),
            [SHEN],
            "Tank: he must be assigned combat damage before the plain units"
        );
    }
}
