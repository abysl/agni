use super::prelude::{a_card, card_target, done, play, spell, stun, ATTACKING_UNIT};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};

fn thwonk(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        stun(ctx, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Thwonk!",
    &[Keyword::Action, Keyword::Repeat(REPEAT)],
    &[play(&[a_card(ATTACKING_UNIT, "an attacking unit")], thwonk)],
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

    const THWONK: u32 = 90;
    const THEIR_THWONK: u32 = 91;
    const ALLY: u32 = 92;
    const EXTRA_RUNES: [u32; 2] = [46, 47];

    fn thwonk_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Thwonk!", 2, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn charge() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(thwonk_card(THWONK, 0));
        fixture.table.cards.push(thwonk_card(THEIR_THWONK, 1));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF2, 0, "Vanguard", 2));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
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
    fn the_script_is_a_repeatable_action_over_one_attacking_unit() {
        assert!(std::ptr::eq(script_of("Thwonk!").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, ATTACKING_UNIT);
    }

    #[test]
    fn only_attackers_are_offered_and_the_chosen_one_deals_no_combat_damage_this_turn() {
        let mut fixture = charge();
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(fixtures::VI));
        assert!(ctx.mark_attacker(ALLY));
        ctx.mark_defender(fixtures::SPRITE);
        fixtures::play_from_hand(&mut ctx, 0, THWONK).unwrap();
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
            ["{card 50}", "{card 92}", "cancel"],
            "the defender and the units in bases are not attacking"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        assert!(!ctx.is_stunned(fixtures::VI), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(fixtures::VI));
        assert_eq!(ctx.combat_might(fixtures::VI), 0);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(!ctx.is_stunned(ALLY));
        assert!(ctx.blob.log.contains(&"{card 50} is stunned".to_string()));
        assert_eq!(ctx.card(THWONK).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_for_two_more_energy_it_stuns_two_attackers() {
        let mut fixture = charge();
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(fixtures::VI));
        assert!(ctx.mark_attacker(ALLY));
        fixtures::play_from_hand(&mut ctx, 0, THWONK).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "four of the five ready runes pay the spell and the repeat"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_stunned(fixtures::VI));
        assert!(ctx.is_stunned(ALLY));
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_an_attacker_it_cannot_be_played_and_a_unit_at_rest_is_refused() {
        let mut fixture = charge();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_THWONK)),
            Err(Refusal::NotYourTurn)
        );
        assert!(ctx.mark_attacker(ALLY));
        fixtures::play_from_hand(&mut ctx, 0, THWONK).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit that is not attacking is no target"
        );
        assert_eq!(fixtures::labels(&ctx), ["{card 92}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(THWONK).unwrap().zone, Some(fixtures::HAND));
        let mut quiet = charge();
        let mut ctx = quiet.ctx();
        fixtures::play_from_hand(&mut ctx, 0, THWONK).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "nobody is attacking: only the way out"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(THWONK).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
