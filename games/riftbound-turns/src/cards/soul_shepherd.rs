use super::prelude::{unit, with_statics};
use super::{Card, Grant, Scope, Static};
use crate::engine::ctx::Ctx;

pub const FLOCK_MIGHT: i16 = 1;

fn a_friendly_token(ctx: &Ctx, _: u32, unit: u32) -> bool {
    ctx.is_token(unit)
}

pub static CARD: Card = with_statics(
    unit("Soul Shepherd", &[], &[]),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: a_friendly_token,
        grants: &[Grant::Might(FLOCK_MIGHT)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::script_of;
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use agni_plugin_sdk::table::CardInfo;

    const SHEPHERD: u32 = 90;
    const THEIR_SHEPHERD: u32 = 91;

    fn shepherd(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, "Soul Shepherd", 3);
        card.domain = vec!["Mind".into()];
        card.energy = Some(5);
        card
    }

    fn pasture(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shepherd(SHEPHERD, zone, 0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SHEPHERD).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_plain_unit_with_one_might_aura_over_your_tokens() {
        assert!(std::ptr::eq(script_of("Soul Shepherd").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(
            CARD.statics[0],
            Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Might(1)],
                ..
            }
        ));
        assert!(CARD.has_aura());
        assert_eq!(FLOCK_MIGHT, 1);
    }

    #[test]
    fn your_tokens_anywhere_have_one_more_might_and_enemy_tokens_and_cards_do_not() {
        let mut fixture = pasture(fixtures::BF1);
        let mut ctx = fixture.ctx();
        let in_base = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        let here = spawn(
            &mut ctx,
            0,
            Token::Sprite,
            Location::Battlefield(fixtures::BF1),
            true,
        )
        .unwrap();
        assert_eq!(ctx.current_might(in_base), 4, "a token in the base");
        assert_eq!(ctx.current_might(here), 4, "a token at her battlefield");
        assert_eq!(ctx.projected_might(in_base), 1);
        assert!(statics::grants_on(&ctx, in_base)
            .iter()
            .any(|grant| matches!(grant, Grant::Might(1))));
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            3,
            "the enemy Sprite is not yours"
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3, "Vi is a card");
        assert_eq!(ctx.current_might(SHEPHERD), 3, "not itself");
        let gold = spawn(&mut ctx, 0, Token::Gold, Location::Base(0), true).unwrap();
        assert!(
            statics::grants_on(&ctx, gold).is_empty(),
            "a Gold is a token gear, and the aura reaches units"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_aura_needs_the_shepherd_in_play_and_leaves_with_it() {
        let mut fixture = pasture(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let sprite = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        assert_eq!(ctx.current_might(sprite), 3, "in hand it projects nothing");
        drop(ctx);
        let mut fixture = pasture(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let sprite = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        assert_eq!(ctx.current_might(sprite), 4);
        ctx.kill(SHEPHERD, Cause::Rule);
        assert!(!ctx.on_board(SHEPHERD));
        assert_eq!(ctx.current_might(sprite), 3, "the buff leaves with him");
    }

    #[test]
    fn each_shepherd_buffs_its_own_controllers_tokens_and_a_stolen_token_changes_flocks() {
        let mut fixture = pasture(fixtures::BASE);
        fixture
            .table
            .cards
            .push(shepherd(THEIR_SHEPHERD, fixtures::BASE, 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let mine = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), true).unwrap();
        assert_eq!(ctx.current_might(mine), 4);
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            4,
            "their Shepherd buffs their Sprite"
        );
        ctx.set_controller(fixtures::SPRITE, 0, SHEPHERD);
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            4,
            "stolen, it is now yours: your Shepherd's buff replaces theirs"
        );
    }
}
