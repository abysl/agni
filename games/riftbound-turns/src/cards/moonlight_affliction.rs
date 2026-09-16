use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const SHRINK: i16 = -10;

fn afflict(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, SHRINK, None);
        ctx.narrate(format!("{{card {unit}}} gets {SHRINK} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Moonlight Affliction",
    &[Keyword::Reaction],
    &[play(&[a_unit("a unit")], afflict)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play as play_engine;
    use crate::engine::priority;
    use crate::state::{Expiry, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const AFFLICTION: u32 = 90;
    const THEIR_AFFLICTION: u32 = 91;
    const COLOSSUS: u32 = 92;
    const MY_EXTRA: [u32; 4] = [46, 47, 48, 49];
    const THEIR_EXTRA: [u32; 5] = [100, 101, 102, 103, 104];

    fn affliction(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Moonlight Affliction", 7, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(affliction(AFFLICTION, 0));
        fixture.table.cards.push(affliction(THEIR_AFFLICTION, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(COLOSSUS, fixtures::BF1, 1, "Colossus", 12));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        for rune in THEIR_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Mind", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_reaction_over_one_unit_that_takes_ten_off() {
        assert!(std::ptr::eq(
            script_of("Moonlight Affliction").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Moonlight Affliction");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), 1);
        assert_eq!(CARD.abilities[0].targets[0].filter, UNIT);
        assert_eq!(SHRINK, -10);
    }

    #[test]
    fn a_colossus_shrinks_by_ten_and_a_small_unit_reads_zero_until_the_turn_ends() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, AFFLICTION).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "{card 92}", "cancel"]
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(COLOSSUS)]);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "seven energy paid");
        assert_eq!(ctx.current_might(COLOSSUS), 12, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(COLOSSUS), 2, "12 - 10");
        assert_eq!(might_counter(&ctx, COLOSSUS), -10);
        assert_eq!(
            ctx.state_of(COLOSSUS).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} gets -10 might this turn".to_string()));
        assert_eq!(ctx.card(AFFLICTION).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(COLOSSUS), 12);
        assert_eq!(might_counter(&ctx, COLOSSUS), 0);
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, AFFLICTION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            0,
            "477.3.c · a 2-Might unit reads 0, never negative"
        );
        assert_eq!(
            ctx.state_of(fixtures::THEIR_UNIT).unwrap().might[0].delta,
            -10,
            "the whole -10 is layered so a later +N composes on it"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_reacts_to_my_spell_and_its_affliction_resolves_first() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        fixtures::play_from_hand(&mut ctx, 1, THEIR_AFFLICTION).unwrap();
        fixtures::choose(&mut ctx, 1, "{card 50}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.current_might(fixtures::VI), 0);
        assert_eq!(might_counter(&ctx, fixtures::VI), -10);
    }

    #[test]
    fn non_units_are_refused_and_a_short_purse_never_opens_the_play() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, AFFLICTION).unwrap();
        for wrong in [fixtures::HAND_GEAR, fixtures::HAND_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(AFFLICTION).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut poor = armed();
        poor.table.cards.retain(|card| !MY_EXTRA.contains(&card.id));
        poor.resolve();
        let ctx = poor.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: AFFLICTION,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotEnoughRunes {
                needed: 7,
                ready: 4
            })
        );
    }
}
