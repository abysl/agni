use super::prelude::{a_card, card_target, done, kill, play, spell, GEAR};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::{Ctx, Killed};

pub const FLOW: Cost = Cost {
    energy: 4,
    power: &[Power::Domain(Domain::Fury)],
};

fn shatter(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(gear) = card_target(ctx, item, 0) {
        if kill(ctx, item, gear) == Killed::Yes {
            ctx.narrate(format!("{{card {gear}}} shatters"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Brittle Steel",
    &[Keyword::Flow(FLOW)],
    &[play(&[a_card(GEAR, "a gear to kill")], shatter)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::state::{Leave, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const STEEL: u32 = 90;
    const THEIR_STEEL: u32 = 91;
    const THEIR_TRINKET: u32 = 92;
    const MY_BAUBLE: u32 = 93;
    const SPARE_RUNE: u32 = 100;

    fn steel(id: u32, zone: u16, seat: u8) -> CardInfo {
        fixtures::spell(id, zone, seat, "Brittle Steel", 2, 1)
    }

    fn forge(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(steel(STEEL, zone, 0));
        fixture
            .table
            .cards
            .push(steel(THEIR_STEEL, fixtures::HAND, 1));
        fixture.table.cards.push(fixtures::gear(
            THEIR_TRINKET,
            fixtures::BASE,
            1,
            "Trinket",
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_BAUBLE, fixtures::BASE, 0, "Bauble", 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(SPARE_RUNE, 0, "Fury", false));
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
    fn the_script_is_a_flow_sorcery_over_one_gear() {
        assert!(std::ptr::eq(script_of("Brittle Steel").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, GEAR);
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
    fn any_gear_on_the_board_is_offered_and_the_chosen_one_dies() {
        let mut fixture = forge(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, STEEL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_TRINKET}}}"),
                format!("{{card {MY_BAUBLE}}}"),
                "cancel".to_string()
            ],
            "gear on the board, mine and theirs; the Boots in hand are not"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_TRINKET}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_TRINKET)]);
        assert!(ctx.on_board(THEIR_TRINKET), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(THEIR_TRINKET).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.events.iter().any(
            |event| matches!(event, Event::Died { card, unit: false, .. } if *card == THEIR_TRINKET)
        ));
        assert!(ctx.on_board(MY_BAUBLE));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {THEIR_TRINKET}}} shatters")));
        assert_eq!(ctx.card(STEEL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn from_the_trash_it_is_a_flow_play_that_is_banished_after_the_kill() {
        let mut fixture = forge(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            STEEL,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_BAUBLE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(MY_BAUBLE).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.banished_of(0), [STEEL]);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {STEEL}}} is banished")));
    }

    #[test]
    fn a_unit_is_refused_a_gear_gone_before_resolution_is_left_alone_and_the_other_seat_waits() {
        let mut fixture = forge(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STEEL)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, STEEL).unwrap();
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, fixtures::HAND_GEAR] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is a unit or a gear in hand"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_TRINKET}}}")).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(THEIR_TRINKET, fixtures::HAND, 1), 1)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(THEIR_TRINKET).unwrap().zone, Some(fixtures::HAND));
        assert!(!ctx.blob.log.iter().any(|line| line.contains("shatters")));
        assert_eq!(ctx.card(STEEL).unwrap().zone, Some(fixtures::TRASH));
    }
}
