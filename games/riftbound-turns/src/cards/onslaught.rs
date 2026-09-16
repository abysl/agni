use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 6;
pub const FLOW: Cost = Cost {
    energy: 4,
    power: &[],
};

fn charge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        ctx.narrate(format!("{{card {unit}}} gets +{MIGHT} this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Onslaught",
    &[Keyword::Flow(FLOW)],
    &[play(&[a_unit("a unit to give +6")], charge)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{CounterDest, EntryMove};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::engine::{chain, play as play_engine, priority, settle};
    use crate::state::{Leave, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;

    const ONSLAUGHT: u32 = 90;

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut onslaught = fixtures::spell(ONSLAUGHT, zone, 0, "Onslaught", 4, 0);
        onslaught.domain = vec!["Body".into()];
        fixture.table.cards.push(onslaught);
        for id in [46, 47] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Fury", false));
        }
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, seat: u8, from: u16) -> EntryMove {
        EntryMove {
            card: ONSLAUGHT,
            from: Some(from),
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_prints_flow_and_gives_six_might_from_hand_then_lands_in_the_trash() {
        assert!(std::ptr::eq(script_of("Onslaught").unwrap(), &CARD));
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        assert!(CARD.has_keyword(Keyword::Flow(Cost::FREE)));
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ONSLAUGHT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 3 + i32::from(MIGHT));
        assert_eq!(ctx.card(ONSLAUGHT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.banished_of(0).is_empty());
    }

    #[test]
    fn from_the_trash_the_drag_is_a_flow_play_that_is_banished_when_it_leaves_the_chain() {
        let mut fixture = armed(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        let entry = drag(&ctx, 0, fixtures::TRASH);
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Ok(Intent::Play {
                card: ONSLAUGHT,
                origin: Origin::Trash {
                    leave: Leave::Banish
                },
                location: None,
                on_chain: true,
            })
        );
        let rows = legal::highlights(&ctx, 0);
        assert!(
            rows.iter()
                .any(|row| row.card == ONSLAUGHT && row.zones == [fixtures::CHAIN]),
            "the trash card lights the chain for its owner: {rows:?}"
        );
        assert!(legal::highlights(&ctx, 1)
            .iter()
            .all(|row| row.card != ONSLAUGHT));
        play_engine::begin(
            &mut ctx,
            0,
            ONSLAUGHT,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.card(ONSLAUGHT).unwrap().zone,
            Some(fixtures::CHAIN),
            "begin moves the card onto the chain itself"
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert!(prompt.cancel, "a Flow play can be taken back");
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.trash_of(0), Vec::<u32>::new());
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 3 + i32::from(MIGHT));
        assert_eq!(ctx.banished_of(0), [ONSLAUGHT]);
        assert!(ctx.trash_of(0).is_empty(), "the trash count is unchanged");
        assert!(ctx.blob.log.contains(&"{card 90} is banished".to_string()));
    }

    #[test]
    fn a_cancelled_flow_play_returns_to_the_trash_and_a_countered_one_is_banished() {
        let mut fixture = armed(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            ONSLAUGHT,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(ONSLAUGHT).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.queue.is_empty());
        let mut countered = armed(fixtures::TRASH);
        let mut ctx = countered.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            ONSLAUGHT,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(chain::counter(&mut ctx, 1, CounterDest::Trash));
        assert_eq!(ctx.banished_of(0), [ONSLAUGHT]);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }

    #[test]
    fn a_spell_without_flow_stays_in_the_trash_and_the_opponents_turn_refuses_the_flow_play() {
        let mut fixture = armed(fixtures::TRASH);
        fixture.table.card_mut(ONSLAUGHT).unwrap().name = "Spark".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        let entry = drag(&ctx, 0, fixtures::TRASH);
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::TrashIsFinal))
        );
        let mut theirs = armed(fixtures::TRASH);
        theirs.blob = crate::state::GameBlob::start(2, 1, crate::state::Mode::Enforced);
        theirs.blob.set_phase(crate::state::Phase::Action);
        theirs.blob.seats = vec![Default::default(); 2];
        let ctx = theirs.ctx();
        let entry = drag(&ctx, 0, fixtures::TRASH);
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotYourTurn),
            "Onslaught keeps its own Sorcery timing from the trash"
        );
        let mut mine = armed(fixtures::TRASH);
        let mut ctx = mine.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        let entry = drag(&ctx, 0, fixtures::TRASH);
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::ChainClosed))
        );
    }
}
