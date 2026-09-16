use super::prelude::{
    a_friendly_unit, card_target, done, grant_this_turn, might_this_turn, on_move_to_battlefield,
    unit,
};
use super::{Card, Flow, Grant, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const TARGET: TargetSpec = a_friendly_unit("a friendly unit to give my keywords and Might");

pub fn keywords_of(ctx: &Ctx, card: u32) -> Vec<Keyword> {
    let mut keywords: Vec<Keyword> = ctx
        .script(card)
        .map(|script| script.keywords.to_vec())
        .unwrap_or_default();
    if let Some(row) = ctx.state_of(card) {
        keywords.extend(row.granted.iter().map(|(keyword, _)| *keyword));
    }
    keywords.extend(
        statics::grants_on(ctx, card)
            .into_iter()
            .filter_map(|grant| match grant {
                Grant::Keyword(keyword) => Some(keyword),
                _ => None,
            }),
    );
    keywords
}

fn lend_the_arm(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !ctx.on_board(me) {
        ctx.narrate(format!(
            "{{card {me}}} is gone · there are no keywords or Might to give"
        ));
        return done();
    }
    let keywords = keywords_of(ctx, me);
    let might = i16::try_from(ctx.current_might(me)).unwrap_or(i16::MAX);
    for keyword in keywords {
        grant_this_turn(ctx, unit, keyword);
    }
    if might != 0 {
        might_this_turn(ctx, item, unit, might, None);
    }
    ctx.narrate(format!(
        "{{card {unit}}} gets {{card {me}}}'s keywords and +{might} Might this turn"
    ));
    done()
}

pub static CARD: Card = unit(
    "Kato the Arm",
    &[Keyword::Deflect(1)],
    &[on_move_to_battlefield(&[TARGET], lend_the_arm)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Trigger, Where, Who};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, play, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;

    const KATO: u32 = 90;

    fn ring() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut kato = fixtures::unit(KATO, fixtures::BASE, 0, "Kato the Arm", 3);
        kato.domain = vec!["Body".into()];
        kato.energy = Some(4);
        kato.power = Some(1);
        fixture.table.cards.push(kato);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(KATO).unwrap(), &CARD));
        fixture
    }

    fn march(fixture: &mut Fixture, from: Location, to: Location) -> Ctx<'_> {
        let zone = to.battlefield().unwrap_or(fixtures::BASE);
        let action = fixtures::move_action(KATO, zone, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march::standard_move(&mut ctx, 0, KATO, from, to);
        settle(&mut ctx).unwrap();
        ctx
    }

    #[test]
    fn the_script_prints_deflect_one_and_a_move_to_battlefield_trigger_aimed_at_a_friendly_unit() {
        assert!(std::ptr::eq(script_of("Kato the Arm").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Deflect(1)]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert_eq!(ability.targets, &[TARGET]);
        assert!(!ability.optional);
    }

    #[test]
    fn his_keywords_are_the_printed_the_granted_and_the_projected_ones() {
        let mut fixture = ring();
        let mut ctx = fixture.ctx();
        assert_eq!(keywords_of(&ctx, KATO), [Keyword::Deflect(1)]);
        let until = this_turn(&ctx);
        assert!(ctx.grant(KATO, Keyword::Ganking, until));
        assert_eq!(
            keywords_of(&ctx, KATO),
            [Keyword::Deflect(1), Keyword::Ganking]
        );
        assert!(keywords_of(&ctx, fixtures::VI).is_empty());
    }

    #[test]
    fn arriving_at_a_battlefield_lends_his_keywords_and_his_might_to_the_pick_for_the_turn() {
        let mut fixture = ring();
        let mut ctx = march(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        let pump = crate::state::ChainItem::new(
            9,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            crate::state::Origin::Hand,
        );
        might_this_turn(&mut ctx, &pump, KATO, 2, None);
        assert_eq!(ctx.current_might(KATO), 5);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {KATO}}}")
            ],
            "any friendly unit, himself included"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert_eq!(ctx.deflect_of(fixtures::VI), 0);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.deflect_of(fixtures::VI), 1);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Deflect(1)));
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3 + 5,
            "his Might as the trigger resolves, pump included"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets {{card {KATO}}}'s keywords and +5 Might this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.deflect_of(fixtures::VI), 0, "this turn only");
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_kato_gone_before_the_trigger_resolves_gives_nothing_and_a_walk_home_fires_nothing() {
        let mut fixture = ring();
        let mut ctx = march(
            &mut fixture,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(ctx.bounce(KATO));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.deflect_of(fixtures::VI), 0);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {KATO}}} is gone · there are no keywords or Might to give"
        )));
        drop(ctx);

        let mut fixture = ring();
        fixture.table.card_mut(KATO).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = march(
            &mut fixture,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
    }
}
