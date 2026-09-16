use super::prelude::{a_unit_at_a_battlefield, card_target, deal, done, facedown_of, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;
pub const DAMAGE_WITH_A_FACEDOWN_CARD: u8 = 4;

pub fn harpoon_damage(ctx: &Ctx, seat: u8) -> u8 {
    if facedown_of(ctx, seat).is_empty() {
        DAMAGE
    } else {
        DAMAGE_WITH_A_FACEDOWN_CARD
    }
}

fn harpoon(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let amount = harpoon_damage(ctx, item.controller);
    if amount == DAMAGE_WITH_A_FACEDOWN_CARD {
        ctx.narrate(format!(
            "{{seat {}}} controls a facedown card · the harpoon deals {amount} instead",
            item.controller
        ));
    }
    if deal(ctx, item, unit, amount) {
        ctx.narrate(format!("{{card {unit}}} takes {amount}"));
    }
    done()
}

pub static CARD: Card = spell(
    "Monster Harpoon",
    &[Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        harpoon,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::PromptWhy;
    use crate::Refusal;

    const HARPOON: u32 = 90;
    const BRUTE: u32 = 91;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            HARPOON,
            fixtures::HAND,
            0,
            "Monster Harpoon",
            1,
            1,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture.resolve();
        fixture
    }

    fn with_a_facedown_card(mut fixture: Fixture) -> Fixture {
        fixture.table.card_mut(fixtures::HAND_HIDDEN).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(fixtures::HAND_HIDDEN).hidden_at = Some(fixtures::BF1);
        fixture.resolve();
        fixture
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, HARPOON).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        fixtures::pass_until_open(ctx);
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_is_an_action_over_a_unit_at_a_battlefield_that_reads_the_casters_facedown_cards()
    {
        assert!(std::ptr::eq(script_of("Monster Harpoon").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(harpoon_damage(&ctx, 0), DAMAGE);
        assert_eq!(harpoon_damage(&ctx, 1), DAMAGE);
        drop(ctx);
        let mut hidden = with_a_facedown_card(armed());
        let ctx = hidden.ctx();
        assert!(ctx.is_facedown(fixtures::HAND_HIDDEN));
        assert_eq!(harpoon_damage(&ctx, 0), DAMAGE_WITH_A_FACEDOWN_CARD);
        assert_eq!(
            harpoon_damage(&ctx, 1),
            DAMAGE,
            "the other seat controls no facedown card"
        );
    }

    #[test]
    fn without_a_facedown_card_it_deals_two() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HARPOON).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 91}", "cancel"],
            "the units at battlefields, friend or foe"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(BRUTE), 2);
        assert!(ctx.on_board(BRUTE));
        assert!(ctx.blob.log.contains(&"{card 91} takes 2".to_string()));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("controls a facedown card")));
        assert_eq!(ctx.card(HARPOON).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_a_facedown_card_it_deals_four_instead() {
        let mut fixture = with_a_facedown_card(armed());
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, BRUTE);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: BRUTE,
            n: DAMAGE_WITH_A_FACEDOWN_CARD,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(BRUTE), "four kills the 4-Might Brute");
        assert!(ctx.blob.log.contains(
            &"{seat 0} controls a facedown card · the harpoon deals 4 instead".to_string()
        ));
        assert!(ctx.blob.log.contains(&"{card 91} takes 4".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_in_a_base_is_refused_and_a_target_that_left_is_not_hit() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HARPOON).unwrap();
        for wrong in [fixtures::THEIR_UNIT, fixtures::VI, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base or not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.recall(BRUTE, false);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(BRUTE), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert_eq!(ctx.card(HARPOON).unwrap().zone, Some(fixtures::TRASH));
    }
}
