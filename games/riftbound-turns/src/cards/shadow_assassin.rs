use super::prelude::{unit, with_statics};
use super::{base_name, Card, Static};
use crate::engine::ctx::Ctx;

pub fn namesakes_in_my_trash(ctx: &Ctx, me: u32) -> Vec<u32> {
    let Some(name) = ctx.card(me).map(|held| base_name(&held.name).to_string()) else {
        return Vec::new();
    };
    ctx.trash_of(ctx.controller(me))
        .into_iter()
        .filter(|card| {
            ctx.card(*card)
                .is_some_and(|held| base_name(&held.name) == name)
        })
        .collect()
}

pub fn enters_ready(ctx: &Ctx, me: u32) -> bool {
    !namesakes_in_my_trash(ctx, me).is_empty()
}

pub static CARD: Card = with_statics(
    unit("Shadow Assassin", &[], &[]),
    &[Static::EntersReady(enters_ready)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use agni_plugin_sdk::table::CardInfo;

    const ASSASSIN: u32 = 90;
    const FALLEN: u32 = 91;
    const THEIR_FALLEN: u32 = 92;
    const ALTERNATE: u32 = 93;

    fn assassin(id: u32, zone: u16, seat: u8, name: &str) -> CardInfo {
        CardInfo {
            energy: Some(5),
            domain: vec!["Fury".into()],
            ..fixtures::unit(id, zone, seat, name, 5)
        }
    }

    fn dojo(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(assassin(ASSASSIN, zone, 0, "Shadow Assassin"));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ASSASSIN).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_a_plain_unit_whose_entry_rule_is_the_enters_ready_static() {
        assert!(std::ptr::eq(script_of("Shadow Assassin").unwrap(), &CARD));
        assert_eq!(CARD.name, "Shadow Assassin");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(CARD.has_static(Static::EntersReady(enters_ready)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn the_seam_reads_a_card_of_his_name_in_his_controllers_trash() {
        let mut fixture = dojo(fixtures::BASE);
        let ctx = fixture.ctx();
        assert!(namesakes_in_my_trash(&ctx, ASSASSIN).is_empty());
        assert!(!enters_ready(&ctx, ASSASSIN), "an empty trash");
        drop(ctx);
        fixture.table.cards.push(assassin(
            THEIR_FALLEN,
            fixtures::TRASH,
            1,
            "Shadow Assassin",
        ));
        fixture
            .table
            .cards
            .push(fixtures::spell(94, fixtures::TRASH, 0, "Spent", 1, 0));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, ASSASSIN),
            "the opponent's trash is not yours and another name is not his"
        );
        drop(ctx);
        fixture
            .table
            .cards
            .push(assassin(FALLEN, fixtures::TRASH, 0, "Shadow Assassin"));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(namesakes_in_my_trash(&ctx, ASSASSIN), [FALLEN]);
        assert!(enters_ready(&ctx, ASSASSIN));
        assert!(
            enters_ready(&ctx, THEIR_FALLEN),
            "read from the entering unit's controller and name"
        );
        assert!(!enters_ready(&ctx, fixtures::VI), "no Vi lies in the trash");
        assert!(!enters_ready(&ctx, 999));
    }

    #[test]
    fn a_print_suffix_is_not_part_of_the_name_and_a_namesake_on_the_board_is_not_in_the_trash() {
        let mut fixture = dojo(fixtures::BASE);
        fixture.table.cards.push(assassin(
            ALTERNATE,
            fixtures::TRASH,
            0,
            "Shadow Assassin (Alternate Art)",
        ));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(namesakes_in_my_trash(&ctx, ASSASSIN), [ALTERNATE]);
        assert!(enters_ready(&ctx, ASSASSIN));
        drop(ctx);
        let mut fixture = dojo(fixtures::BASE);
        fixture
            .table
            .cards
            .push(assassin(FALLEN, fixtures::BF1, 0, "Shadow Assassin"));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(
            !enters_ready(&ctx, ASSASSIN),
            "a namesake on the board is not in the trash"
        );
    }

    #[test]
    fn played_with_a_namesake_in_the_trash_he_enters_ready() {
        let mut fixture = dojo(fixtures::HAND);
        fixture
            .table
            .cards
            .push(assassin(FALLEN, fixtures::TRASH, 0, "Shadow Assassin"));
        for rune in [100, 101, 102] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(enters_ready(&ctx, ASSASSIN));
        fixtures::play_from_hand(&mut ctx, 0, ASSASSIN).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(ASSASSIN));
        assert!(!ctx.card(ASSASSIN).unwrap().exhausted);
    }
}
