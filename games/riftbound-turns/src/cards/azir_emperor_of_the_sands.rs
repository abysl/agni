use super::prelude::{
    activated, done, exhausting_self, legend, named, spawn, usable_if, with_statics, Location,
    Token, ONE_ENERGY,
};
use super::{
    base_name, Card, Flow, Grant, Item, Keyword, Scope, Source, Stage, Static, Timing,
    TOKEN_SAND_SOLDIER,
};
use crate::engine::ctx::Ctx;

const SOLDIER_ARRIVES_READY: bool = false;

pub fn is_sand_soldier(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card)
        .is_some_and(|held| base_name(&held.name) == TOKEN_SAND_SOLDIER)
}

fn a_sand_soldier(ctx: &Ctx, _: u32, unit: u32) -> bool {
    is_sand_soldier(ctx, unit)
}

pub fn played_an_equipment_this_turn(ctx: &Ctx, seat: u8) -> bool {
    ctx.blob.seat(seat).equipment_played
}

fn armed_this_turn(ctx: &Ctx, source: Source) -> bool {
    played_an_equipment_this_turn(ctx, ctx.controller(source.card))
}

fn arise(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    if let Some(soldier) = spawn(
        ctx,
        seat,
        Token::SandSoldier,
        Location::Base(seat),
        SOLDIER_ARRIVES_READY,
    ) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {soldier}}} to their base"
        ));
    }
    done()
}

