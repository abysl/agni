use super::prelude::{done, might_this_turn, play, spell};
use super::rumble_mechanized_menace::friendly_mechs;
use super::{Card, Cost, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
pub const REPEAT: Cost = Cost {
    energy: 1,
    power: &[Power::Rainbow],
};

fn rev(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let mechs = friendly_mechs(ctx, seat);
    if mechs.is_empty() {
        ctx.narrate(format!("{{seat {seat}}} has no Mechs"));
        return done();
    }
    for mech in mechs {
        might_this_turn(ctx, item, mech, MIGHT, None);
        ctx.narrate(format!("{{card {mech}}} gets +{MIGHT} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Danger Zone",
    &[Keyword::Reaction, Keyword::Repeat(REPEAT)],
    &[play(&[], rev)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::rumble_mechanized_menace::is_mech;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_REPEAT;
    use crate::engine::priority;
    use crate::state::{Expiry, PromptWhy};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const ZONE: u32 = 90;
    const THEIR_ZONE: u32 = 91;
    const BOT: u32 = 92;
    const TOKEN: u32 = 93;
    const THEIR_BOT: u32 = 94;
    const MY_EXTRA: [u32; 2] = [46, 47];
    const THEIR_RUNE: u32 = 48;

    fn zone(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Danger Zone", 1, 1);
        card.domain = vec!["Fury".into(), "Mind".into()];
        card
    }

    fn garage() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(zone(ZONE, 0));
        fixture.table.cards.push(zone(THEIR_ZONE, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BOT, fixtures::BF1, 0, "Bubble Bot", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(TOKEN, fixtures::BASE, 0, "Mech", 3));
        fixture.table.tokens.push(TOKEN);
        fixture.table.cards.push(fixtures::unit(
            THEIR_BOT,
            fixtures::BASE,
            1,
            "Forecaster",
            2,
        ));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(THEIR_RUNE, 1, "Mind", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_repeatable_reaction_without_targets_and_reads_mechs_by_printed_name() {
        assert!(std::ptr::eq(script_of("Danger Zone").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        let mut fixture = garage();
        let ctx = fixture.ctx();
        assert!(is_mech(&ctx, BOT));
        assert!(is_mech(&ctx, TOKEN));
        assert!(is_mech(&ctx, THEIR_BOT));
        assert!(!is_mech(&ctx, fixtures::VI));
        assert!(!is_mech(&ctx, 999));
        assert_eq!(friendly_mechs(&ctx, 0), [BOT, TOKEN]);
        assert_eq!(friendly_mechs(&ctx, 1), [THEIR_BOT]);
    }

    #[test]
    fn every_friendly_mech_gets_one_might_for_the_turn_and_nothing_else_does() {
        let mut fixture = garage();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ZONE).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none(), "no targets to choose");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            4,
            "one energy exhausted; the power recycles the spent rune"
        );
        assert_eq!(might_counter(&ctx, BOT), 0, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(BOT), 3, "2 + 1");
        assert_eq!(ctx.current_might(TOKEN), 4, "3 + 1");
        assert_eq!(might_counter(&ctx, fixtures::VI), 0, "Vi is no Mech");
        assert_eq!(might_counter(&ctx, THEIR_BOT), 0, "their Mech is not mine");
        assert_eq!(
            ctx.state_of(BOT).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} gets +1 might this turn".to_string()));
        assert_eq!(ctx.card(ZONE).unwrap().zone, Some(fixtures::TRASH));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(BOT), 2);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_for_one_energy_and_a_rainbow_it_gives_two_and_the_other_seat_reacts_on_my_turn() {
        let mut fixture = garage();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ZONE).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "two energy exhausted, the spent rune and one of them recycled for the power and the rainbow"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(BOT), 4, "2 + 1 + 1");
        assert_eq!(ctx.current_might(TOKEN), 5);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        let mut theirs = garage();
        let mut ctx = theirs.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        fixtures::play_from_hand(&mut ctx, 1, THEIR_ZONE).unwrap();
        fixtures::choose(&mut ctx, 1, "no").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.current_might(THEIR_BOT), 3);
        assert_eq!(might_counter(&ctx, BOT), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_mech_it_resolves_to_nothing_and_an_unaffordable_repeat_is_not_offered() {
        let mut fixture = garage();
        fixture
            .table
            .cards
            .retain(|card| ![BOT, TOKEN].contains(&card.id));
        fixture.table.tokens.retain(|card| *card != TOKEN);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ZONE).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&"{seat 0} has no Mechs".to_string()));
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert_eq!(ctx.card(ZONE).unwrap().zone, Some(fixtures::TRASH));
        let mut poor = garage();
        poor.table
            .cards
            .retain(|card| !MY_EXTRA.contains(&card.id) && ![41, 43].contains(&card.id));
        poor.resolve();
        let mut ctx = poor.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        fixtures::play_from_hand(&mut ctx, 0, ZONE).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one ready rune pays the energy and the spent rune the power, nothing is left for the repeat, and there is nothing to ask"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.blob.chain[0].repeated());
    }

    #[test]
    #[ignore = "engine gap · tags on the face · CardInfo carries no tags, so rumble_mechanized_menace::is_mech matches the printed names of the Mech-tagged units (MECHS) or a worn Hexplate; a tags row on the face closes it"]
    fn a_unit_printed_with_the_mech_tag_is_a_mech_whatever_its_name() {
        let mut fixture = garage();
        fixture.table.cards.push(fixtures::unit(
            95,
            fixtures::BASE,
            0,
            "Unnamed Prototype",
            1,
        ));
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_mech(&ctx, 95));
    }
}
