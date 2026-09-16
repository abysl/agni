use super::prelude::{friendly_gear, unit, with_statics};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const LADDER: usize = 16;

pub fn friendly_gear_count(ctx: &Ctx, card: u32) -> usize {
    friendly_gear(ctx, ctx.controller(card)).len()
}

pub fn might_bonus(ctx: &Ctx, card: u32) -> i16 {
    i16::try_from(friendly_gear_count(ctx, card).min(LADDER)).unwrap_or(i16::MAX)
}

fn at_least<const N: usize>(ctx: &Ctx, card: u32, _: u32) -> bool {
    friendly_gear_count(ctx, card) >= N
}

fn always(_: &Ctx, _: u32) -> bool {
    true
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Grant::MightIf(at_least::<$step>, 1)),+]
    };
}

pub static GEAR: &[Grant] = ladder!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

pub static CARD: Card = with_statics(
    unit(
        "Ornn - Forge God",
        &[Keyword::Deflect(2), Keyword::Weaponmaster],
        &[],
    ),
    &[Static::While(always, GEAR)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::WEAPONMASTER;
    use crate::cards::prelude::{attach_gear, attached_to, spawn_gold, Attached};
    use crate::cards::{script_of, Trigger};
    use crate::engine::cost;
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::statics;
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy};

    const ORNN: u32 = 90;
    const SWORD: u32 = 91;
    const THEIR_GEAR: u32 = 92;
    const HAMMER: u32 = 93;
    const BOOTS: u32 = 94;
    const CHAOS_RUNE: u32 = 46;

    fn forge() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            ORNN,
            fixtures::BASE,
            0,
            "Ornn - Forge God",
            4,
        ));
        fixture
            .table
            .cards
            .push(fixtures::gear(SWORD, fixtures::BASE, 0, "Anvil", 4));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Theirs", 2));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ORNN).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_a_deflect_two_weaponmaster_whose_while_is_one_might_per_friendly_gear() {
        assert!(std::ptr::eq(script_of("Ornn - Forge God").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Deflect(2), Keyword::Weaponmaster]);
        assert!(
            CARD.abilities.is_empty(),
            "821 · Weaponmaster is the engine's"
        );
        assert_eq!(WEAPONMASTER.trigger, Trigger::Play);
        assert!(WEAPONMASTER.optional, "Weaponmaster is a may");
        assert!(matches!(CARD.statics, [Static::While(_, _)]));
        assert_eq!(GEAR.len(), LADDER);
        assert!(GEAR
            .iter()
            .all(|grant| matches!(grant, Grant::MightIf(_, 1))));
    }

    #[test]
    fn his_might_counts_your_gear_attached_or_not_and_tokens_but_never_theirs() {
        let mut fixture = forge();
        let mut ctx = fixture.ctx();
        assert_eq!(friendly_gear_count(&ctx, ORNN), 1, "the sword alone");
        assert_eq!(might_bonus(&ctx, ORNN), 1);
        assert_eq!(statics::grants_on(&ctx, ORNN).len(), 1);
        assert_eq!(ctx.current_might(ORNN), 5);
        assert!(spawn_gold(&mut ctx, 0, true).is_some());
        assert_eq!(ctx.current_might(ORNN), 6, "a Gold token is a gear");
        assert!(spawn_gold(&mut ctx, 1, true).is_some());
        assert_eq!(ctx.current_might(ORNN), 6, "their Gold is not yours");
        assert_eq!(
            attach_gear(&mut ctx, fixtures::HAND_GEAR, ORNN),
            Attached::NotGear,
            "gear in hand is not on the board"
        );
        assert_eq!(attach_gear(&mut ctx, SWORD, ORNN), Attached::Yes);
        assert_eq!(
            ctx.current_might(ORNN),
            6,
            "attached gear still counts once and a badgeless gear adds nothing"
        );
        assert_eq!(ctx.deflect_of(ORNN), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_gear_that_dies_or_changes_hands_leaves_his_count_at_once() {
        let mut fixture = forge();
        fixture
            .table
            .cards
            .push(fixtures::gear(HAMMER, fixtures::BASE, 0, "Hammer", 3));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(ORNN), 6);
        assert!(ctx.set_controller(HAMMER, 1, THEIR_GEAR));
        assert_eq!(ctx.current_might(ORNN), 5, "the hammer is theirs now");
        assert!(ctx.set_controller(THEIR_GEAR, 0, ORNN));
        assert_eq!(ctx.current_might(ORNN), 6, "and their gear is yours");
        assert_eq!(ctx.kill(SWORD, Cause::Rule), Killed::Yes);
        assert_eq!(ctx.current_might(ORNN), 5);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn played_from_hand_he_pays_the_printed_cost_then_weaponmaster_equips_the_boots_and_both_count()
    {
        let mut fixture = forge();
        fixture
            .table
            .cards
            .retain(|card| card.id != SWORD && card.id != fixtures::HAND_GEAR);
        fixture.table.card_mut(ORNN).unwrap().zone = Some(fixtures::HAND);
        fixture.table.card_mut(ORNN).unwrap().energy = Some(1);
        fixture.table.card_mut(ORNN).unwrap().domain = vec!["Mind".into()];
        let mut boots = fixtures::gear(BOOTS, fixtures::BASE, 0, "Boots of Swiftness", 3);
        boots.domain = vec!["Chaos".into()];
        fixture.table.cards.push(boots);
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let item = ChainItem::new(1, ItemKind::Permanent { card: ORNN }, 0, Origin::Hand);
        assert_eq!(
            cost::of_item(&ctx, &item, None).energy,
            1,
            "no discount of his own"
        );
        assert!(
            statics::grants_on(&ctx, ORNN).is_empty(),
            "in hand he projects nothing"
        );
        fixtures::play_from_hand(&mut ctx, 0, ORNN).unwrap();
        assert!(ctx.on_board(ORNN));
        assert_eq!(
            ctx.current_might(ORNN),
            5,
            "the boots on the board count already"
        );
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BOOTS}}}"), "skip".to_string()],
            "the Weaponmaster may offers the Equipment or nothing"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BOOTS}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            attached_to(&ctx, BOOTS),
            Some(ORNN),
            "the Equip cost is one Chaos, which weaponmaster_cost leaves standing since it strikes a Rainbow need only"
        );
        assert_eq!(ctx.runes_of(0).len(), 4, "the Chaos rune paid the Equip");
        assert_eq!(
            ctx.current_might(ORNN),
            7,
            "the boots' badge and their count on the ladder"
        );
        assert!(ctx.has_keyword(ORNN, Keyword::Ganking));
        assert!(ctx.fault.is_none());
    }
}
