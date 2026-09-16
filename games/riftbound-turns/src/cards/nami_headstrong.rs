use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{
    an_enemy_unit, at_end_of_turn, buff, card_target, done, on_hold_me, on_you_play_card, play,
    ready, stun, triggered, unit, when, with_additional,
};
use super::{Card, Cost, Domain, Flow, Item, Power, Source, Stage, Trigger, KIND_UNIT};
use crate::engine::ctx::{Ctx, Event};
use crate::state::When;

pub const ADDITIONAL: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Calm)],
};
pub const TIDECALL: u8 = 1;
pub const SURGE: u8 = 2;
pub const LAPSE: u8 = 3;

fn ebb(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        stun(ctx, unit);
    }
    done()
}

pub fn promise_stands(ctx: &Ctx, me: u32) -> bool {
    let turn = ctx.turn();
    ctx.blob.delayed.iter().any(|delayed| {
        delayed.source == me && delayed.ability == LAPSE && delayed.when == When::EndOfTurn(turn)
    })
}

fn retire_promise(ctx: &mut Ctx, me: u32) {
    ctx.blob
        .delayed
        .retain(|delayed| !(delayed.source == me && delayed.ability == LAPSE));
}

fn tidecall(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if promise_stands(ctx, me) {
        ctx.narrate(format!(
            "{{card {me}}}: the promise already stands this turn"
        ));
        return done();
    }
    at_end_of_turn(ctx, item, LAPSE, Vec::new());
    ctx.narrate(format!(
        "{{card {me}}}: the next unit {{seat {}}} plays this turn readies and is buffed",
        item.controller
    ));
    done()
}

pub fn a_unit_you_play_while_the_promise_stands(ctx: &Ctx, event: &Event, source: Source) -> bool {
    let Event::Played {
        card,
        controller,
        kind,
        ..
    } = event
    else {
        return false;
    };
    *card != source.card
        && kind == KIND_UNIT
        && *controller == ctx.controller(source.card)
        && promise_stands(ctx, source.card)
}

fn surge(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = item.subject_card() else {
        return done();
    };
    retire_promise(ctx, me);
    if !ctx.on_board(unit) {
        ctx.narrate(format!("{{card {unit}}} has already left"));
        return done();
    }
    if ready(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} readies"));
    }
    if buff(ctx, unit) {
        ctx.narrate(format!("{{card {unit}}} is buffed"));
    }
    done()
}

fn lapse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    ctx.narrate(format!("{{card {me}}}: the promise lapses with the turn"));
    done()
}

