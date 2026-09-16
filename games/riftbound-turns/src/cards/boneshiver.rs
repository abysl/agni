use super::prelude::{
    channel_exhausted, done, equip, gear, on_conquer_me, while_attached, with_statics,
};
use super::{Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, GRANTED};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Body)],
};

pub const MIGHT_BONUS: i16 = 2;
pub const RUNES: usize = 1;
pub const ON_CONQUER: u8 = GRANTED;

fn shiver(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let channelled = channel_exhausted(ctx, seat, RUNES);
    ctx.narrate(format!(
        "{{seat {seat}}} channels {channelled} rune{} exhausted",
        if channelled == 1 { "" } else { "s" }
    ));
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [on_conquer_me(&[], shiver)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear("Boneshiver", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{conquered, equipment, queue_granted, GEAR};
    use crate::cards::prelude::attach_gear;
    use crate::cards::{script_of, Static, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{settle, triggers};
    use crate::state::TargetRef;

    fn pool(ctx: &Ctx, seat: u8) -> Vec<(u32, bool)> {
        ctx.table
            .held(fixtures::RUNE_POOL, seat)
            .map(|rune| (rune.id, rune.exhausted))
            .collect()
    }

    fn rune_deck(ctx: &Ctx, seat: u8) -> usize {
        ctx.table.held(fixtures::RUNE_DECK, seat).count()
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Boneshiver", 3, "Body"));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_one_energy_body_equipment_with_two_might_and_a_wearer_conquer_listener() {
        assert!(std::ptr::eq(script_of("Boneshiver").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(EQUIP.energy, 1);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].cost, Some(EQUIP));
        let listener = &WEARER_TEXT[0];
        assert_eq!(listener.trigger, Trigger::Conquer(Who::Me));
        assert!(listener.condition.is_none());
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(2), Grant::Ability(_)]
        ));
    }

    #[test]
    fn the_wearers_conquer_channels_one_rune_exhausted() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        let before = pool(&ctx, 0);
        let deck = rune_deck(&ctx, 0);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let after = pool(&ctx, 0);
        assert_eq!(after.len(), before.len() + RUNES);
        assert_eq!(rune_deck(&ctx, 0), deck - RUNES);
        assert!(
            after
                .iter()
                .filter(|rune| !before.contains(rune))
                .all(|(_, exhausted)| *exhausted),
            "the channelled rune arrives exhausted"
        );
        assert_eq!(pool(&ctx, 1).len(), 2, "the other seat channels nothing");
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 1 rune exhausted".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_empty_rune_deck_channels_nothing_and_the_engine_hears_the_attached_gear() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::RUNE_DECK) || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let before = pool(&ctx, 0).len();
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(pool(&ctx, 0).len(), before);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} channels 0 runes exhausted".to_string()));
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(
            triggers::collect(&mut ctx),
            1,
            "136.2.c · the effect text is the wearer's own"
        );
    }

    #[test]
    fn a_conquer_by_the_wearer_channels_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        let before = pool(&ctx, 0).len();
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(pool(&ctx, 0).len(), before + RUNES);
    }
}
