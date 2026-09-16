use super::prelude::{
    a_unit_at_a_battlefield, card_target, deal, done, play, spell, target, ANOTHER_UNIT,
};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;
pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[Power::Domain(Domain::Fury)],
};

pub const UP_TO_ONE_OTHER_UNIT: TargetSpec =
    target(ANOTHER_UNIT, 0, 1, TargetKind::Card, "up to one other unit");

fn pierce(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for index in [0, 1] {
        if let Some(unit) = card_target(ctx, item, index) {
            if deal(ctx, item, unit, DAMAGE) {
                ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
            }
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Piercing Light",
    &[Keyword::Repeat(REPEAT)],
    &[play(
        &[
            a_unit_at_a_battlefield("a unit at a battlefield"),
            UP_TO_ONE_OTHER_UNIT,
        ],
        pierce,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const LIGHT: u32 = 90;
    const THEIR_LIGHT: u32 = 91;
    const BRUTE: u32 = 92;
    const MY_EXTRA: [u32; 3] = [46, 47, 48];

    fn light(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Piercing Light", 2, 1);
        card.domain = vec!["Fury".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(light(LIGHT, 0));
        fixture.table.cards.push(light(THEIR_LIGHT, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
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

    #[test]
    fn the_script_is_a_repeatable_plain_spell_over_a_battlefield_unit_and_up_to_one_other() {
        assert!(std::ptr::eq(script_of("Piercing Light").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[1], UP_TO_ONE_OTHER_UNIT);
        assert_eq!((UP_TO_ONE_OTHER_UNIT.min, UP_TO_ONE_OTHER_UNIT.max), (0, 1));
    }

    #[test]
    fn the_first_takes_two_then_another_unit_anywhere_takes_two_or_the_second_is_skipped() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LIGHT).unwrap();
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
            ["{card 60}", "{card 92}", "cancel"],
            "units at battlefields only: the two in bases are not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "skip", "cancel"],
            "any other unit, anywhere, or none"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(BRUTE),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            4,
            "two energy exhausted; the Fury power recycles the spent rune"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.damage_on(BRUTE), 2);
        assert!(
            !ctx.on_board(fixtures::THEIR_UNIT),
            "two kills a 2-Might Jinx"
        );
        assert!(ctx.blob.log.contains(&"{card 92} takes 2".to_string()));
        assert_eq!(ctx.card(LIGHT).unwrap().zone, Some(fixtures::TRASH));
        let mut alone = armed();
        let mut ctx = alone.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LIGHT).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert_eq!(ctx.blob.chain[0].spec_counts, [1, 0]);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 2);
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_for_two_energy_and_a_fury_it_deals_a_second_pair() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LIGHT).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 2 }));
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 3 }));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(item.spec_counts, [1, 1, 1, 0]);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "four energy exhausted, the spent rune and one of them recycled for the two Fury"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(BRUTE), 4, "two from each execution");
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 2);
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_in_a_base_is_refused_first_and_the_first_pick_is_refused_as_the_other() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_LIGHT)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, LIGHT).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Vi sits in her base"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[BRUTE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the other unit is another unit"
        );
        assert!(ctx.blob.prompt.is_some());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(LIGHT).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_second_groups_other_unit_excludes_that_groups_own_first_pick() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LIGHT).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 3 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 81}", "{card 92}", "skip", "cancel"],
            "the Sprite is this group's first pick, the Brute is fair game again"
        );
    }
}
