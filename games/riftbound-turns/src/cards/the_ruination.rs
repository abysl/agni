use super::prelude::{done, play, spell, units_on_board};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::{Cause, Ctx};
use crate::engine::kill;

pub fn every_unit(ctx: &Ctx) -> Vec<u32> {
    let mut units = units_on_board(ctx);
    units.sort_unstable();
    units
}

fn ruin(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let units = every_unit(ctx);
    let me = item.kind.source();
    if units.is_empty() {
        ctx.narrate(format!("{{card {me}}} finds no unit to kill"));
        return done();
    }
    ctx.narrate(format!("{{card {me}}} kills every unit"));
    let dead = kill::batch(ctx, &units, Cause::Item(item.id));
    for unit in dead {
        ctx.narrate(format!("{{card {unit}}} dies"));
    }
    done()
}

pub static CARD: Card = spell("The Ruination", &[], &[play(&[], ruin)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{replaces, unit, with_replacement, Location};
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, settle};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const RUINATION: u32 = 90;
    const THEIR_RUINATION: u32 = 91;
    const BRUTE: u32 = 92;
    const MY_GEAR: u32 = 93;
    const MY_EXTRA: [u32; 5] = [100, 101, 102, 103, 104];

    static PHOENIX: Card = with_replacement(
        unit("Phoenix", &[], &[]),
        replaces(
            |_, would, source| would.unit == source.card,
            |ctx, would, _| {
                ctx.recall(would.unit, true);
            },
        ),
    );

    fn ruination(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "The Ruination", 9, 3);
        card.domain = vec!["Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ruination(RUINATION, 0));
        fixture.table.cards.push(ruination(THEIR_RUINATION, 1));
        for (offset, rune) in MY_EXTRA.into_iter().enumerate() {
            let domain = if offset < 3 { "Order" } else { "Fury" };
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, domain, false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 7));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
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

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, RUINATION).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.d · nothing is targeted");
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    #[test]
    fn the_script_is_a_targetless_sorcery_that_lists_every_unit_in_id_order() {
        assert!(std::ptr::eq(script_of("The Ruination").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            every_unit(&ctx),
            [fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT, BRUTE],
            "units only, in bases and at battlefields, mine and theirs"
        );
    }

    #[test]
    fn every_unit_dies_and_gear_battlefields_and_the_legend_stand() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        for unit in [fixtures::VI, fixtures::THEIR_UNIT, BRUTE] {
            assert_eq!(
                ctx.card(unit).unwrap().zone,
                Some(fixtures::TRASH),
                "{unit} dies"
            );
            assert!(ctx.events.iter().any(
                |event| matches!(event, Event::Died { card, unit: true, .. } if *card == unit)
            ));
            assert!(ctx.blob.log.contains(&format!("{{card {unit}}} dies")));
        }
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "the killed token ceases to exist"
        );
        assert!(ctx.on_board(MY_GEAR), "gear is not a unit");
        assert_eq!(
            ctx.card(fixtures::GROUNDS).unwrap().zone,
            Some(fixtures::BF1)
        );
        assert_eq!(
            ctx.card(fixtures::LEGEND_CARD).unwrap().zone,
            Some(fixtures::LEGEND)
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} kills every unit".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(RUINATION).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "nine of the nine ready runes paid the energy"
        );
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_replaced_death_is_not_a_kill_and_an_empty_board_narrates() {
        let mut fixture = armed();
        fixture.scripts = fixture.scripts.clone().with_script(BRUTE, &PHOENIX);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Base(1)),
            "the replacement recalled it instead"
        );
        assert!(!ctx.blob.log.contains(&format!("{{card {BRUTE}}} dies")));
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::TRASH),
            "the others still die"
        );
        drop(ctx);
        let mut bare = armed();
        bare.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT, BRUTE].contains(&card.id)
        });
        bare.resolve();
        let mut ctx = bare.ctx();
        cast(&mut ctx);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} finds no unit to kill".to_string()));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn it_is_the_turn_players_sorcery_and_needs_nine_energy_and_three_order() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_RUINATION)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut poor = armed();
        poor.table.card_mut(MY_EXTRA[0]).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, RUINATION)),
            Err(Refusal::NoPowerOf),
            "the energy is there but only two Order runes are"
        );
        drop(ctx);
        let mut short = armed();
        short.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = true;
        short.resolve();
        let ctx = short.ctx();
        assert!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, RUINATION)).is_err(),
            "eight ready runes cannot pay nine energy"
        );
    }
}
