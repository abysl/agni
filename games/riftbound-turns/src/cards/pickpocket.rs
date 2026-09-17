use super::prelude::{
    card_target, done, kill, play, spawn_gold, target, unit, GEAR_COSTING_ONE_OR_LESS,
};
use super::{Card, Flow, Item, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::{Ctx, Killed};

const GOLD_ARRIVES_READY: bool = false;

const VICTIM: TargetSpec = target(
    GEAR_COSTING_ONE_OR_LESS,
    0,
    1,
    TargetKind::Card,
    "a gear costing 1 or less",
);

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(gear) = card_target(ctx, item, 0) else {
        return done();
    };
    if kill(ctx, item, gear) != Killed::Yes {
        return done();
    }
    ctx.narrate(format!("{{card {gear}}} dies"));
    spawn_gold(ctx, item.controller, GOLD_ARRIVES_READY);
    done()
}

pub static CARD: Card = unit("Pickpocket", &[], &[play(&[VICTIM], resolve)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude;
    use crate::cards::{Filter, Trigger, KIND_GEAR, TOKEN_GOLD};
    use crate::engine::ctx::{EntryMove, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, play as play_engine, priority, prompts, resume, settle};
    use crate::state::{ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::{Answer, Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const PICKPOCKET: u32 = 90;
    const THEIR_PICKPOCKET: u32 = 91;
    const TRINKET: u32 = 92;
    const LOOT: u32 = 93;
    const ANVIL: u32 = 94;
    const STONE: u32 = 95;
    const GOLD: u32 = 96;

    static WARDING_STONE: Card = prelude::with_replacement(
        prelude::gear("Warding Stone", &[], &[]),
        prelude::replaces(
            |ctx, would, source| ctx.is_gear(would.unit) && would.unit != source.card,
            |ctx, would, _| {
                ctx.bounce(would.unit);
            },
        ),
    );

    fn pickpocket(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            might: Some(3),
            domain: vec!["Mind".into()],
            ..fixtures::card(id, zone, seat, "Pickpocket", "Unit")
        }
    }

    fn alley() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(pickpocket(PICKPOCKET, fixtures::HAND, 0));
        fixture.resolve();
        fixture
    }

    fn with_gear() -> Fixture {
        let mut fixture = alley();
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(LOOT, fixtures::BASE, 1, "Loot", 0));
        fixture
            .table
            .cards
            .push(fixtures::gear(ANVIL, fixtures::BASE, 1, "Anvil", 3));
        fixture.resolve();
        fixture
    }

    fn steal(ctx: &mut Ctx) -> Result<(), Refusal> {
        play_engine::begin(ctx, 0, PICKPOCKET, Origin::Hand, Some(Location::Base(0)))?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, 0)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn answer(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx.blob.prompt.as_ref().map(|held| held.id).unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn golds(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat && card.id != GOLD)
            .map(|card| card.id)
            .collect()
    }

    fn item(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the play trigger is pending")
    }

    fn trashed(ctx: &Ctx, card: u32, seat: u8) -> bool {
        ctx.effects.contains(&Effect::Move {
            card,
            zone: fixtures::TRASH,
            seat,
            index: TOP,
        })
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_pickpocket_is_a_unit_whose_play_trigger_may_choose_one_cheap_gear() {
        assert_eq!(CARD.name, "Pickpocket");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional, "the may lives in the 0..1 target");
        assert!(ability.cost.is_none());
        assert!(ability.condition.is_none());
        assert!(ability.candidates.is_none());
        assert_eq!(ability.targets.len(), 1);
        let spec = &ability.targets[0];
        assert_eq!(
            (spec.min, spec.max),
            (0, 1),
            "352.13: up to one, may be zero"
        );
        assert_eq!(spec.kind, TargetKind::Card);
        assert_eq!(spec.label, "a gear costing 1 or less");
        assert_eq!(
            spec.filter,
            Filter::And(&[Filter::Gear, Filter::EnergyAtMost(1)])
        );
        assert!(std::ptr::eq(
            super::super::script_of("Pickpocket").unwrap(),
            &CARD
        ));
    }

    #[test]
    fn playing_it_offers_every_gear_costing_one_or_less_and_the_chosen_one_dies_for_a_gold() {
        let mut fixture = with_gear();
        fixture.table.cards.push(fixtures::gold(GOLD, 1, false));
        fixture.resolve();
        let action = fixtures::move_action(PICKPOCKET, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        steal(&mut ctx).unwrap();
        assert_eq!(ctx.location(PICKPOCKET), Some(Location::Base(0)));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target {
                item: item(&ctx),
                spec: 0
            })
        );
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {TRINKET}}}"),
                format!("{{card {LOOT}}}"),
                format!("{{card {GOLD}}}"),
                "skip".to_string()
            ],
            "friendly and enemy gear alike, a Gold token included; the Anvil costs 3"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {PICKPOCKET}}}: choose a gear costing 1 or less (0 of 1)")
        );
        assert!(golds(&ctx, 0).is_empty(), "the Gold waits for the chain");
        answer(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == PICKPOCKET
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(LOOT)]);
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(ctx.on_board(LOOT), "the kill waits for the trigger");
        let next = ctx.table.next_id;
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(LOOT));
        assert!(trashed(&ctx, LOOT, 1), "to its own owner's trash");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 1, .. } if *card == LOOT
        )));
        let gold = *golds(&ctx, 0).first().expect("the Gold the theft bought");
        assert_eq!(gold, next);
        assert_eq!(ctx.card(gold).unwrap().kind.as_deref(), Some(KIND_GEAR));
        assert!(ctx.is_token(gold));
        assert_eq!(ctx.location(gold), Some(Location::Base(0)));
        assert!(ctx.card(gold).unwrap().exhausted, "played exhausted");
        assert!(ctx.effects.contains(&Effect::exhaust(gold)));
        assert!(ctx.on_board(TRINKET), "only the chosen gear dies");
        assert!(ctx.on_board(ANVIL));
        assert!(ctx.blob.log.contains(&format!("{{card {LOOT}}} dies")));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
    }

    #[test]
    fn declining_the_choice_kills_nothing_and_mints_no_gold() {
        let mut fixture = with_gear();
        let action = fixtures::move_action(PICKPOCKET, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        steal(&mut ctx).unwrap();
        let offered = prompts::offered(&ctx);
        let skip = offered.len() - 1;
        assert_eq!(offered[skip].answer, Answer::Skip);
        answer(&mut ctx, 0, skip as u16).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "390.3 never removes an up-to-one");
        assert!(ctx.blob.chain[0].targets.is_empty());
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(TRINKET));
        assert!(ctx.on_board(LOOT));
        assert!(golds(&ctx, 0).is_empty());
        assert!(golds(&ctx, 1).is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PICKPOCKET}}} ability resolves")));
    }

    #[test]
    fn with_no_cheap_gear_on_the_board_the_trigger_asks_nothing_and_mints_no_gold() {
        let mut fixture = alley();
        fixture
            .table
            .cards
            .push(fixtures::gear(ANVIL, fixtures::BASE, 0, "Anvil", 3));
        fixture.resolve();
        let action = fixtures::move_action(PICKPOCKET, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        steal(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none(), "nothing worth asking about");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].targets.is_empty());
        assert_eq!(ctx.blob.chain[0].spec_counts, [0]);
        resolve_chain(&mut ctx);
        assert!(ctx.on_board(ANVIL));
        assert!(golds(&ctx, 0).is_empty());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
    }

    #[test]
    fn a_gear_whose_death_is_replaced_leaves_the_pickpocket_without_its_gold() {
        let mut fixture = with_gear();
        fixture
            .table
            .cards
            .push(fixtures::gear(STONE, fixtures::BASE, 0, "Warding Stone", 4));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(STONE, &WARDING_STONE);
        let action = fixtures::move_action(PICKPOCKET, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        steal(&mut ctx).unwrap();
        assert_eq!(labels(&ctx)[0], format!("{{card {TRINKET}}}"));
        answer(&mut ctx, 0, 0).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(TRINKET), "the Stone bounced it instead");
        assert_eq!(ctx.card(TRINKET).unwrap().zone, Some(fixtures::HAND));
        assert!(!trashed(&ctx, TRINKET, 0));
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
        assert!(
            golds(&ctx, 0).is_empty(),
            "the if-you-do reads the kill, not the attempt"
        );
        assert!(!ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {STONE}}} replaces the death of {{card {TRINKET}}}"
        )));
    }

    #[test]
    fn a_gear_that_left_the_board_before_the_trigger_resolves_is_not_killed_and_mints_no_gold() {
        let mut fixture = with_gear();
        let action = fixtures::move_action(PICKPOCKET, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        steal(&mut ctx).unwrap();
        answer(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(TRINKET)]);
        assert!(ctx.bounce(TRINKET));
        resolve_chain(&mut ctx);
        assert_eq!(ctx.card(TRINKET).unwrap().zone, Some(fixtures::HAND));
        assert!(!trashed(&ctx, TRINKET, 0));
        assert!(
            golds(&ctx, 0).is_empty(),
            "356.3.e: the target is gone, the rest of the effect is nothing"
        );
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { .. })));
    }

    #[test]
    fn a_gear_costing_more_than_one_is_no_answer_and_the_other_seat_cannot_play_it() {
        let mut fixture = with_gear();
        let action = fixtures::move_action(PICKPOCKET, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        steal(&mut ctx).unwrap();
        let pending = item(&ctx);
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        let offered = labels(&ctx).len() as u16;
        assert_eq!(offered, 3, "two cheap gear and skip");
        assert_eq!(
            prompts::answer(
                &mut ctx,
                0,
                Pick {
                    prompt,
                    option: offered
                }
            ),
            Err(Refusal::Pick(PickRefusal::NoSuchOption {
                option: 3,
                count: 3
            })),
            "the Anvil is not among the offered gear"
        );
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 })),
            "the theft is the pickpocket's own choice"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, pending, 0, &[ANVIL]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.on_board(ANVIL));
        assert!(golds(&ctx, 0).is_empty());

        let mut theirs = with_gear();
        theirs
            .table
            .cards
            .push(pickpocket(THEIR_PICKPOCKET, fixtures::HAND, 1));
        theirs.resolve();
        let ctx = theirs.ctx();
        let entry = EntryMove {
            card: THEIR_PICKPOCKET,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::NotYourTurn),
            "seat 1 waits for its own action phase"
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
    }
}
