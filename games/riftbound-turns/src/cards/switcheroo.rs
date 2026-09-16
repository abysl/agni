use super::prelude::{
    a_card, a_unit_at_a_battlefield, card_target, done, play, spell, swap_might_this_turn,
    ANOTHER_UNIT_WITH_FIRST,
};
use super::{Card, Flow, Item, Keyword, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const TARGETS: &[TargetSpec] = &[
    a_unit_at_a_battlefield("a unit at a battlefield"),
    a_card(
        ANOTHER_UNIT_WITH_FIRST,
        "another unit at the same battlefield",
    ),
];

fn switch(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let (Some(first), Some(second)) = (card_target(ctx, item, 0), card_target(ctx, item, 1)) {
        swap_might_this_turn(ctx, item, first, second);
    }
    done()
}

pub static CARD: Card = spell(
    "Switcheroo",
    &[Keyword::Hidden, Keyword::Action],
    &[play(TARGETS, switch)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::UNIT_AT_BATTLEFIELD;
    use crate::cards::{Filter, TargetKind, Trigger};
    use crate::engine::ctx::{EntryMove, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{hide, phases, play as play_engine, priority, prompts, settle};
    use crate::state::{Expiry, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::{CardInfo, Target};

    const SWITCHEROO: u32 = 90;
    const THEIR_SWITCHEROO: u32 = 91;
    const BRUTE: u32 = 92;
    const CHAOS_A: u32 = 46;
    const CHAOS_B: u32 = 47;
    const ENERGY: u8 = 2;
    const POWER: u8 = 2;

    fn switcheroo(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Switcheroo", ENERGY, POWER);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.cards.push(switcheroo(SWITCHEROO, 0));
        fixture.table.cards.push(switcheroo(THEIR_SWITCHEROO, 1));
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        for rune in [CHAOS_A, CHAOS_B] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
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

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play_engine::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn play_from_facedown(ctx: &mut Ctx, zone: u16) -> Result<(), Refusal> {
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(SWITCHEROO, chain, 0), 0)
            .unwrap();
        play_engine::begin(ctx, 0, SWITCHEROO, Origin::Facedown { zone }, None)?;
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? else {
            return settle(ctx);
        };
        match (answered.why, answered.answer) {
            (PromptWhy::Target { item, .. }, Answer::Cancel) => play_engine::cancel(ctx, item),
            (PromptWhy::Target { item, spec }, _) => {
                play_engine::choose_targets(ctx, item, spec, &answered.prompt.picked)?
            }
            (why, answer) => panic!("Switcheroo opens only target prompts: {why:?} {answer:?}"),
        }
        settle(ctx)
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        pick(ctx, seat, option)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn deltas(ctx: &Ctx, card: u32) -> Vec<i16> {
        ctx.state_of(card)
            .map(|row| row.might.iter().map(|held| held.delta).collect())
            .unwrap_or_default()
    }

    fn switched(ctx: &mut Ctx) {
        play_from_hand(ctx, 0, SWITCHEROO).unwrap();
        choose(ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        choose(ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        resolve_chain(ctx);
    }

    #[test]
    fn the_script_is_a_hidden_action_choosing_two_units_at_one_battlefield() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Switcheroo").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Switcheroo");
        assert!(CARD.has_keyword(Keyword::Hidden));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(ability.targets.len(), 2);
        let first = &ability.targets[0];
        assert_eq!(first.filter, UNIT_AT_BATTLEFIELD);
        assert_eq!((first.min, first.max), (1, 1));
        assert_eq!(first.kind, TargetKind::Card);
        let second = &ability.targets[1];
        assert_eq!(second.filter, ANOTHER_UNIT_WITH_FIRST);
        assert_eq!(
            second.filter,
            Filter::And(&[
                Filter::Unit,
                Filter::NotSame(0),
                Filter::SameLocationAs(0),
                Filter::AtBattlefield
            ])
        );
        assert_eq!((second.min, second.max), (1, 1));
        assert!(
            !hide::lifted_by(&first.filter) && !hide::lifted_by(&second.filter),
            "737.1.d · both units stand at the hiding battlefield"
        );
    }

    #[test]
    fn it_swaps_the_two_units_might_for_the_turn_as_two_deltas_that_expire_together() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, SWITCHEROO).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {BRUTE}}}"),
                "cancel".to_string()
            ],
            "any unit at any battlefield, friend or foe · Jinx in its base is not offered"
        );
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            labels(&ctx),
            [format!("{{card {BRUTE}}}"), "cancel".to_string()],
            "another unit at the same battlefield · the Sprite elsewhere is not offered"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 1 }),
            format!("{{card {SWITCHEROO}}}: choose another unit at the same battlefield (0 of 1)")
        );
        choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Card(BRUTE)]
        );
        assert!(
            ctx.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Move { card, zone, .. }
                if *card == CHAOS_A && *zone == fixtures::RUNE_DECK)),
            "two Chaos power recycles both Chaos runes: {:?}",
            ctx.effects
        );
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(BRUTE), 4);
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(ctx.current_might(BRUTE), 3);
        assert_eq!(deltas(&ctx, fixtures::VI), [1]);
        assert_eq!(deltas(&ctx, BRUTE), [-1]);
        assert_eq!(
            ctx.state_of(fixtures::VI).unwrap().might[0].until,
            Expiry::EndOfTurn(ctx.turn())
        );
        assert_eq!(might_counter(&ctx, fixtures::VI), 1);
        assert_eq!(might_counter(&ctx, BRUTE), -1);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} and {{card {BRUTE}}} swap Might this turn · 4 and 3",
            fixtures::VI
        )));
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "nothing is drawn");
        assert_eq!(ctx.card(SWITCHEROO).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.seat(0).played_main);
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(BRUTE), 4);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0);
        assert_eq!(might_counter(&ctx, BRUTE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn it_reads_the_layered_might_and_a_later_stupefy_composes_on_the_swapped_value() {
        let mut fixture = armed();
        let spark = fixture
            .table
            .cards
            .iter()
            .position(|card| card.id == fixtures::HAND_SPELL)
            .unwrap();
        fixture.table.cards[spark] =
            fixtures::spell(fixtures::HAND_SPELL, fixtures::HAND, 0, "Stupefy", 1, 0);
        fixture.table.cards[spark].domain = vec!["Mind".into()];
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let discipline = Item::new(
            9,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        let turn = ctx.turn();
        ctx.might(
            fixtures::VI,
            2,
            Expiry::EndOfTurn(turn),
            None,
            discipline.id,
        );
        assert_eq!(ctx.current_might(fixtures::VI), 5, "3 printed plus 2");
        switched(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "the swap reads the buffed 5, not the printed 3"
        );
        assert_eq!(ctx.current_might(BRUTE), 5);
        assert_eq!(deltas(&ctx, fixtures::VI), [2, -1]);
        assert_eq!(deltas(&ctx, BRUTE), [1]);
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.current_might(BRUTE),
            4,
            "Stupefy's -1 lands on the swapped 5"
        );
        assert_eq!(deltas(&ctx, BRUTE), [1, -1]);
        assert_eq!(might_counter(&ctx, BRUTE), 0);
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(BRUTE), 4);
        assert!(deltas(&ctx, fixtures::VI).is_empty());
        assert!(deltas(&ctx, BRUTE).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_left_the_battlefield_before_resolution_leaves_both_untouched() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, SWITCHEROO).unwrap();
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(BRUTE, fixtures::BASE, 1), 1)
            .unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(ctx.current_might(BRUTE), 4);
        assert!(deltas(&ctx, fixtures::VI).is_empty());
        assert!(deltas(&ctx, BRUTE).is_empty());
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            &format!("{{card {SWITCHEROO}}} resolves")
        );
        assert_eq!(ctx.card(SWITCHEROO).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn played_from_facedown_both_units_come_from_the_hiding_battlefield_for_free() {
        let mut fixture = armed();
        fixture.table.card_mut(SWITCHEROO).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.card_state_mut(SWITCHEROO).hidden_at = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        play_from_facedown(&mut ctx, fixtures::BF1).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {BRUTE}}}"),
                "cancel".to_string()
            ],
            "737.1.d · the Sprite at the other battlefield is not offered"
        );
        choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "cancel".to_string()]
        );
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(
            ctx.blob.chain[0].origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        let paid: Vec<&Effect> = ctx
            .effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .collect();
        assert!(paid.is_empty(), "a hidden card reacts for free");
        resolve_chain(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert_eq!(ctx.current_might(BRUTE), 3);
        assert_eq!(ctx.card(SWITCHEROO).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn the_wrong_seat_a_unit_in_base_a_unit_elsewhere_the_same_unit_and_a_short_purse_are_refused()
    {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SWITCHEROO)),
            Err(Refusal::NotYourTurn),
            "an action has no window on the other seat's turn outside a showdown"
        );
        play_from_hand(&mut ctx, 0, SWITCHEROO).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in a base is not at a battlefield"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a battlefield is not a unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the first unit is mandatory"
        );
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit cannot swap Might with itself"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the Sprite stands at another battlefield"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        assert!(ctx.effects.is_empty(), "nothing was paid");
        choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(SWITCHEROO).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(deltas(&ctx, fixtures::VI).is_empty());
        drop(ctx);

        let mut alone = armed();
        alone.table.card_mut(BRUTE).unwrap().zone = Some(fixtures::BF2);
        alone.resolve();
        let mut ctx = alone.ctx();
        play_from_hand(&mut ctx, 0, SWITCHEROO).unwrap();
        choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(
            labels(&ctx),
            ["cancel"],
            "Vi stands alone, so there is no second unit at her battlefield"
        );
        choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut broke = armed();
        broke.table.cards.retain(|card| card.id != CHAOS_B);
        broke.resolve();
        let ctx = broke.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, SWITCHEROO)),
            Err(Refusal::NoPowerOf),
            "two Chaos power wants two Chaos runes"
        );
    }
}
