use super::prelude::{
    activated, asking, done, draw, exhausting_self, gear, named, usable_if, with_candidates,
    ONE_ENERGY,
};
use super::{Card, Flow, Item, Source, Stage, Timing};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const RECYCLES: usize = 3;
const DRAWS: usize = 1;
const PICK: u8 = 1;

pub fn recycle_cost_payable(ctx: &Ctx, source: Source) -> bool {
    ctx.trash_of(ctx.controller(source.card)).len() >= RECYCLES
}

pub fn pay_recycle_cost(ctx: &mut Ctx, seat: u8, picked: &[u32]) -> bool {
    let trash = ctx.trash_of(seat);
    let mut chosen: Vec<u32> = picked
        .iter()
        .copied()
        .filter(|card| trash.contains(card))
        .collect();
    chosen.sort_unstable();
    chosen.dedup();
    if chosen.len() < RECYCLES {
        return false;
    }
    for card in chosen.iter().take(RECYCLES) {
        ctx.recycle_to_bottom(*card);
        ctx.narrate(format!("{{seat {seat}}} recycles {{card {card}}}"));
    }
    true
}

fn trash_cards(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    ctx.trash_of(item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn grab(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 != PICK {
        if !recycle_cost_payable(
            ctx,
            Source {
                card: item.kind.source(),
                ability: 0,
            },
        ) {
            ctx.narrate(format!("{{seat {seat}}} has too little trash to recycle"));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, PICK, RECYCLES as u8, RECYCLES as u8));
    }
    let picked = ctx.picks().to_vec();
    if pay_recycle_cost(ctx, seat, &picked) {
        draw(ctx, seat, DRAWS);
    }
    done()
}

pub static CARD: Card = gear(
    "Garbage Grabber",
    &[],
    &[usable_if(
        named(
            asking(
                with_candidates(
                    exhausting_self(activated(Timing::Sorcery, ONE_ENERGY, &[], grab)),
                    trash_cards,
                ),
                "three cards from your trash to recycle",
            ),
            "draw 1",
        ),
        recycle_cost_payable,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, prompts, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const GRABBER: u32 = 90;
    const TRASHED: [u32; 4] = [91, 92, 93, 94];

    fn grabber(seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Mind".into()],
            exhausted,
            ..fixtures::gear(GRABBER, fixtures::BASE, seat, "Garbage Grabber", 2)
        }
    }

    fn with_trash(count: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(grabber(0, false));
        for id in TRASHED.iter().take(count) {
            fixture
                .table
                .cards
                .push(fixtures::spell(*id, fixtures::TRASH, 0, "Spark", 1, 0));
        }
        fixture.resolve();
        fixture
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
                } if *zone == fixtures::MAIN_DECK => Some(*card),
                _ => None,
            })
            .collect()
    }

    fn pick(ctx: &mut Ctx, card: u32) {
        fixtures::choose(ctx, 0, &format!("{{card {card}}}")).unwrap();
    }

    #[test]
    fn the_grabber_is_an_exhaust_and_one_energy_activation_gated_on_three_cards_of_trash() {
        assert!(std::ptr::eq(script_of("Garbage Grabber").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(ONE_ENERGY));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(
            ability.usable.is_some(),
            "416.3 · the recycle must be payable"
        );
        assert!(ability.candidates.is_some());
        assert_eq!(
            ability.question,
            Some("three cards from your trash to recycle")
        );
        assert!(ability.targets.is_empty());
    }

    #[test]
    fn with_three_or_more_cards_in_the_trash_it_recycles_exactly_three_then_draws_one() {
        let mut fixture = with_trash(4);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        let deck = ctx.table.held(fixtures::MAIN_DECK, 0).count();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == GRABBER && offer.enabled));
        activate::activate(&mut ctx, 0, GRABBER, 0).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.card(GRABBER).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 3, 3));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {GRABBER}}}: choose three cards from your trash to recycle (0 of 3)")
        );
        assert_eq!(
            fixtures::labels(&ctx),
            TRASHED.map(|card| format!("{{card {card}}}"))
        );
        pick(&mut ctx, TRASHED[0]);
        pick(&mut ctx, TRASHED[2]);
        assert_eq!(ctx.hand_of(0).len(), hand, "nothing until the third pick");
        pick(&mut ctx, TRASHED[3]);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(recycled(&ctx), [TRASHED[0], TRASHED[2], TRASHED[3]]);
        assert_eq!(ctx.trash_of(0), [TRASHED[1]]);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(
            ctx.table.held(fixtures::MAIN_DECK, 0).count(),
            deck + 3 - 1,
            "three to the bottom, one off the top"
        );
    }

    #[test]
    fn with_fewer_than_three_cards_in_the_trash_the_activation_is_refused() {
        let mut fixture = with_trash(2);
        let ctx = fixture.ctx();
        assert_eq!(
            activate::legal(&ctx, 0, GRABBER, 0).err(),
            Some(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "416.3 · a cost that can't be completed can't be paid"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == GRABBER));
        drop(ctx);
        let mut spent = with_trash(3);
        spent.table.card_mut(GRABBER).unwrap().exhausted = true;
        let ctx = spent.ctx();
        assert_eq!(
            activate::legal(&ctx, 0, GRABBER, 0).err(),
            Some(Refusal::Exhausted)
        );
    }

    #[test]
    fn the_recycle_takes_only_cards_from_the_payers_trash_and_never_pays_short() {
        let mut fixture = with_trash(3);
        fixture
            .table
            .cards
            .push(fixtures::spell(95, fixtures::TRASH, 1, "Spark", 1, 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!pay_recycle_cost(
            &mut ctx,
            0,
            &[TRASHED[0], TRASHED[1], 95]
        ));
        assert!(
            recycled(&ctx).is_empty(),
            "an opponent's card is not yours to recycle"
        );
        assert!(!pay_recycle_cost(
            &mut ctx,
            0,
            &[TRASHED[0], TRASHED[0], TRASHED[1]]
        ));
        assert!(
            recycled(&ctx).is_empty(),
            "the same card twice is not three"
        );
        assert!(pay_recycle_cost(&mut ctx, 0, &TRASHED[..3]));
        assert_eq!(recycled(&ctx), &TRASHED[..3]);
    }

    #[test]
    #[ignore = "engine gap · non-resource costs: the three recycles belong at the pay stage of the activation, before the ability reaches the chain, not at its resolution"]
    fn the_three_recycles_are_paid_before_the_ability_reaches_the_chain() {
        let mut fixture = with_trash(3);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, GRABBER, 0).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.trash_of(0).is_empty(),
            "the trash was recycled as the cost"
        );
        assert_eq!(recycled(&ctx).len(), RECYCLES);
    }
}
