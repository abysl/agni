use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit("Master Bingwen", &[Keyword::Weaponmaster], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::WEAPONMASTER;
    use crate::cards::prelude::{attached_to, is_attached, weaponmaster_cost};
    use crate::cards::Domain;
    use crate::cards::IMPLICIT_WEAPONMASTER;
    use crate::cards::{script_of, Trigger};
    use crate::engine::cost::Need;
    use crate::engine::ctx::{Ctx, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const BINGWEN: u32 = 90;
    const BRUTALIZER: u32 = 91;
    const LOOT: u32 = 92;
    const ENERGY: u8 = 6;
    const EXTRA_RUNES: [u32; 3] = [100, 101, 102];
    const MIGHT: u8 = 6;

    fn bingwen(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(BINGWEN, zone, seat, "Master Bingwen", MIGHT)
        }
    }

    fn armory(equipment: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bingwen(fixtures::HAND, 0));
        let gear = if equipment {
            fixtures::gear(BRUTALIZER, fixtures::BASE, 0, "Brutalizer", 2)
        } else {
            fixtures::gear(LOOT, fixtures::BASE, 0, "Loot", 2)
        };
        fixture.table.cards.push(gear);
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(BINGWEN).unwrap(),
            &CARD
        ));
        fixture
    }

    fn play(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, BINGWEN).unwrap();
        assert_eq!(ctx.location(BINGWEN), Some(Location::Base(0)));
        assert!(
            ctx.card(BINGWEN).unwrap().exhausted,
            "a played unit enters exhausted"
        );
    }

    #[test]
    fn the_script_is_a_weaponmaster_unit_with_the_shared_optional_play_trigger() {
        assert!(std::ptr::eq(script_of("Master Bingwen").unwrap(), &CARD));
        assert_eq!(CARD.name, "Master Bingwen");
        assert_eq!(CARD.keywords, &[Keyword::Weaponmaster]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert!(
            CARD.abilities.is_empty(),
            "821 · Weaponmaster is the engine's"
        );
        assert_eq!(WEAPONMASTER.trigger, Trigger::Play);
        assert_eq!(WEAPONMASTER.label, Some("weaponmaster"));
        assert!(WEAPONMASTER.optional, "you may");
        assert_eq!(WEAPONMASTER.targets.len(), 1);
        assert_eq!(WEAPONMASTER.targets[0].min, 0);
        assert_eq!(WEAPONMASTER.targets[0].max, 1);
    }

    #[test]
    fn played_beside_an_equipment_it_offers_the_gear_and_attaches_it_for_the_equip_cost_less_a_rainbow(
    ) {
        let mut fixture = armory(true);
        let mut ctx = fixture.ctx();
        play(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BRUTALIZER}}}"), "skip".to_string()],
            "821.1.c · an Equipment you control, or nothing"
        );
        let cost = weaponmaster_cost(&ctx, BRUTALIZER).unwrap();
        assert_eq!(cost.energy, 0);
        assert_eq!(
            cost.power,
            [Need::Domain(Domain::Calm)],
            "821.1.c.3 · a domain need is not the rainbow"
        );
        let runes = ctx.runes_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTALIZER}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: IMPLICIT_WEAPONMASTER } if source == BINGWEN
        ));
        assert!(
            !is_attached(&ctx, BRUTALIZER),
            "nothing until the trigger resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, BRUTALIZER), Some(BINGWEN));
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "the Calm rune is recycled"
        );
        assert_eq!(
            ctx.current_might(BINGWEN),
            i32::from(MIGHT) + 3,
            "the Brutalizer's +1 and its fresh +2"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn skipping_the_offer_leaves_the_gear_where_it_is() {
        let mut fixture = armory(true);
        let mut ctx = fixture.ctx();
        play(&mut ctx);
        let runes = ctx.runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, BRUTALIZER));
        assert_eq!(ctx.runes_of(0).len(), runes, "nothing paid");
        assert_eq!(ctx.current_might(BINGWEN), i32::from(MIGHT));
    }

    #[test]
    fn a_gear_that_is_not_equipment_is_never_offered() {
        let mut fixture = armory(false);
        let mut ctx = fixture.ctx();
        play(&mut ctx);
        assert!(
            !fixtures::labels(&ctx).contains(&format!("{{card {LOOT}}}")),
            "Loot prints no Equip: {:?}",
            fixtures::labels(&ctx)
        );
        if ctx.blob.prompt.is_some() {
            fixtures::choose(&mut ctx, 0, "skip").unwrap();
        }
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, LOOT));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }
}
