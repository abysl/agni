use super::prelude::{
    a_play_location, done, friendly_units, on_conquer_me, on_hold_me, play, spawn, unit,
    with_statics, zone_target, Location, Token,
};
use super::{Ability, Card, Flow, Grant, Item, Stage, Static, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

pub const LADDER: i32 = 16;
const TENTACLE_ARRIVES_READY: bool = false;
pub const WHERE_THE_TENTACLE_GOES: TargetSpec = a_play_location("where the Tentacle is played");

pub fn token_units_you_control(ctx: &Ctx, seat: u8) -> usize {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| ctx.is_token(*unit))
        .count()
}

pub fn might_bonus(ctx: &Ctx, card: u32) -> i16 {
    i16::try_from(token_units_you_control(ctx, ctx.controller(card))).unwrap_or(i16::MAX)
}

fn at_least<const N: i32>(ctx: &Ctx, card: u32, _: u32) -> bool {
    i32::from(might_bonus(ctx, card)) >= N
}

fn always(_: &Ctx, _: u32) -> bool {
    true
}

macro_rules! ladder {
    ($($step:literal)+) => {
        &[$(Grant::MightIf(at_least::<$step>, 1)),+]
    };
}

pub static TOKENS: &[Grant] = ladder!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

fn summon(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let at = zone_target(item, 0)
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .unwrap_or(Location::Base(seat));
    if let Some(tentacle) = spawn(ctx, seat, Token::Tentacle, at, TENTACLE_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {tentacle}}} to {}",
            describe(at)
        ));
    }
    done()
}

pub const ON_PLAY: Ability = play(&[WHERE_THE_TENTACLE_GOES], summon);
pub const ON_CONQUER: Ability = on_conquer_me(&[WHERE_THE_TENTACLE_GOES], summon);
pub const ON_HOLD: Ability = on_hold_me(&[WHERE_THE_TENTACLE_GOES], summon);

