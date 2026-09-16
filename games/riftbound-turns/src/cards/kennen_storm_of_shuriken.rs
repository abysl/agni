use super::prelude::{
    a_card, burn, card_target, done, on_conquer_me, play, unit, FRIENDLY_SPELL_IN_TRASH,
};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::cost;
use crate::engine::ctx::Ctx;
use crate::state::{CostedGrant, CostedKind, Expiry};

fn owned_printed_cost(card: &agni_plugin_sdk::table::CardInfo) -> (u8, Vec<super::Power>) {
    let domains: Vec<_> = card
        .domain
        .iter()
        .filter_map(|domain| super::Domain::parse(domain))
        .collect();
    let power = usize::from(card.power.unwrap_or(0));
    let powers = match domains.as_slice() {
        [] => vec![super::Power::Rainbow; power],
        [domain] => vec![super::Power::Domain(*domain); power],
        domains if domains.len() == power => {
            domains.iter().copied().map(super::Power::Domain).collect()
        }
        _ => vec![super::Power::Own; power],
    };
    (card.energy.unwrap_or(0), powers)
}

pub const BURN: usize = 2;

pub const SURGE: TargetSpec = a_card(
    FRIENDLY_SPELL_IN_TRASH,
    "a spell in your trash to give Flow equal to its cost this turn",
);

pub fn flow_for_its_cost(ctx: &Ctx, spell: u32) -> Option<cost::Cost> {
    if !ctx.is_spell(spell) {
        return None;
    }
    ctx.card(spell).map(cost::printed)
}

pub fn flow_granted(ctx: &Ctx, spell: u32) -> bool {
    ctx.granted_cost(spell, Keyword::Flow(super::Cost::FREE))
        .is_some()
}

pub fn grants_flow(ctx: &Ctx, seat: u8, spell: u32) -> Option<cost::Cost> {
    if ctx.controller(spell) != seat || !ctx.in_trash(spell) {
        return None;
    }
    ctx.granted_cost(spell, Keyword::Flow(super::Cost::FREE))
}

fn storm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let burned = burn(ctx, seat, BURN);
    ctx.narrate(format!(
        "{{card {}}} puts {burned} of {{seat {seat}}}'s top {BURN} cards into their trash",
        item.kind.source()
    ));
    done()
}

fn lend_flow(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(spell) = card_target(ctx, item, 0) else {
        return done();
    };
    let Some(flow) = flow_for_its_cost(ctx, spell) else {
        return done();
    };
    let Some(card) = ctx.card(spell) else {
        return done();
    };
    let (energy, power) = owned_printed_cost(card);
    let grant = CostedGrant {
        kind: CostedKind::Flow,
        energy,
        power,
        until: Expiry::EndOfTurn(ctx.turn()),
    };
    ctx.grant_costed_this_turn(spell, grant);
    ctx.narrate(format!(
        "{{card {spell}}} has [Flow] {} this turn",
        flow.label()
    ));
    done()
}