pub static CARD: Card = with_additional(
    unit(
        "Nami - Headstrong",
        &[],
        &[
            when(
                play(&[an_enemy_unit("an enemy unit to stun")], ebb),
                paid_additional_on_entry,
            ),
            on_hold_me(&[], tidecall),
            when(
                on_you_play_card(&[], surge),
                a_unit_you_play_while_the_promise_stands,
            ),
            triggered(Trigger::Reflexive, &[], lapse),
        ],
    ),
    ADDITIONAL,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spawn, Location, Token};
    use crate::cards::{script_of, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, phases, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef, SLOT_ADDITIONAL};
    use agni_plugin_sdk::table::CardInfo;

    const NAMI: u32 = 90;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 3;
    const CALM_RUNE: u32 = 46;
    const SECOND_UNIT: u32 = 91;

    fn nami(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Calm".into()],
            ..fixtures::unit(NAMI, zone, seat, "Nami - Headstrong", MIGHT)
        }
    }

    fn tide(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(nami(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.table.card_mut(fixtures::HAND_UNIT).unwrap().energy = Some(1);
        fixture.table.cards.push(CardInfo {
            energy: Some(1),
            ..fixtures::unit(SECOND_UNIT, fixtures::HAND, 0, "Marai Warden", 2)
        });
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(NAMI).unwrap(), &CARD));
        fixture
    }

    fn recycled_calm(ctx: &Ctx) -> usize {
        [42, CALM_RUNE]
            .into_iter()
            .filter(|rune| ctx.card(*rune).unwrap().zone == Some(fixtures::RUNE_DECK))
            .count()
    }

    fn play_unit_to_base(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, unit).unwrap();
        if matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })) {
            fixtures::choose(ctx, 0, "your base").unwrap();
        }
    }

    fn holds(fixture: &mut Fixture) {
        fixture.table.card_mut(NAMI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
    }

    #[test]
    fn the_script_carries_a_calm_additional_cost_a_gated_stun_a_hold_and_the_promise_pair() {
        assert!(std::ptr::eq(script_of("Nami - Headstrong").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.additional, Some(ADDITIONAL));
        assert_eq!(CARD.abilities.len(), 4);
        let stun = &CARD.abilities[0];
        assert_eq!(stun.trigger, Trigger::Play);
        assert!(stun.condition.is_some());
        assert_eq!(stun.targets.len(), 1);
        assert_eq!(
            CARD.abilities[usize::from(TIDECALL)].trigger,
            Trigger::Hold(Who::Me)
        );
        let surge = &CARD.abilities[usize::from(SURGE)];
        assert_eq!(surge.trigger, Trigger::YouPlayCard);
        assert!(surge.condition.is_some());
        assert_eq!(
            CARD.abilities[usize::from(LAPSE)].trigger,
            Trigger::Reflexive
        );
    }

    #[test]
    fn paid_she_stuns_an_enemy_unit_on_entry_and_unpaid_she_stuns_nothing() {
        let mut fixture = tide(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, NAMI).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL
        ));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            recycled_calm(&ctx),
            1,
            "a Calm rune recycles for the additional power"
        );
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 81}"]);
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert!(
            !ctx.is_stunned(fixtures::THEIR_UNIT),
            "nothing until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(fixtures::THEIR_UNIT));
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut fixture = tide(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, NAMI).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.blob.prompt.is_none(), "no target prompt unpaid");
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.is_stunned(fixtures::THEIR_UNIT));
        assert!(!ctx.is_stunned(fixtures::SPRITE));
        assert_eq!(recycled_calm(&ctx), 0, "no Calm rune was recycled");
    }

    #[test]
    fn holding_registers_the_promise_and_the_next_unit_played_this_turn_readies_and_is_buffed() {
        let mut fixture = tide(fixtures::BF1);
        holds(&mut fixture);
        let mut ctx = fixture.ctx();
        assert!(!promise_stands(&ctx, NAMI));
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index }) if source == NAMI && index == TIDECALL
        ));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(promise_stands(&ctx, NAMI));
        assert_eq!(ctx.blob.delayed.len(), 1);
        assert_eq!(ctx.blob.delayed[0].when, When::EndOfTurn(ctx.turn()));
        play_unit_to_base(&mut ctx, fixtures::HAND_UNIT);
        assert!(
            ctx.card(fixtures::HAND_UNIT).unwrap().exhausted,
            "it entered exhausted"
        );
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index }) if source == NAMI && index == SURGE
        ));
        assert_eq!(
            ctx.blob.chain.last().unwrap().subject,
            Some(TargetRef::Card(fixtures::HAND_UNIT)),
            "the played unit is the trigger's subject"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(fixtures::HAND_UNIT).unwrap().exhausted, "readied");
        assert!(ctx.is_buffed(fixtures::HAND_UNIT), "buffed");
        assert!(!promise_stands(&ctx, NAMI), "the promise is spent");
        assert!(ctx.blob.delayed.is_empty());
        play_unit_to_base(&mut ctx, SECOND_UNIT);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.card(SECOND_UNIT).unwrap().exhausted,
            "the next unit only · the second stays exhausted"
        );
        assert!(!ctx.is_buffed(SECOND_UNIT));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_token_unit_played_while_the_promise_stands_spends_it() {
        let mut fixture = tide(fixtures::BF1);
        holds(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(promise_stands(&ctx, NAMI));
        let sprite = spawn(&mut ctx, 0, Token::Sprite, Location::Base(0), false).unwrap();
        settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index }) if source == NAMI && index == SURGE
        ));
        assert_eq!(
            ctx.blob.chain.last().unwrap().subject,
            Some(TargetRef::Card(sprite)),
            "350.2 · a Sprite played is a unit played"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.card(sprite).unwrap().exhausted, "readied");
        assert!(ctx.is_buffed(sprite), "buffed");
        assert!(!promise_stands(&ctx, NAMI));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_unspent_promise_lapses_at_the_end_of_the_turn_and_a_spell_or_gear_never_spends_it() {
        let mut fixture = tide(fixtures::BF1);
        holds(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(promise_stands(&ctx, NAMI));
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(promise_stands(&ctx, NAMI), "a spell is not a unit");
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_GEAR).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(promise_stands(&ctx, NAMI), "nor is a gear");
        phases::end_turn(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!promise_stands(&ctx, NAMI), "the turn ended");
        assert!(ctx.blob.delayed.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {NAMI}}}: the promise lapses with the turn"
        )));
    }

    #[test]
    fn an_opponents_unit_and_a_hold_elsewhere_are_not_hers() {
        let mut fixture = tide(fixtures::BF1);
        holds(&mut fixture);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(promise_stands(&ctx, NAMI));
        let source = Source {
            card: NAMI,
            ability: SURGE,
        };
        let theirs = Event::Played {
            card: fixtures::THEIR_UNIT,
            controller: 1,
            kind: KIND_UNIT.into(),
            origin: crate::state::Origin::Hand,
            paid_additional: false,
        };
        assert!(!a_unit_you_play_while_the_promise_stands(
            &ctx, &theirs, source
        ));
        let mine = Event::Played {
            card: fixtures::HAND_UNIT,
            controller: 0,
            kind: KIND_UNIT.into(),
            origin: crate::state::Origin::Hand,
            paid_additional: false,
        };
        assert!(a_unit_you_play_while_the_promise_stands(
            &ctx, &mine, source
        ));
        drop(ctx);
        let mut away = tide(fixtures::BASE);
        away.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        away.blob.set_holder(fixtures::BF1, Some(0));
        away.resolve();
        let mut ctx = away.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "Vi holds, Nami sits in base");
        assert!(!promise_stands(&ctx, NAMI));
    }
}
