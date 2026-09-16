use super::prelude::{card_targets, deal, done, play, spell, target};
use super::{
    Card, Cost, Domain, Filter, Flow, Item, Keyword, Power, Stage, TargetKind, TargetSpec,
};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 1;
pub const UP_TO: u8 = 3;
pub const REPEAT: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Mind)],
};

pub const UNITS_AT_ONE_LOCATION: TargetSpec = target(
    Filter::And(&[Filter::Unit, Filter::SameLocationAsPicks]),
    0,
    UP_TO,
    TargetKind::Card,
    "up to three units at one location",
);

fn breathe(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for unit in card_targets(ctx, item) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Bellows Breath",
    &[Keyword::Action, Keyword::Repeat(REPEAT)],
    &[play(&[UNITS_AT_ONE_LOCATION], breathe)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::PromptWhy;
    use crate::Refusal;

    const BELLOWS: u32 = 90;
    const SECOND: u32 = 91;
    const THIRD: u32 = 92;
    const FOURTH: u32 = 93;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut bellows = fixtures::spell(BELLOWS, fixtures::HAND, 0, "Bellows Breath", 1, 1);
        bellows.domain = vec!["Mind".into()];
        fixture.table.cards.push(bellows);
        for (id, zone, seat) in [
            (SECOND, fixtures::BF2, 1),
            (THIRD, fixtures::BF2, 0),
            (FOURTH, fixtures::BF2, 1),
        ] {
            fixture
                .table
                .cards
                .push(fixtures::unit(id, zone, seat, "Jinx", 4));
        }
        for id in [46, 47] {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Mind", false));
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn repeat_is_asked_before_any_target_and_two_groups_are_chosen_and_dealt() {
        assert!(std::ptr::eq(script_of("Bellows Breath").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(Cost::FREE)));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BELLOWS).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 50}",
                "{card 60}",
                "{card 81}",
                "{card 91}",
                "{card 92}",
                "{card 93}",
                "done",
                "skip",
                "cancel"
            ]
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 91}", "{card 92}", "{card 93}", "done", "cancel"],
            "after the first pick only that location's units remain"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["done", "cancel"],
            "a fourth pick is refused"
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["done", "cancel"],
            "the second group has its own first pick and the enemy base is elsewhere"
        );
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(item.spec_counts, [3, 1]);
        fixtures::pass_until_open(&mut ctx);
        for unit in [fixtures::SPRITE, SECOND, THIRD, fixtures::VI] {
            assert_eq!(ctx.damage_on(unit), 1, "{unit} takes one");
        }
        assert_eq!(ctx.damage_on(FOURTH), 0);
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, crate::engine::ctx::Event::PlayedSpell { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn repeat_declined_asks_one_group_and_units_in_both_groups_take_one_each_time() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BELLOWS).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        assert!(ctx.blob.prompt.is_none(), "one group only");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 1);
        assert_eq!(ctx.damage_on(SECOND), 0);
        let mut twice = armed();
        let mut ctx = twice.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BELLOWS).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "done").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(SECOND), 2, "the same unit twice takes two");
    }

    #[test]
    fn an_unaffordable_repeat_is_not_offered_and_a_far_pick_is_refused() {
        let mut fixture = armed();
        fixture.table.cards.retain(|card| card.id != 47);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BELLOWS).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 1, spec: 0 }),
            "without a Mind rune to spare the Repeat is skipped"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::SPRITE, fixtures::VI]),
            Err(Refusal::Illegal(
                crate::engine::legal::Reason::NotALegalTarget
            ))
        );
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
    }
}