pub static CARD: Card = unit(
    "Kennen, Storm of Shuriken",
    &[],
    &[play(&[], storm), on_conquer_me(&[SURGE], lend_flow)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Domain, Power, Trigger, Who, IMPLICIT_FLOW};
    use crate::engine::cost::Need;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, chain, cleanup, legal, triggers};
    use crate::state::{GameBlob, ItemKind, PromptWhy, TargetRef};
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const KENNEN: u32 = 90;
    const BURIED_SPELL: u32 = 91;
    const BURIED_CHEAP: u32 = 92;
    const BURIED_UNIT: u32 = 93;
    const THEIR_BURIED_SPELL: u32 = 94;
    const MY_TOP: [u32; 2] = [23, 22];

    fn kennen(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(0),
            power: Some(0),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(KENNEN, zone, 0, "Kennen, Storm of Shuriken", 4)
        }
    }

    fn tempest(kennen_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(kennen(kennen_zone));
        fixture.table.cards.push(fixtures::spell(
            BURIED_SPELL,
            fixtures::TRASH,
            0,
            "Spark",
            2,
            1,
        ));
        fixture.table.cards.push(fixtures::spell(
            BURIED_CHEAP,
            fixtures::TRASH,
            0,
            "Flicker",
            0,
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(BURIED_UNIT, fixtures::TRASH, 0, "Jinx", 2));
        fixture.table.cards.push(fixtures::spell(
            THEIR_BURIED_SPELL,
            fixtures::TRASH,
            1,
            "Spark",
            2,
            1,
        ));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_burns_two_on_play_and_lends_flow_to_a_trash_spell_on_conquer() {
        assert!(std::ptr::eq(
            script_of("Kennen, Storm of Shuriken").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Kennen, Storm of Shuriken");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 2);
        let played = &CARD.abilities[0];
        assert_eq!(played.trigger, Trigger::Play);
        assert!(!played.optional);
        assert!(played.targets.is_empty());
        let conquer = &CARD.abilities[1];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(!conquer.optional);
        assert_eq!(conquer.targets, &[SURGE]);
        assert_eq!((SURGE.min, SURGE.max), (1, 1));
        assert_eq!(SURGE.filter, FRIENDLY_SPELL_IN_TRASH);
        assert_eq!(BURN, 2);
    }

    #[test]
    fn playing_him_puts_the_top_two_of_your_deck_into_your_trash() {
        let mut fixture = tempest(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let deck = ctx.zones.main_deck.unwrap();
        let before = ctx.table.held(deck, 0).count();
        fixtures::play_from_hand(&mut ctx, 0, KENNEN).unwrap();
        assert_eq!(ctx.location(KENNEN), Some(Location::Base(0)));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == KENNEN
        ));
        assert_eq!(ctx.table.held(deck, 0).count(), before, "the burn waits");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.table.held(deck, 0).count(), before - 2);
        for card in MY_TOP {
            assert!(ctx.in_trash(card), "{card} was on top");
            assert!(ctx.events.contains(&Event::Burned { seat: 0, card }));
        }
        assert!(ctx.blob.log.contains(&format!(
            "{{card {KENNEN}}} puts 2 of {{seat 0}}'s top 2 cards into their trash"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_flow_it_lends_is_the_spells_printed_cost() {
        let mut fixture = tempest(fixtures::BASE);
        let ctx = fixture.ctx();
        let flow = flow_for_its_cost(&ctx, BURIED_SPELL).unwrap();
        assert_eq!(flow.energy, 2);
        assert_eq!(flow.power, [Need::Domain(Domain::Fury)]);
        let cheap = flow_for_its_cost(&ctx, BURIED_CHEAP).unwrap();
        assert_eq!(cheap.energy, 0);
        assert_eq!(cheap.power, [Need::Domain(Domain::Fury)]);
        assert_eq!(
            flow_for_its_cost(&ctx, BURIED_UNIT),
            None,
            "a unit is not a spell"
        );
        assert_eq!(flow_for_its_cost(&ctx, 999), None);
        assert!(!flow_granted(&ctx, BURIED_SPELL));
        assert_eq!(grants_flow(&ctx, 0, BURIED_SPELL), None, "nothing lent yet");
        assert_eq!(grants_flow(&ctx, 1, THEIR_BURIED_SPELL), None);
    }

    #[test]
    fn owned_printed_cost_preserves_fixed_domains_and_compacts_choices() {
        let cases = [
            (vec![], 2, vec![Power::Rainbow, Power::Rainbow]),
            (vec!["Fury"], 2, vec![Power::Domain(Domain::Fury); 2]),
            (
                vec!["Fury", "Mind"],
                2,
                vec![Power::Domain(Domain::Fury), Power::Domain(Domain::Mind)],
            ),
            (vec!["Fury", "Mind"], 1, vec![Power::Own]),
        ];
        for (domains, power, expected) in cases {
            let mut card = fixtures::spell(90, fixtures::TRASH, 0, "Cost", 3, power);
            card.domain = domains.into_iter().map(String::from).collect();
            assert_eq!(owned_printed_cost(&card), (3, expected));
        }
    }

    #[test]
    fn conquering_asks_for_a_spell_in_your_trash_and_names_its_flow() {
        let mut fixture = tempest(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::establish(&mut ctx, fixtures::BF1);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Conquered { zone, seat: 0, .. } if *zone == fixtures::BF1
        )));
        assert_eq!(triggers::collect(&mut ctx), 1);
        chain::proceed(&mut ctx);
        let item = ctx
            .blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the conquer trigger is pending");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BURIED_SPELL}}}"),
                format!("{{card {BURIED_CHEAP}}}")
            ],
            "your spells in the trash · not the unit, not theirs"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BURIED_SPELL}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == KENNEN
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BURIED_SPELL)]);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.in_trash(BURIED_SPELL),
            "it stays in the trash until played"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {BURIED_SPELL}}} has [Flow] 2 energy and 1 Fury power this turn"
        )));
        assert!(flow_granted(&ctx, BURIED_SPELL));
        assert!(
            !flow_granted(&ctx, BURIED_CHEAP),
            "only the chosen spell is lent"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_spell_in_your_trash_the_conquer_trigger_has_nothing_to_offer() {
        let mut fixture = tempest(fixtures::BF1);
        fixture
            .table
            .cards
            .retain(|card| ![BURIED_SPELL, BURIED_CHEAP].contains(&card.id));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cleanup::establish(&mut ctx, fixtures::BF1);
        triggers::collect(&mut ctx);
        chain::proceed(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "no candidate, no prompt");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.in_trash(THEIR_BURIED_SPELL),
            "the enemy's spell is not yours"
        );
        assert!(!ctx.blob.log.iter().any(|line| line.contains("has [Flow]")));
    }

    #[test]
    fn the_lent_spell_can_be_played_from_the_trash_for_its_cost_this_turn() {
        let mut fixture = tempest(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::establish(&mut ctx, fixtures::BF1);
        triggers::collect(&mut ctx);
        chain::proceed(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BURIED_SPELL}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let flow = flow_for_its_cost(&ctx, BURIED_SPELL).unwrap();
        assert_eq!(grants_flow(&ctx, 0, BURIED_SPELL), Some(flow));
        assert!(activate::flow_offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == BURIED_SPELL));
        let entry = crate::engine::ctx::EntryMove {
            card: BURIED_SPELL,
            from: ctx.zones.trash,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert!(legal::classify(&ctx, 0, &entry).is_ok());
        let until = crate::cards::prelude::this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(grants_flow(&ctx, 0, BURIED_SPELL), None, "this turn only");
    }

    #[test]
    fn a_saved_lent_flow_grant_pays_and_banishes_the_spell_after_reload() {
        let mut fixture = tempest(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::establish(&mut ctx, fixtures::BF1);
        triggers::collect(&mut ctx);
        chain::proceed(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {BURIED_SPELL}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        let saved = ctx.blob.encode();
        let saved_table = ctx.table.clone();
        drop(ctx);
        fixture.table = saved_table;
        fixture.blob = GameBlob::decode(&saved).expect("the grant survives a blob round trip");

        let mut restored = fixture.ctx();
        let ready_before = restored.ready_runes_of(0).len();
        activate::activate(&mut restored, 0, BURIED_SPELL, IMPLICIT_FLOW).unwrap();
        assert_eq!(
            restored.ready_runes_of(0).len(),
            ready_before - 2,
            "the reloaded owned Flow cost pays two energy"
        );
        assert_eq!(
            restored.card(fixtures::RUNE_A).unwrap().zone,
            restored.zones.rune_deck,
            "the Fury power is recycled from the spent rune pool"
        );
        assert_eq!(
            restored.card(BURIED_SPELL).unwrap().zone,
            restored.zones.chain
        );
        fixtures::pass_until_open(&mut restored);
        assert!(restored.blob.chain.is_empty());
        assert_eq!(restored.banished_of(0), [BURIED_SPELL]);
        assert!(!restored.trash_of(0).contains(&BURIED_SPELL));
        assert_eq!(grants_flow(&restored, 0, BURIED_SPELL), None);
    }
}
