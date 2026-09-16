use super::prelude::{open_battlefields, unit, with_statics, Location};
use super::{Card, Keyword, Static};
use crate::engine::ctx::Ctx;

pub static CARD: Card = with_statics(
    unit("Sai Scout", &[Keyword::Vision], &[]),
    &[Static::PlayLocations(open_play_locations)],
);

pub fn open_play_locations(ctx: &Ctx, _: u8, card: u32) -> Vec<Location> {
    if ctx
        .script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
    {
        open_battlefields(ctx)
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent};
    use crate::state::{Origin, PromptWhy};
    use agni_plugin_sdk::decide::{Effect, TOP};

    const SCOUT: u32 = 90;
    const MY_TOP: u32 = 23;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut scout = fixtures::unit(SCOUT, fixtures::HAND, 0, "Sai Scout", 5);
        scout.energy = Some(0);
        scout.domain = vec!["Chaos".into()];
        fixture.table.cards.push(scout);
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: SCOUT,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn to_base(ctx: &mut Ctx) {
        ctx.table
            .apply_entry(&fixtures::move_action(SCOUT, fixtures::BASE, 0), 0)
            .unwrap();
        crate::engine::play::begin(ctx, 0, SCOUT, Origin::Hand, Some(Location::Base(0))).unwrap();
        crate::engine::settle(ctx).unwrap();
    }

    #[test]
    fn the_script_prints_vision_and_names_the_open_battlefields_as_its_own_play_locations() {
        assert!(std::ptr::eq(script_of("Sai Scout").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Vision]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.grants_play_locations());
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(SCOUT, Keyword::Vision));
        assert_eq!(
            open_play_locations(&ctx, 0, SCOUT),
            [Location::Battlefield(fixtures::BF1)],
            "the one open battlefield; the enemy's held one is not"
        );
        assert!(open_play_locations(&ctx, 0, fixtures::HAND_UNIT).is_empty());
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Ok(Intent::Play {
                card: SCOUT,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            })
        );
    }

    #[test]
    fn vision_looks_at_the_top_card_when_he_is_played_and_may_recycle_it() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        to_base(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::Peek { card, seat: 0 } if *card == MY_TOP)));
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP}}}")).unwrap();
        let deck = ctx.zones.main_deck.unwrap();
        assert_eq!(
            ctx.table.held(deck, 0).next().map(|card| card.id),
            Some(MY_TOP),
            "recycled to the bottom"
        );
    }

    #[test]
    fn he_may_be_played_to_an_open_battlefield() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Ok(Intent::Play {
                card: SCOUT,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            })
        );
    }
}