pub static CARD: Card = with_statics(
    legend(
        "Azir - Emperor of the Sands",
        &[],
        &[named(
            usable_if(
                exhausting_self(activated(Timing::Sorcery, ONE_ENERGY, &[], arise)),
                armed_this_turn,
            ),
            "play a Sand Soldier",
        )],
    ),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: a_sand_soldier,
        grants: &[Grant::Keyword(Keyword::Weaponmaster)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::WEAPONMASTER_TARGET;
    use crate::cards::{script_of, SelfCost, Trigger, KIND_GEAR, KIND_UNIT};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, statics};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const AZIR: u32 = fixtures::LEGEND_CARD;
    const SOLDIER: u32 = 90;
    const THEIR_SOLDIER: u32 = 91;
    const CHAOS_RUNE: u32 = 46;

    fn soldier(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            might: Some(2),
            ..fixtures::card(id, fixtures::BASE, seat, TOKEN_SAND_SOLDIER, KIND_UNIT)
        }
    }

    fn shurima() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(AZIR).unwrap().name = CARD.name.into();
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(AZIR).unwrap(), &CARD));
        fixture
    }

    fn with_soldiers() -> Fixture {
        let mut fixture = shurima();
        fixture.table.cards.push(soldier(SOLDIER, 0));
        fixture.table.cards.push(soldier(THEIR_SOLDIER, 1));
        fixture.table.tokens.extend([SOLDIER, THEIR_SOLDIER]);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        fixture
    }

    fn soldiers_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SAND_SOLDIER && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn play_the_boots(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, fixtures::HAND_GEAR).unwrap();
        fixtures::pass_until_open(ctx);
        assert!(ctx.on_board(fixtures::HAND_GEAR));
    }

    fn request(fixture: &mut Fixture, run: impl FnOnce(&mut Ctx)) {
        let mut ctx = fixture.ctx();
        run(&mut ctx);
        assert!(ctx.fault.is_none());
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        fixture.blob = crate::state::GameBlob::decode(&fixture.blob.encode()).unwrap();
    }

    #[test]
    fn reported_equipment_unlocks_azir_and_weaponmaster_across_requests() {
        for (name, energy, domain) in [
            ("B.F. Sword", 4, "Order"),
            ("Soul Sword", 1, "Calm"),
            ("Hand Hammer", 2, "Calm"),
        ] {
            let mut fixture = shurima();
            let gear = fixture.table.card_mut(fixtures::HAND_GEAR).unwrap();
            gear.name = name.into();
            gear.energy = Some(energy);
            gear.domain = vec![domain.into()];
            for id in 90..98 {
                fixture
                    .table
                    .cards
                    .push(fixtures::rune(id, 0, domain, false));
            }
            fixture.table.next_id = 100;
            fixture.resolve();
            request(&mut fixture, |ctx| {
                fixtures::play_from_hand(ctx, 0, fixtures::HAND_GEAR).unwrap();
                fixtures::pass_until_open(ctx);
            });
            request(&mut fixture, |ctx| {
                assert!(ctx.events.is_empty());
                assert!(
                    activate::offers(ctx, 0)
                        .iter()
                        .any(|offer| offer.source == AZIR && offer.enabled),
                    "{name}"
                );
                activate::activate(ctx, 0, AZIR, 0).unwrap();
            });
            request(&mut fixture, fixtures::pass_until_open);
            request(&mut fixture, |ctx| {
                assert_eq!(soldiers_of(ctx, 0).len(), 1);
                fixtures::choose(ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
            });
            request(&mut fixture, |ctx| {
                let runes = ctx.runes_of(0).len();
                fixtures::pass_until_open(ctx);
                let soldier = soldiers_of(ctx, 0)[0];
                assert_eq!(
                    crate::cards::prelude::attached_to(ctx, fixtures::HAND_GEAR),
                    Some(soldier),
                    "{name}"
                );
                assert_eq!(
                    ctx.runes_of(0).len(),
                    runes - 1,
                    "a domain-specific Equip cost is not discounted"
                );
            });
        }
    }

    #[test]
    fn equipment_history_survives_its_removal_but_expires_with_the_turn() {
        let mut fixture = shurima();
        request(&mut fixture, play_the_boots);
        request(&mut fixture, |ctx| {
            ctx.kill(fixtures::HAND_GEAR, Cause::Rule);
        });
        request(&mut fixture, |ctx| {
            assert!(played_an_equipment_this_turn(ctx, 0));
            assert!(!played_an_equipment_this_turn(ctx, 1));
            crate::engine::expiry::at_expiration(ctx);
        });
        let ctx = fixture.ctx();
        assert!(!played_an_equipment_this_turn(&ctx, 0));
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == AZIR));
    }

    #[test]
    fn non_equipment_gear_does_not_unlock_azir() {
        let mut fixture = shurima();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().name = "Ravenborn Tome".into();
        fixture.resolve();
        request(&mut fixture, play_the_boots);
        let ctx = fixture.ctx();
        assert_eq!(ctx.blob.seat(0).gear_played, 1);
        assert!(!played_an_equipment_this_turn(&ctx, 0));
    }

    #[test]
    fn the_legend_grants_weaponmaster_to_sand_soldiers_and_has_one_gated_exhaust_activation() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Keyword(Keyword::Weaponmaster)],
                ..
            }]
        ));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.label, Some("play a Sand Soldier"));
        assert!(ability.usable.is_some());
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn friendly_sand_soldiers_read_weaponmaster_and_no_other_unit_does() {
        let mut fixture = with_soldiers();
        let mut ctx = fixture.ctx();
        assert!(is_sand_soldier(&ctx, SOLDIER));
        assert!(!is_sand_soldier(&ctx, fixtures::VI));
        assert!(!is_sand_soldier(&ctx, AZIR));
        assert!(ctx.has_keyword(SOLDIER, Keyword::Weaponmaster));
        assert!(matches!(
            statics::grants_on(&ctx, SOLDIER).as_slice(),
            [Grant::Keyword(Keyword::Weaponmaster)]
        ));
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Weaponmaster));
        assert!(
            !ctx.has_keyword(THEIR_SOLDIER, Keyword::Weaponmaster),
            "the opponent's Soldier is not his"
        );
        assert!(ctx.set_controller(THEIR_SOLDIER, 0, SOLDIER));
        assert!(
            ctx.has_keyword(THEIR_SOLDIER, Keyword::Weaponmaster),
            "friendly follows the controller"
        );
    }

    #[test]
    fn the_gate_reads_an_equipment_played_this_turn_by_his_controller() {
        let mut fixture = shurima();
        let mut ctx = fixture.ctx();
        assert!(!played_an_equipment_this_turn(&ctx, 0));
        assert_eq!(
            activate::activate(&mut ctx, 0, AZIR, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "use only if you've played an Equipment this turn"
        );
        assert!(
            activate::offers(&ctx, 0).is_empty(),
            "not offered at all until an Equipment is played"
        );
        ctx.raise(Event::Played {
            card: fixtures::HAND_UNIT,
            controller: 0,
            kind: KIND_UNIT.into(),
            origin: crate::state::Origin::Hand,
            paid_additional: false,
        });
        assert!(
            !played_an_equipment_this_turn(&ctx, 0),
            "a unit is not an Equipment"
        );
        ctx.raise(Event::Played {
            card: fixtures::HAND_GEAR,
            controller: 1,
            kind: KIND_GEAR.into(),
            origin: crate::state::Origin::Hand,
            paid_additional: false,
        });
        assert!(
            !played_an_equipment_this_turn(&ctx, 0),
            "the opponent's Equipment is not yours"
        );
        play_the_boots(&mut ctx);
        assert!(played_an_equipment_this_turn(&ctx, 0));
        let his: Vec<(String, bool)> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == AZIR)
            .map(|offer| (offer.label, offer.enabled))
            .collect();
        assert_eq!(
            his,
            [(
                format!("{{card {AZIR}}}: play a Sand Soldier (1 energy, exhaust)"),
                true
            )]
        );
    }

    #[test]
    fn with_an_equipment_played_one_energy_and_his_exhaust_play_an_exhausted_soldier_to_the_base() {
        let mut fixture = shurima();
        let mut ctx = fixture.ctx();
        play_the_boots(&mut ctx);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "the Boots cost two");
        activate::activate(&mut ctx, 0, AZIR, 0).unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target, no location to pick");
        assert!(ctx.card(AZIR).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == AZIR
        ));
        assert!(
            soldiers_of(&ctx, 0).is_empty(),
            "the Soldier waits for the chain"
        );
        let next = ctx.table.next_id;
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let soldier = *soldiers_of(&ctx, 0).first().expect("one Sand Soldier");
        assert_eq!(soldier, next);
        assert!(ctx.is_unit(soldier));
        assert!(ctx.is_token(soldier));
        assert_eq!(ctx.card(soldier).unwrap().might, Some(2));
        assert_eq!(ctx.location(soldier), Some(Location::Base(0)));
        assert!(
            ctx.card(soldier).unwrap().exhausted,
            "369.3 · a played unit enters exhausted"
        );
        assert!(
            ctx.has_keyword(soldier, Keyword::Weaponmaster),
            "the new Soldier is under his aura"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {soldier}}} to their base"
        )));
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { spec: 0, .. })),
            "821 · the granted Weaponmaster offers the Boots"
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, AZIR, 0),
            Err(Refusal::Exhausted),
            "once a turn by his exhaust"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_wrong_seat_and_a_seat_short_of_energy_are_refused() {
        let mut fixture = shurima();
        for rune in [42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, AZIR, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(activate::offers(&ctx, 1).is_empty());
        ctx.raise(Event::Played {
            card: fixtures::HAND_GEAR,
            controller: 0,
            kind: KIND_GEAR.into(),
            origin: crate::state::Origin::Hand,
            paid_additional: false,
        });
        assert!(played_an_equipment_this_turn(&ctx, 0));
        ctx.exhaust(41);
        assert_eq!(
            activate::activate(&mut ctx, 0, AZIR, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            })
        );
        assert!(soldiers_of(&ctx, 0).is_empty());
    }

    #[test]
    fn an_equipment_played_in_an_earlier_request_still_opens_the_gate() {
        let mut fixture = shurima();
        {
            let mut ctx = fixture.ctx();
            play_the_boots(&mut ctx);
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        fixture.blob = crate::state::GameBlob::decode(&fixture.blob.encode()).unwrap();
        let mut ctx = fixture.ctx();
        assert!(ctx.events.is_empty(), "a fresh request");
        assert!(played_an_equipment_this_turn(&ctx, 0));
        activate::activate(&mut ctx, 0, AZIR, 0).unwrap();
        resolve_top(&mut ctx);
        assert_eq!(soldiers_of(&ctx, 0).len(), 1);
    }

    #[test]
    fn a_sand_soldier_played_under_his_aura_may_attach_a_loose_equipment_as_it_enters() {
        let mut fixture = shurima();
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", true));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play_the_boots(&mut ctx);
        activate::activate(&mut ctx, 0, AZIR, 0).unwrap();
        resolve_top(&mut ctx);
        let soldier = *soldiers_of(&ctx, 0).first().expect("one Sand Soldier");
        assert!(ctx.has_keyword(soldier, Keyword::Weaponmaster));
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { spec: 0, .. })),
            "the Weaponmaster attach asks for {}",
            WEAPONMASTER_TARGET.label
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_GEAR)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            crate::cards::prelude::attached_to(&ctx, fixtures::HAND_GEAR),
            Some(soldier)
        );
    }
}
