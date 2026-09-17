use super::prelude::{adding, gear, RAINBOW};
use super::renata_glasc_chem_baroness::gold_adds_while_paying;
use super::{Adds, Card, Cost, Paying, Power, TOKEN_GOLD};
use crate::engine::ctx::Ctx;

pub const ADDS: Cost = RAINBOW;
pub const ADDS_NEAR_VICTORY: Cost = Cost {
    energy: 1,
    power: &[Power::Rainbow],
};

pub static CARD: Card = adding(gear("Gold", &[], &[]), adds);

pub fn adds_while_paying(ctx: &Ctx, seat: u8, gold: u32) -> Option<Cost> {
    let ready = ctx
        .card(gold)
        .is_some_and(|held| held.name == TOKEN_GOLD && !held.exhausted);
    let mine = ctx.is_token(gold) && ctx.on_board(gold) && ctx.controller(gold) == seat;
    if !(ready && mine) {
        return None;
    }
    Some(if gold_adds_while_paying(ctx, seat, gold).is_some() {
        ADDS_NEAR_VICTORY
    } else {
        ADDS
    })
}

fn adds(ctx: &Ctx, seat: u8, gold: u32, _: Paying) -> Option<Adds> {
    adds_while_paying(ctx, seat, gold).map(Adds::killing)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Domain, Paying, TOKEN_GOLD};
    use crate::engine::cost::{Cost, Need};
    use crate::engine::ctx::{Ctx, EntryMove};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cost, legal, pay, play, prompts, resume, settle};
    use crate::state::{Origin, PromptWhy, FLAG_PAYING};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick};

    const GOLD: u32 = 85;

    fn purse(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::gold(GOLD, 0, exhausted));
        fixture.table.tokens.push(GOLD);
        fixture.table.tokens.sort_unstable();
        fixture.resolve();
        fixture
    }

    fn one_calm_rune(exhausted: bool) -> Fixture {
        let mut fixture = purse(exhausted);
        fixture
            .table
            .cards
            .retain(|card| !card.is_kind("Rune") || card.owner == 1);
        fixture
            .table
            .cards
            .push(fixtures::rune(84, 0, "Calm", false));
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
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx.blob.prompt.as_ref().map_or(0, |prompt| prompt.id);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn recycled(ctx: &Ctx) -> Vec<u32> {
        ctx.effects
            .iter()
            .filter_map(|effect| match effect {
                Effect::Move {
                    card,
                    zone,
                    index: BOTTOM,
                    ..
                } if *zone == fixtures::RUNE_DECK => Some(*card),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_gold_is_a_gear_token_body_and_its_reaction_is_never_an_affordance() {
        let mut fixture = purse(false);
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(script_of(TOKEN_GOLD).unwrap(), &CARD));
        assert!(std::ptr::eq(fixture_script(&ctx), &CARD));
        assert_eq!(CARD.name, TOKEN_GOLD);
        assert!(
            CARD.abilities.is_empty(),
            "416.2: an [Add] resolves at once and never sits on the chain"
        );
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(ctx.is_gear(GOLD));
        assert!(ctx.is_token(GOLD));
        assert!(activate::ability_at(&ctx, GOLD, 0).is_none());
        assert_eq!(
            activate::legal(&ctx, 0, GOLD, 0).err(),
            Some(Refusal::Illegal(Reason::NoSuchAbility)),
            "\"Kill this, exhaust: [Add] rainbow\" is not activated, it is paid with"
        );
        assert!(
            !activate::offers(&ctx, 0)
                .iter()
                .any(|offer| offer.source == GOLD),
            "the strip never offers the Gold as an ability"
        );
        assert_eq!(pay::ready_golds(&ctx, 0), [GOLD], "it is a payment source");
    }

    fn fixture_script(ctx: &Ctx) -> &'static Card {
        ctx.script(GOLD).expect("the token resolves to its script")
    }

    #[test]
    fn a_ready_gold_adds_a_rainbow_for_any_power_and_pays_by_dying_exhausted() {
        let mut fixture = one_calm_rune(false);
        let mut ctx = fixture.ctx();
        let fury = Cost {
            energy: 1,
            power: vec![Need::Domain(Domain::Fury)],
            ..Cost::default()
        };
        let planned = pay::plan(&ctx, 0, &fury).unwrap();
        assert_eq!(
            planned.sources(),
            [GOLD],
            "no Fury rune: the Gold stands in"
        );
        assert_eq!(planned.recycle, Vec::<u32>::new());
        assert_eq!(planned.exhaust, [84], "the Calm rune still pays the energy");
        let rainbow = Cost {
            energy: 0,
            power: vec![Need::Rainbow],
            ..Cost::default()
        };
        assert_eq!(
            pay::plan(&ctx, 0, &rainbow).unwrap().recycle,
            [84],
            "a rune that fits is recycled before a Gold is spent"
        );
        assert!(
            pay::source_is_a_choice(&ctx, 0, &rainbow, Paying::Applied),
            "rune or Gold: the seat is asked"
        );
        assert!(
            pay::source_is_a_choice(&ctx, 0, &fury, Paying::Applied),
            "with no rune that fits, the Gold is still the seat's to spend · asked"
        );
        pay::pay(&mut ctx, 0, &planned);
        assert_eq!(
            ctx.effects,
            [
                Effect::exhaust(GOLD),
                Effect::Despawn { card: GOLD },
                Effect::exhaust(84),
            ],
            "kill this, exhaust — then the runes pay the rest"
        );
        assert!(ctx.card(GOLD).is_none(), "a killed token is despawned");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {GOLD}}} pays 1 power")));
        assert!(pay::ready_golds(&ctx, 0).is_empty());
        assert_eq!(
            pay::plan(&ctx, 0, &fury),
            Err(Refusal::NotEnoughRunes {
                needed: 1,
                ready: 0
            }),
            "the Gold is spent once and for all"
        );
    }

    #[test]
    fn playing_a_spell_asks_which_source_pays_and_the_chosen_gold_dies_in_a_runes_place() {
        let mut fixture = purse(false);
        let mut ctx = fixture.ctx();
        let spark = cost::printed(ctx.card(fixtures::HAND_SPELL).unwrap());
        assert!(pay::source_is_a_choice(&ctx, 0, &spark, Paying::Applied));
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::PayWith { item: 1 }));
        let prompt = ctx.blob.prompt.clone().expect("the payment is a choice");
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert!(prompt.cancel, "a play can still be taken back");
        assert_eq!(
            labels(&ctx),
            [
                format!("kill {{card {GOLD}}}"),
                "recycle a rune".to_string(),
                "cancel".to_string()
            ]
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::PayWith { item: 1 }),
            format!("pay 1 power for {{card {}}} with", fixtures::HAND_SPELL)
        );
        assert_eq!(prompts::offered(&ctx)[0].answer, Answer::Card(GOLD));
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.card(GOLD).is_none(), "the Gold paid with its life");
        assert!(ctx.effects.contains(&Effect::exhaust(GOLD)));
        assert!(ctx.effects.contains(&Effect::Despawn { card: GOLD }));
        assert!(
            recycled(&ctx).is_empty(),
            "no rune was recycled: the Gold covered the power"
        );
        assert_eq!(ctx.blob.chain.len(), 1, "the spell is on the chain");
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    fn answering_recycle_a_rune_leaves_the_gold_alive_and_unpinned() {
        let mut fixture = purse(false);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::PayWith { item: 1 }));
        pick(&mut ctx, 0, 1).unwrap();
        assert!(ctx.card(GOLD).is_some(), "the Gold was not asked to die");
        assert!(!ctx.has_flag(GOLD, FLAG_PAYING));
        assert!(!ctx.card(GOLD).unwrap().exhausted);
        assert_eq!(recycled(&ctx), [fixtures::RUNE_A]);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(pay::ready_golds(&ctx, 0), [GOLD]);
    }

    #[test]
    fn cancelling_the_payment_prompt_takes_the_play_back_unpaid() {
        let mut fixture = purse(false);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::PayWith { item: 1 }));
        assert_eq!(
            prompts::offered(&ctx).last().unwrap().answer,
            Answer::Cancel
        );
        pick(&mut ctx, 0, 2).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty(), "nothing reached the chain");
        assert!(ctx.card(GOLD).is_some(), "the Gold is alive");
        assert!(!ctx.has_flag(GOLD, FLAG_PAYING), "and unpinned");
        assert!(recycled(&ctx).is_empty(), "and no rune was spent");
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "the spell went back to the hand"
        );
    }

    #[test]
    fn an_exhausted_gold_pays_for_nothing_and_is_never_offered_as_a_payment() {
        let mut fixture = one_calm_rune(true);
        let ctx = fixture.ctx();
        assert!(pay::ready_golds(&ctx, 0).is_empty());
        let fury = Cost {
            energy: 0,
            power: vec![Need::Domain(Domain::Fury)],
            ..Cost::default()
        };
        assert_eq!(
            pay::plan(&ctx, 0, &fury),
            Err(Refusal::NoPowerOf),
            "an exhausted Gold cannot exhaust itself again"
        );
        assert!(!pay::source_is_a_choice(&ctx, 0, &fury, Paying::Applied));
        drop(ctx);
        let mut fixture = purse(true);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "with no ready Gold there is nothing to ask"
        );
        assert_eq!(recycled(&ctx), [fixtures::RUNE_A]);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.card(GOLD).is_some());
    }
}
