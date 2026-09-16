use super::prelude::{a_unit, card_target, done, is_empowered, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const BOOST: i16 = 2;
pub const EMPOWERED_BOOST: i16 = 4;

pub fn boost_for(empowered: bool) -> i16 {
    if empowered {
        EMPOWERED_BOOST
    } else {
        BOOST
    }
}

fn roar(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let boost = boost_for(is_empowered(ctx, unit));
    might_this_turn(ctx, item, unit, boost, None);
    ctx.narrate(format!("{{card {unit}}} gets +{boost} Might this turn"));
    done()
}

pub static CARD: Card = spell(
    "Guttural Roar",
    &[Keyword::Action],
    &[play(&[a_unit("a unit")], roar)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{this_turn, UNIT};
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::COUNTER_EMPOWERED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const ROAR: u32 = 90;
    const BODY_RUNE: u32 = 46;

    fn roar_card() -> CardInfo {
        CardInfo {
            domain: vec!["Body".into()],
            ..fixtures::spell(ROAR, fixtures::HAND, 0, "Guttural Roar", 2, 0)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(roar_card());
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(ROAR).unwrap(), &CARD));
        fixture
    }

    fn empowered(mut fixture: Fixture, unit: u32) -> Fixture {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(unit),
            counter: COUNTER_EMPOWERED,
            value: 1,
        });
        fixture.table.counters.sort();
        fixture.resolve();
        fixture
    }

    fn cast_on(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, ROAR).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_script_is_an_action_over_any_unit_with_two_boosts() {
        assert!(std::ptr::eq(script_of("Guttural Roar").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!(ability.targets[0].kind, TargetKind::Card);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(boost_for(false), BOOST);
        assert_eq!(boost_for(true), EMPOWERED_BOOST);
        assert_eq!((BOOST, EMPOWERED_BOOST), (2, 4));
    }

    #[test]
    fn a_plain_unit_gets_two_this_turn_friend_or_foe() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ROAR).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"],
            "any unit, friendly or enemy"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 4);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} gets +2 Might this turn".to_string()));
        assert_eq!(ctx.card(ROAR).unwrap().zone, Some(fixtures::TRASH));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2, "this turn only");
    }

    #[test]
    fn an_empowered_unit_gets_four_instead_read_as_the_spell_resolves() {
        let mut fixture = empowered(armed(), fixtures::VI);
        let mut ctx = fixture.ctx();
        assert!(ctx.is_empowered(fixtures::VI));
        cast_on(&mut ctx, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 7);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gets +4 Might this turn".to_string()));
        let mut late = armed();
        let mut ctx = late.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ROAR).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert!(ctx.empower(fixtures::VI), "Empowered in response");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            7,
            "the Empowered check happens at resolution"
        );
    }

    #[test]
    fn a_gear_a_legend_and_an_empty_pick_are_refused_and_a_gone_target_gets_nothing() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_GEAR).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ROAR).unwrap();
        for wrong in [
            fixtures::HAND_GEAR,
            fixtures::LEGEND_CARD,
            fixtures::GROUNDS,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a unit"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        ctx.bounce(fixtures::THEIR_UNIT);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("Might this turn")));
        assert_eq!(ctx.card(ROAR).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }
}
