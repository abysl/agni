use super::prelude::{a_card, card_target, done, kill, play, spell};
use super::{Card, Domain, Filter, Flow, Item, Stage};
use crate::engine::ctx::{Ctx, Killed};

pub const ENEMY_CHAOS_UNIT_OR_GEAR: Filter = Filter::And(&[
    Filter::Or(&[Filter::Unit, Filter::Gear]),
    Filter::Enemy,
    Filter::Domain(Domain::Chaos),
]);

fn decree(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(card) = card_target(ctx, item, 0) {
        if kill(ctx, item, card) == Killed::Yes {
            ctx.narrate(format!("{{card {card}}} dies"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Decree of Unity",
    &[],
    &[play(
        &[a_card(
            ENEMY_CHAOS_UNIT_OR_GEAR,
            "an enemy Chaos unit or gear to kill",
        )],
        decree,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Keyword, TargetKind, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const DECREE: u32 = 90;
    const THEIR_DECREE: u32 = 91;
    const THEIR_CHAOS_UNIT: u32 = 92;
    const THEIR_CHAOS_GEAR: u32 = 93;
    const THEIR_FURY_GEAR: u32 = 94;
    const MY_CHAOS_UNIT: u32 = 95;
    const ORDER_RUNE: u32 = 100;

    fn decree_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Decree of Unity", 2, 1);
        card.domain = vec!["Order".into()];
        card
    }

    fn chaos(card: CardInfo) -> CardInfo {
        CardInfo {
            domain: vec!["Chaos".into()],
            ..card
        }
    }

    fn court() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(decree_card(DECREE, 0));
        fixture.table.cards.push(decree_card(THEIR_DECREE, 1));
        fixture.table.cards.push(chaos(fixtures::unit(
            THEIR_CHAOS_UNIT,
            fixtures::BF1,
            1,
            "Cultist",
            3,
        )));
        fixture.table.cards.push(chaos(fixtures::gear(
            THEIR_CHAOS_GEAR,
            fixtures::BASE,
            1,
            "Idol",
            2,
        )));
        fixture.table.cards.push(fixtures::gear(
            THEIR_FURY_GEAR,
            fixtures::BASE,
            1,
            "Trinket",
            2,
        ));
        fixture.table.cards.push(chaos(fixtures::unit(
            MY_CHAOS_UNIT,
            fixtures::BASE,
            0,
            "Acolyte",
            2,
        )));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
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

    #[test]
    fn the_script_is_a_plain_sorcery_over_an_enemy_chaos_unit_or_gear() {
        assert!(std::ptr::eq(script_of("Decree of Unity").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Flow(crate::cards::Cost::FREE)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, ENEMY_CHAOS_UNIT_OR_GEAR);
        assert_eq!(
            (
                ability.targets[0].min,
                ability.targets[0].max,
                ability.targets[0].kind
            ),
            (1, 1, TargetKind::Card)
        );
    }

    #[test]
    fn only_enemy_chaos_units_and_gear_are_offered_and_the_chosen_unit_dies() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_CHAOS_UNIT}}}"),
                format!("{{card {THEIR_CHAOS_GEAR}}}"),
                "cancel".to_string()
            ],
            "the Fury units and gear, the Sprite and my own Chaos Acolyte are out"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_CHAOS_UNIT}}}")).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(THEIR_CHAOS_UNIT)]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(THEIR_CHAOS_UNIT).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, unit: true, .. } if *card == THEIR_CHAOS_UNIT)
        ));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {THEIR_CHAOS_UNIT}}} dies")));
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "the lone unit at the battlefield is gone"
        );
        assert_eq!(ctx.card(DECREE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn an_enemy_chaos_gear_dies_the_same_way() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_CHAOS_GEAR}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.card(THEIR_CHAOS_GEAR).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, unit: false, .. } if *card == THEIR_CHAOS_GEAR)
        ));
        assert!(ctx.on_board(THEIR_FURY_GEAR));
        assert!(ctx.on_board(THEIR_CHAOS_UNIT));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_wrong_domain_side_or_zone_is_refused_and_the_other_seat_waits() {
        let mut fixture = court();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DECREE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        for wrong in [
            fixtures::THEIR_UNIT,
            THEIR_FURY_GEAR,
            MY_CHAOS_UNIT,
            fixtures::ROCKFALL,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not Chaos, not an enemy's, or not a unit or gear on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_CHAOS_UNIT}}}")).unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(THEIR_CHAOS_UNIT, fixtures::HAND, 1),
                1,
            )
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(THEIR_CHAOS_UNIT).unwrap().zone,
            Some(fixtures::HAND),
            "a target gone before resolution is left alone"
        );
        assert!(!ctx.blob.log.iter().any(|line| line.ends_with(" dies")));
        assert_eq!(ctx.card(DECREE).unwrap().zone, Some(fixtures::TRASH));
    }
}
