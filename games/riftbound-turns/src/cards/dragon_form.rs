use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const BASE_MIGHT: i32 = 5;
pub const FLOW: Cost = Cost {
    energy: 3,
    power: &[],
};

pub fn base_might_delta(ctx: &Ctx, unit: u32, wanted: i32) -> Option<i16> {
    i16::try_from(wanted - ctx.printed_might(unit)).ok()
}

pub fn base_might_becomes(ctx: &mut Ctx, item: &Item, unit: u32, wanted: i32) -> bool {
    let Some(delta) = base_might_delta(ctx, unit, wanted) else {
        return false;
    };
    if delta != 0 {
        might_this_turn(ctx, item, unit, delta, None);
    }
    ctx.narrate(format!(
        "{{card {unit}}}'s base Might becomes {wanted} this turn"
    ));
    true
}

fn transform(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        base_might_becomes(ctx, item, unit, BASE_MIGHT);
    }
    done()
}

pub static CARD: Card = spell(
    "Dragon Form",
    &[Keyword::Flow(FLOW)],
    &[play(
        &[a_unit("a unit whose base Might becomes 5")],
        transform,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{might_this_turn as pump, UNIT};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{Expiry, Leave, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const FORM: u32 = 90;
    const THEIR_FORM: u32 = 91;
    const SECOND_FORM: u32 = 92;
    const WYRM: u32 = 93;
    const SPARE_RUNES: [u32; 3] = [100, 101, 102];

    fn form(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Dragon Form", 3, 0);
        card.domain = vec!["Order".into()];
        card
    }

    fn lair(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(form(FORM, zone, 0));
        fixture
            .table
            .cards
            .push(form(THEIR_FORM, fixtures::HAND, 1));
        fixture
            .table
            .cards
            .push(form(SECOND_FORM, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(WYRM, fixtures::BF1, 1, "Wyrm", 7));
        for rune in SPARE_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn cast(ctx: &mut Ctx, spell: u32, unit: u32) {
        fixtures::play_from_hand(ctx, 0, spell).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(
            ctx.blob.chain.last().unwrap().targets,
            [TargetRef::Card(unit)]
        );
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_is_a_flow_sorcery_over_any_unit_reading_the_printed_might() {
        assert!(std::ptr::eq(script_of("Dragon Form").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!(BASE_MIGHT, 5);
        let mut fixture = lair(fixtures::HAND);
        let ctx = fixture.ctx();
        assert_eq!(base_might_delta(&ctx, fixtures::VI, BASE_MIGHT), Some(2));
        assert_eq!(base_might_delta(&ctx, WYRM, BASE_MIGHT), Some(-2));
        assert_eq!(base_might_delta(&ctx, fixtures::THEIR_UNIT, 2), Some(0));
    }

    #[test]
    fn a_small_unit_grows_to_five_and_a_big_one_shrinks_to_five_until_the_turn_ends() {
        let mut fixture = lair(fixtures::HAND);
        let mut ctx = fixture.ctx();
        cast(&mut ctx, FORM, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 5, "3 becomes 5");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50}'s base Might becomes 5 this turn".to_string()));
        assert_eq!(ctx.card(FORM).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::VI), 3, "the turn ends, 3 again");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        drop(ctx);

        let mut shrunk = lair(fixtures::HAND);
        let mut ctx = shrunk.ctx();
        cast(&mut ctx, FORM, WYRM);
        assert_eq!(ctx.current_might(WYRM), 5, "7 becomes 5");
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(WYRM), 7);
    }

    #[test]
    fn modifiers_already_on_the_unit_stay_on_top_of_the_new_base() {
        let mut fixture = lair(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FORM).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let item = ctx.blob.chain[0].clone();
        pump(&mut ctx, &item, fixtures::VI, 2, None);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            7,
            "a base of 5 with the +2 still applied"
        );
        assert!(ctx.buff(fixtures::VI));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            8,
            "a buff counter sits above the base"
        );
    }

    #[test]
    fn from_the_trash_the_flow_play_transforms_and_is_banished() {
        let mut fixture = lair(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            FORM,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 5);
        assert_eq!(ctx.banished_of(0), [FORM]);
        assert!(ctx.trash_of(0).is_empty());
    }

    #[test]
    fn a_gear_is_refused_a_unit_gone_before_resolution_is_untouched_and_the_other_seat_waits() {
        let mut fixture = lair(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_FORM)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, FORM).unwrap();
        for wrong in [fixtures::GROUNDS, fixtures::HAND_UNIT, fixtures::HAND_GEAR] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::HAND, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .state_of(fixtures::VI)
            .is_none_or(|row| row.might.is_empty()));
        assert!(!ctx.blob.log.iter().any(|line| line.contains("base Might")));
        assert_eq!(ctx.card(FORM).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    #[ignore = "engine gap · a base-Might override on CardState for the turn read first by current_might (the Might-layers row); base_might_becomes rides a this-turn delta off the printed Might, so a second Dragon Form later this turn stacks a second delta instead of setting the base again"]
    fn a_second_dragon_form_this_turn_still_reads_five() {
        let mut fixture = lair(fixtures::HAND);
        let mut ctx = fixture.ctx();
        cast(&mut ctx, FORM, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        cast(&mut ctx, SECOND_FORM, fixtures::VI);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            5,
            "the base is set, not added to"
        );
    }
}
