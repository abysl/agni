use super::prelude::{a_play_location, done, play, spawn, spell, zone_target, Location, Token};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

pub const SOLDIER_ARRIVES_READY: bool = false;
pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};

fn call(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let at = zone_target(item, 0)
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .unwrap_or(Location::Base(seat));
    if let Some(soldier) = spawn(ctx, seat, Token::SandSoldier, at, SOLDIER_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {soldier}}} to {}",
            describe(at)
        ));
    }
    done()
}

pub static CARD: Card = spell(
    "Desert's Call",
    &[Keyword::Repeat(REPEAT)],
    &[play(
        &[a_play_location("where the Sand Soldier is played")],
        call,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger, TOKEN_SAND_SOLDIER};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const CALL: u32 = 90;
    const THEIR_CALL: u32 = 91;
    const MY_EXTRA: [u32; 2] = [46, 47];

    fn call_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Desert's Call", 2, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(call_card(CALL, 0));
        fixture.table.cards.push(call_card(THEIR_CALL, 1));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
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

    fn soldiers_of<'c>(ctx: &'c Ctx, seat: u8) -> Vec<&'c CardInfo> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SAND_SOLDIER && card.owner == seat)
            .collect()
    }

    #[test]
    fn the_script_is_a_repeatable_plain_spell_that_picks_a_play_location() {
        assert!(std::ptr::eq(script_of("Desert's Call").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].kind, TargetKind::Zone);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn an_exhausted_two_might_sand_soldier_is_played_to_the_chosen_location() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(soldiers_of(&ctx, 0).is_empty());
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "{zone 9}", "cancel"],
            "the base and the held battlefield, never the other seat's"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Zone(fixtures::BF1)]);
        assert!(soldiers_of(&ctx, 0).is_empty(), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let soldiers = soldiers_of(&ctx, 0);
        assert_eq!(soldiers.len(), 1);
        let soldier = soldiers[0];
        assert_eq!(soldier.zone, Some(fixtures::BF1));
        assert!(
            soldier.exhausted,
            "369.3 · a unit without Accelerate enters exhausted"
        );
        assert_eq!(soldier.might, Some(2));
        assert_eq!(ctx.controller(soldier.id), 0);
        assert!(ctx.is_token(soldier.id));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, .. } if *card == soldier.id
        )));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {}}} to {{zone 9}}",
            soldier.id
        )));
        assert_eq!(ctx.card(CALL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_it_plays_two_soldiers_each_to_its_own_location() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(
            item.targets,
            [
                TargetRef::Zone(fixtures::BF1),
                TargetRef::Zone(fixtures::BASE)
            ]
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "two energy twice");
        fixtures::pass_until_open(&mut ctx);
        let soldiers = soldiers_of(&ctx, 0);
        assert_eq!(soldiers.len(), 2);
        let mut zones: Vec<u16> = soldiers.iter().filter_map(|card| card.zone).collect();
        zones.sort_unstable();
        assert_eq!(zones, [fixtures::BASE, fixtures::BF1]);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seats_battlefield_is_refused_and_the_spell_has_no_window_off_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CALL)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[u32::from(fixtures::BF2)]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the other seat holds it"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CALL).unwrap().zone, Some(fixtures::HAND));
        assert!(soldiers_of(&ctx, 0).is_empty());
    }
}
