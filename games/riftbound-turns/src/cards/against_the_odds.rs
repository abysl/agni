use super::prelude::{a_card, card_target, done, location_of, might_this_turn, play, spell};
use super::{Card, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const PER_ENEMY: i16 = 2;

pub const FRIENDLY_UNIT_AT_A_BATTLEFIELD: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::AtBattlefield]);

pub fn enemies_there(ctx: &Ctx, unit: u32) -> usize {
    let Some(at) = location_of(ctx, unit) else {
        return 0;
    };
    let seat = ctx.controller(unit);
    ctx.units_at(at)
        .into_iter()
        .filter(|other| ctx.controller(*other) != seat)
        .count()
}

fn rally(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let enemies = enemies_there(ctx, unit);
    let Ok(delta) = i16::try_from(enemies).map(|count| count.saturating_mul(PER_ENEMY)) else {
        return done();
    };
    if delta == 0 {
        ctx.narrate(format!("{{card {unit}}} faces no enemy there"));
        return done();
    }
    might_this_turn(ctx, item, unit, delta, None);
    ctx.narrate(format!(
        "{{card {unit}}} gets +{delta} Might this turn · {enemies} enemy units there"
    ));
    done()
}

pub static CARD: Card = spell(
    "Against the Odds",
    &[Keyword::Reaction],
    &[play(
        &[a_card(
            FRIENDLY_UNIT_AT_A_BATTLEFIELD,
            "a friendly unit at a battlefield",
        )],
        rally,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{phases, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const ODDS: u32 = 90;
    const RAIDER: u32 = 91;
    const THEIR_ODDS: u32 = 92;

    fn odds(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Fury".into()],
            ..fixtures::spell(id, fixtures::HAND, seat, "Against the Odds", 2, 0)
        }
    }

    fn outnumbered() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(odds(ODDS, 0));
        fixture.table.cards.push(odds(THEIR_ODDS, 1));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF2, 1, "Raider", 2));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ODDS).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_a_reaction_over_one_friendly_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Against the Odds").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].kind, TargetKind::Card);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT_AT_A_BATTLEFIELD);
        assert_eq!(PER_ENEMY, 2);
    }

    #[test]
    fn vi_gets_two_might_per_enemy_at_her_battlefield_until_the_turn_ends() {
        let mut fixture = outnumbered();
        let mut ctx = fixture.ctx();
        assert_eq!(
            enemies_there(&ctx, fixtures::VI),
            2,
            "the Sprite and the Raider"
        );
        fixtures::play_from_hand(&mut ctx, 0, ODDS).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "only a friendly unit at a battlefield · the enemy units and units in base are not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "nothing before it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 7);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +4 Might this turn · 2 enemy units there".to_string()));
        assert_eq!(ctx.card(ODDS).unwrap().zone, Some(fixtures::TRASH));
        phases::end_turn(&mut ctx).unwrap();
        settle(&mut ctx).unwrap();
        phases::finish_turn(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "the bonus lasts the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_count_is_read_as_the_spell_resolves_and_no_enemy_means_no_bonus() {
        let mut fixture = outnumbered();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ODDS).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        for enemy in [fixtures::SPRITE, RAIDER] {
            ctx.move_unit(
                enemy,
                Location::Base(1),
                crate::engine::ctx::MoveCause::Effect,
            );
        }
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} faces no enemy there".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_in_base_is_refused_and_the_other_seat_cannot_react_on_an_open_turn() {
        let mut fixture = outnumbered();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: THEIR_ODDS,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::NotYourTurn),
            "nothing to react to in the other seat's Neutral Open"
        );
        fixtures::play_from_hand(&mut ctx, 0, ODDS).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "Vi in base is no target"
        );
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(ODDS).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
