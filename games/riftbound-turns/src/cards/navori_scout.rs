use super::prelude::unit;
use super::{Card, Keyword};

pub const DEFLECT: u8 = 1;

pub static CARD: Card = unit("Navori Scout", &[Keyword::Deflect(DEFLECT)], &[]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_card, play, spell, MOVABLE_UNIT};
    use crate::cards::{script_of, Flow};
    use crate::engine::ctx::{Ctx, EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play as play_engine, settle};
    use crate::state::{Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const SCOUT: u32 = 90;
    const ZAP: u32 = 91;
    const ENERGY: u8 = 4;
    const MIGHT: u8 = 4;

    static SPARK: Card = spell(
        "Spark",
        &[],
        &[play(&[a_card(MOVABLE_UNIT, "a unit")], |_, _, _| {
            Flow::Done
        })],
    );

    fn scout(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            domain: vec!["Calm".into()],
            ..fixtures::unit(SCOUT, zone, seat, "Navori Scout", MIGHT)
        }
    }

    fn scouting_for(seat: u8, runes: &[u32]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(scout(fixtures::BASE, seat));
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1 || runes.contains(&card.id));
        fixture
            .table
            .cards
            .push(fixtures::spell(ZAP, fixtures::HAND, 0, "Spark", 1, 1));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ZAP, &SPARK);
        fixture
    }

    fn play_spell(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        let entry = EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        legal::classify(ctx, seat, &entry)?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play_engine::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn item_of(ctx: &Ctx) -> u16 {
        match ctx.blob.why {
            Some(PromptWhy::Target { item, .. }) => item,
            why => panic!("a target prompt was expected, got {why:?}"),
        }
    }

    #[test]
    fn the_script_is_a_deflect_one_unit_with_no_abilities() {
        assert!(std::ptr::eq(script_of("Navori Scout").unwrap(), &CARD));
        assert_eq!(CARD.name, "Navori Scout");
        assert_eq!(CARD.keywords, &[Keyword::Deflect(1)]);
        assert!(CARD.abilities.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        let mut fixture = scouting_for(1, &[41]);
        assert!(std::ptr::eq(fixture.scripts.of_card(SCOUT).unwrap(), &CARD));
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(SCOUT, Keyword::Deflect(1)));
        assert_eq!(ctx.deflect_of(SCOUT), DEFLECT);
        assert_eq!(ctx.location(SCOUT), Some(Location::Base(1)));
    }

    #[test]
    fn an_opponents_spell_cannot_choose_it_without_a_spare_rune_for_the_rainbow() {
        let mut fixture = scouting_for(1, &[41]);
        let mut ctx = fixture.ctx();
        play_spell(&mut ctx, 0, ZAP).unwrap();
        let item = item_of(&ctx);
        let offered = fixtures::labels(&ctx);
        assert!(
            !offered.contains(&format!("{{card {SCOUT}}}")),
            "one rune pays the spell but not the deflect: {offered:?}"
        );
        assert!(
            offered.contains(&format!("{{card {}}}", fixtures::VI)),
            "the caster's own unit costs nothing extra: {offered:?}"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[SCOUT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "735.1.c · the rainbow is a mandatory additional cost"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.effects.is_empty(), "nothing was paid");
    }

    #[test]
    fn with_a_second_rune_the_opponent_may_choose_it_and_pays_the_rainbow_on_top() {
        let mut fixture = scouting_for(1, &[41, 42]);
        let mut ctx = fixture.ctx();
        play_spell(&mut ctx, 0, ZAP).unwrap();
        let offered = fixtures::labels(&ctx);
        assert!(
            offered.contains(&format!("{{card {SCOUT}}}")),
            "a second rune buys the deflect: {offered:?}"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert!(
            ctx.ready_runes_of(0).is_empty(),
            "one energy for Spark and one rune more for the Scout"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(SCOUT)]);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn its_own_controller_pays_no_deflect() {
        let mut fixture = scouting_for(0, &[41]);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.location(SCOUT), Some(Location::Base(0)));
        play_spell(&mut ctx, 0, ZAP).unwrap();
        let offered = fixtures::labels(&ctx);
        assert!(
            offered.contains(&format!("{{card {SCOUT}}}")),
            "735.1 · Deflect taxes opponents only: {offered:?}"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SCOUT}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(SCOUT)]);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }
}
