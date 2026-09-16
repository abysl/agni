use super::prelude::{a_unit, card_target, done, might_this_turn, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const GLORY: i16 = 3;

fn glorify(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, GLORY, None);
        ctx.narrate(format!("{{card {unit}}} gets +{GLORY} might this turn"));
    }
    done()
}

pub static CARD: Card = spell(
    "Call to Glory",
    &[Keyword::Reaction],
    &[play(&[a_unit("a unit to give +3 might")], glorify)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT;
    use crate::cards::wallop::{buffs_that_could_pay, IGNORED_COST};
    use crate::cards::{script_of, Cost, Trigger};
    use crate::engine::ctx::COUNTER_MIGHT;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::{cost, pay, priority};
    use crate::state::{ChainItem, Expiry, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const CALL: u32 = 90;

    fn call_card() -> CardInfo {
        let mut card = fixtures::spell(CALL, fixtures::HAND, 0, "Call to Glory", 3, 0);
        card.domain = vec!["Order".into()];
        card
    }

    fn rally() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(call_card());
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_a_reaction_that_gives_one_unit_three_might_for_the_turn() {
        assert!(std::ptr::eq(script_of("Call to Glory").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(GLORY, 3);
        assert_eq!(IGNORED_COST, Cost::FREE);
    }

    #[test]
    fn paid_in_full_the_chosen_unit_reads_plus_three_until_the_turn_ends() {
        let mut fixture = rally();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "cancel"]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "3 energy from four ready runes"
        );
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 5);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 3);
        assert_eq!(
            ctx.state_of(fixtures::THEIR_UNIT).unwrap().might[0].until,
            Expiry::EndOfTurn(1)
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets +3 might this turn",
            fixtures::THEIR_UNIT
        )));
        ctx.expire(Expiry::EndOfTurn(1));
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 2);
        assert_eq!(might_counter(&ctx, fixtures::THEIR_UNIT), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn as_a_reaction_it_answers_a_spell_on_the_chain_and_resolves_first() {
        let mut fixture = rally();
        for rune in [46, 47, 48] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2, "the reaction sits on top");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.current_might(fixtures::VI), 6);
    }

    #[test]
    fn a_target_that_left_the_board_gets_nothing_and_a_hand_card_is_refused() {
        let mut fixture = rally();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::TRASH, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert!(ctx
            .state_of(fixtures::VI)
            .is_none_or(|row| row.might.is_empty()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
    }

    #[test]
    #[ignore = "engine gap · spend a buff: an additional-cost kind that spends a friendly buff at the pay stage and ignores the printed cost; with no ready rune and a buffed unit, the Call must be playable for the buff alone"]
    fn with_no_rune_ready_a_buffed_unit_pays_for_the_call_instead() {
        let mut fixture = rally();
        for id in [fixtures::RUNE_A, 41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        ctx.buff(fixtures::VI);
        assert_eq!(buffs_that_could_pay(&ctx, 0), [fixtures::VI]);
        let item = ChainItem::new(1, ItemKind::Spell { card: CALL }, 0, Origin::Hand);
        assert!(!pay::affordable(&ctx, 0, &cost::of_item(&ctx, &item, None)));
        fixtures::play_from_hand(&mut ctx, 0, CALL).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(!ctx.is_buffed(fixtures::VI), "the buff is the whole price");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 5);
    }
}
