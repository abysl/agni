use super::prelude::{an_item, counter_spell, done, play, spell};
use super::{Card, Filter, Keyword};

pub const DEFY_COUNTERABLE: Filter = Filter::And(&[
    Filter::Spell,
    Filter::EnergyAtMost(4),
    Filter::PowerAtMost(1),
]);

pub static CARD: Card = spell(
    "Defy",
    &[Keyword::Reaction],
    &[play(
        &[an_item(DEFY_COUNTERABLE, "a spell to counter")],
        |ctx, item, _| {
            counter_spell(ctx, item, 0);
            done()
        },
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play, priority, prompts, settle, targets};
    use crate::state::{ChainItem, ItemKind, ItemStatus, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};

    const DEFY: u32 = 90;
    const CALM_RUNE: u32 = 46;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut defy = fixtures::spell(DEFY, fixtures::HAND, 1, "Defy", 1, 1);
        defy.domain = vec!["Calm".into()];
        fixture.table.cards.push(defy);
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 1, "Calm", false));
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
        play::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let answered = prompts::answer(ctx, seat, Pick { prompt, option })?;
        if let Some(answered) = answered {
            match (answered.why, answered.answer) {
                (PromptWhy::Target { item, .. }, Answer::Cancel) => play::cancel(ctx, item),
                (PromptWhy::Target { item, spec }, _) => {
                    play::choose_targets(ctx, item, spec, &answered.prompt.picked)?
                }
                _ => {}
            }
        }
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn finalized(id: u16, kind: ItemKind, controller: u8, origin: Origin) -> ChainItem {
        let mut item = ChainItem::new(id, kind, controller, origin);
        item.status = ItemStatus::Finalized;
        item
    }

    fn countered_from_the_trash(leave: crate::state::Leave) -> (Fixture, Vec<u32>, Vec<u32>) {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_SPELL).unwrap().zone = Some(fixtures::TRASH);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        play::begin(
            &mut ctx,
            0,
            fixtures::HAND_SPELL,
            Origin::Trash { leave },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, DEFY).unwrap();
        pick(&mut ctx, 1, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.log.contains(&"{card 71} is countered".to_string()));
        let banished = ctx.banished_of(0);
        let deck: Vec<u32> = ctx
            .table
            .held(fixtures::MAIN_DECK, 0)
            .map(|card| card.id)
            .collect();
        assert_ne!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        drop(ctx);
        (fixture, banished, deck)
    }

    #[test]
    fn defy_sends_a_flow_played_spell_to_banishment_and_a_recycled_one_to_the_deck() {
        let (_, banished, _) = countered_from_the_trash(crate::state::Leave::Banish);
        assert_eq!(
            banished,
            [fixtures::HAND_SPELL],
            "829.1.b.1: banished, not trashed again"
        );
        let (_, banished, deck) = countered_from_the_trash(crate::state::Leave::Recycle);
        assert!(banished.is_empty());
        assert_eq!(
            deck.first(),
            Some(&fixtures::HAND_SPELL),
            "390.3.a: recycled to the bottom"
        );
    }

    #[test]
    fn defy_counters_a_cheap_spell_which_never_resolves_and_keeps_its_cost() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, DEFY)),
            Err(Refusal::NotYourTurn),
            "no chain on the other seat's turn: nothing to react to"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let paid = ctx.effects.clone();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, DEFY)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 0 holds priority first"
        );
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        play_from_hand(&mut ctx, 1, DEFY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(labels(&ctx), ["{card 71} on the chain", "cancel"]);
        let offered = prompts::offered(&ctx);
        assert_eq!(offered[0].answer, Answer::Item(1));
        assert_eq!(offered[0].card, Some(fixtures::HAND_SPELL));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 2, spec: 0 }),
            "{card 90}: choose a spell to counter (0 of 1)"
        );
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 0, &[2]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Defy never counters itself"
        );
        pick(&mut ctx, 1, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 2);
        assert_eq!(ctx.blob.chain[1].targets, [TargetRef::Item(1)]);
        assert!(ctx.effects.contains(&Effect::exhaust(CALM_RUNE)));
        assert!(
            ctx.effects.contains(&Effect::Move {
                card: CALM_RUNE,
                zone: fixtures::RUNE_DECK,
                seat: 1,
                index: BOTTOM
            }),
            "the Calm rune pays Defy's power: {:?}",
            ctx.effects
        );
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx.blob.priority.is_none());
        assert!(ctx.blob.is_neutral_open());
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.card(DEFY).unwrap().zone, Some(fixtures::TRASH));
        assert!(
            ctx.effects.starts_with(&paid),
            "nothing is refunded: {:?}",
            ctx.effects
        );
        assert!(ctx.blob.log.contains(&"{card 71} is countered".to_string()));
        assert!(!ctx.blob.log.iter().any(|line| line == "{card 71} resolves"));
        assert!(ctx.blob.log.contains(&"{card 90} resolves".to_string()));
        let spells: Vec<&Event> = ctx
            .events
            .iter()
            .filter(|event| matches!(event, Event::PlayedSpell { .. }))
            .collect();
        assert_eq!(
            spells,
            [&Event::PlayedSpell {
                item: 2,
                controller: 1,
                nth: 1
            }],
            "the countered spell was never played"
        );
        assert!(!ctx.blob.seat(0).played_main);
        assert!(ctx.blob.seat(1).played_main);
    }

    #[test]
    fn defy_offers_only_spells_within_the_printed_cost_and_never_abilities() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        ctx.blob.chain.push(finalized(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        ));
        ctx.blob.chain.push(finalized(
            2,
            ItemKind::Trigger {
                source: fixtures::VI,
                index: 0,
            },
            0,
            Origin::Board,
        ));
        let defy = ChainItem::new(3, ItemKind::Spell { card: DEFY }, 1, Origin::Hand);
        let spec = &CARD.abilities[0].targets[0];
        let offered = |ctx: &Ctx| targets::candidates(ctx, &defy, spec);
        assert_eq!(offered(&ctx), [TargetRef::Item(1)]);
        let spark = |ctx: &mut Ctx, energy: u8, power: u8| {
            let face = ctx.table.card_mut(fixtures::HAND_SPELL).unwrap();
            face.energy = Some(energy);
            face.power = Some(power);
        };
        spark(&mut ctx, 4, 1);
        assert_eq!(offered(&ctx), [TargetRef::Item(1)]);
        spark(&mut ctx, 5, 0);
        assert_eq!(
            offered(&ctx),
            Vec::<TargetRef>::new(),
            "5 energy is too much"
        );
        spark(&mut ctx, 0, 2);
        assert_eq!(
            offered(&ctx),
            Vec::<TargetRef>::new(),
            "2 power is too much"
        );
        spark(&mut ctx, 5, 0);
        ctx.blob.chain[0].origin = Origin::Facedown {
            zone: fixtures::BF2,
        };
        assert_eq!(
            offered(&ctx),
            Vec::<TargetRef>::new(),
            "a hidden spell played for nothing is still judged by its printed cost"
        );
        spark(&mut ctx, 4, 1);
        assert_eq!(offered(&ctx), [TargetRef::Item(1)]);
        ctx.blob.chain[0].controller = 1;
        assert_eq!(
            offered(&ctx),
            [TargetRef::Item(1)],
            "Defy may counter its controller's own spell"
        );
        ctx.blob.chain[0].status = ItemStatus::Resolving;
        assert_eq!(offered(&ctx), Vec::<TargetRef>::new());
    }

    #[test]
    fn defy_with_nothing_to_counter_opens_a_cancel_only_prompt_and_comes_back() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::HAND_SPELL).unwrap().power = Some(2);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        let hand = ctx.hand_of(1).len();
        play_from_hand(&mut ctx, 1, DEFY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        let offered = labels(&ctx);
        assert!(
            !offered.iter().any(|label| label.contains("on the chain")),
            "a two-power spell is out of Defy's reach: {offered:?}"
        );
        let cancel = offered
            .iter()
            .position(|label| label == "cancel")
            .expect("a play can always be taken back") as u16;
        assert_eq!(
            play::choose_targets(&mut ctx, 2, 0, &[1]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.blob.prompt.is_some());
        pick(&mut ctx, 1, cancel).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.card(DEFY).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.hand_of(1).len(), hand);
        assert!(ctx.effects.contains(&Effect::Move {
            card: DEFY,
            zone: fixtures::HAND,
            seat: 1,
            index: TOP
        }));
        assert!(
            !ctx.effects.contains(&Effect::exhaust(CALM_RUNE)),
            "a cancelled Defy paid nothing"
        );
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert!(ctx.blob.log.contains(&"{card 71} resolves".to_string()));
    }
}