pub static CARD: Card = with_statics(
    unit(
        "Illaoi, Prophet of the Great Kraken",
        &[],
        &[ON_PLAY, ON_CONQUER, ON_HOLD],
    ),
    &[Static::While(always, TOKENS)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger, Who, TOKEN_TENTACLE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, settle, statics};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const ILLAOI: u32 = 90;
    const THEIR_ILLAOI: u32 = 91;
    const GOLD: u32 = 92;
    const EXTRA_RUNES: [u32; 3] = [46, 47, 48];

    fn illaoi(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(id, zone, seat, "Illaoi, Prophet of the Great Kraken", 4)
        }
    }

    fn temple(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(illaoi(ILLAOI, zone, 0));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ILLAOI).unwrap(),
            &CARD
        ));
        fixture
    }

    fn tentacles<'c>(ctx: &'c Ctx<'c>, owner: u8) -> Vec<&'c CardInfo> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_TENTACLE && card.owner == owner)
            .collect()
    }

    fn her_items(ctx: &Ctx, index: u8) -> usize {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: held } if source == ILLAOI && held == index))
            .count()
    }

    #[test]
    fn the_script_is_a_play_a_conquer_and_a_hold_trigger_sharing_one_summon_and_a_token_ladder() {
        assert!(std::ptr::eq(
            script_of("Illaoi, Prophet of the Great Kraken").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 3);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[1].trigger, Trigger::Conquer(Who::Me));
        assert_eq!(CARD.abilities[2].trigger, Trigger::Hold(Who::Me));
        for ability in CARD.abilities {
            assert_eq!(ability.targets, [WHERE_THE_TENTACLE_GOES]);
            assert_eq!(WHERE_THE_TENTACLE_GOES.kind, TargetKind::Zone);
            assert!(!ability.optional);
            assert!(ability.condition.is_none());
            assert!(ability.cost.is_none());
        }
        assert!(matches!(CARD.statics, [Static::While(_, _)]));
        assert_eq!(TOKENS.len(), LADDER as usize);
        assert!(TOKENS
            .iter()
            .all(|grant| matches!(grant, Grant::MightIf(_, 1))));
    }

    #[test]
    fn her_might_follows_the_token_units_her_controller_controls() {
        let mut fixture = temple(fixtures::BASE);
        fixture
            .table
            .cards
            .push(illaoi(THEIR_ILLAOI, fixtures::BASE, 1));
        fixture.table.cards.push(fixtures::gold(GOLD, 0, false));
        fixture.table.tokens.push(GOLD);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            token_units_you_control(&ctx, 0),
            0,
            "a Gold is a token, not a unit"
        );
        assert_eq!(might_bonus(&ctx, ILLAOI), 0);
        assert_eq!(ctx.current_might(ILLAOI), 4);
        assert!(statics::grants_on(&ctx, ILLAOI).is_empty());
        let first = ctx
            .spawn(0, Token::Tentacle, Location::Base(0), false)
            .unwrap();
        assert_eq!(token_units_you_control(&ctx, 0), 1);
        assert_eq!(ctx.current_might(ILLAOI), 5, "+1 for her Tentacle");
        ctx.spawn(
            0,
            Token::Tentacle,
            Location::Battlefield(fixtures::BF1),
            false,
        );
        ctx.spawn(0, Token::SandSoldier, Location::Base(0), false);
        assert_eq!(token_units_you_control(&ctx, 0), 3);
        assert_eq!(
            ctx.current_might(ILLAOI),
            7,
            "every token unit you control, wherever it is"
        );
        assert_eq!(
            ctx.current_might(THEIR_ILLAOI),
            5,
            "their Illaoi counts their tokens: the Sprite, and none of hers"
        );
        assert_eq!(
            ctx.current_might(fixtures::SPRITE),
            3,
            "the Sprite itself wears no ladder"
        );
        ctx.kill(first, crate::engine::ctx::Cause::Rule);
        assert_eq!(
            ctx.current_might(ILLAOI),
            6,
            "the ladder reads the board as it stands"
        );
    }

    #[test]
    fn playing_her_asks_where_the_tentacle_goes_and_plays_it_exhausted_there() {
        let mut fixture = temple(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ILLAOI).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(ILLAOI));
        assert_eq!(her_items(&ctx, 0), 1, "her play trigger waits");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "{zone 9}"],
            "her base and the battlefield she holds; no cancel on a trigger"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(tentacles(&ctx, 0).is_empty(), "nothing before it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let spawned = tentacles(&ctx, 0);
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].zone, Some(fixtures::BF1));
        assert!(spawned[0].exhausted);
        assert_eq!(spawned[0].might, Some(1));
        assert!(ctx.is_token(spawned[0].id));
        assert_eq!(
            ctx.current_might(ILLAOI),
            5,
            "+1 for the Tentacle she just played"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn conquering_and_holding_each_play_a_tentacle() {
        let mut fixture = temple(fixtures::BF2);
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            cleanup::establish(&mut ctx, fixtures::BF2),
            cleanup::Established::Conquered(0)
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.points(0), 1);
        assert_eq!(her_items(&ctx, 1), 1, "the conquer trigger");
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        let spawned = tentacles(&ctx, 0);
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].zone, Some(fixtures::BASE));
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = temple(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(her_items(&ctx, 2), 1, "the hold trigger");
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        let spawned = tentacles(&ctx, 0);
        assert_eq!(spawned.len(), 1);
        assert_eq!(spawned[0].zone, Some(fixtures::BF1));
        assert_eq!(ctx.current_might(ILLAOI), 5);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_hold_without_her_an_opponents_score_and_a_move_play_nothing() {
        let mut fixture = temple(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(her_items(&ctx, 2), 0, "Vi holds; she sits in the base");
        assert_eq!(cleanup::score_holds(&mut ctx, 1), [fixtures::BF2]);
        settle(&mut ctx).unwrap();
        assert_eq!(her_items(&ctx, 2), 0);
        crate::engine::march::effect_move(
            &mut ctx,
            &fixtures::effect_of(0),
            ILLAOI,
            Location::Battlefield(fixtures::BF1),
        );
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "a move is neither a play nor a score"
        );
        assert!(tentacles(&ctx, 0).is_empty());
    }
}
