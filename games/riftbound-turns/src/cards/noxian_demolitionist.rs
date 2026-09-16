use super::prelude::{asking, done, kill, on_conquer_me, optional, unit, with_candidates};
use super::{Card, Flow, Item, Stage, KIND_GEAR};
use crate::engine::ctx::{Ctx, Killed};
use crate::state::TargetRef;

pub const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str = "a gear with Energy cost no more than his Might to kill";

pub fn energy_cost_of(ctx: &Ctx, gear: u32) -> i32 {
    ctx.card(gear)
        .and_then(|held| held.energy)
        .map(i32::from)
        .unwrap_or(0)
}

pub fn gear_costing_no_more_than_my_might(ctx: &Ctx, me: u32) -> Vec<u32> {
    let might = ctx.current_might(me);
    ctx.faces_on_board()
        .filter(|card| card.is_kind(KIND_GEAR))
        .map(|card| card.id)
        .filter(|gear| energy_cost_of(ctx, *gear) <= might)
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != STAGE_PICKED {
        return Vec::new();
    }
    gear_costing_no_more_than_my_might(ctx, item.kind.source())
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn demolish(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_PICKED => {
            let offered = gear_costing_no_more_than_my_might(ctx, me);
            let Some(gear) = ctx
                .picks()
                .first()
                .copied()
                .filter(|gear| offered.contains(gear))
            else {
                ctx.narrate(format!("{{card {me}}} demolishes nothing"));
                return done();
            };
            if kill(ctx, item, gear) == Killed::Yes {
                ctx.narrate(format!("{{card {gear}}} dies"));
            }
            done()
        }
        _ => {
            if gear_costing_no_more_than_my_might(ctx, me).is_empty() {
                ctx.narrate(format!(
                    "{{card {me}}} finds no gear cheap enough to demolish"
                ));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Noxian Demolitionist",
    &[],
    &[asking(
        with_candidates(optional(on_conquer_me(&[], demolish)), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::sabotage::tests::pass_until_parked;
    use crate::cards::{script_of, Trigger, Who, TOKEN_GOLD};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const DEMOLITIONIST: u32 = 90;
    const CHEAP: u32 = 91;
    const PRICEY: u32 = 92;
    const MY_GEAR: u32 = 93;
    const GOLD: u32 = 94;

    fn demolitionist(zone: u16, seat: u8, might: u8) -> CardInfo {
        CardInfo {
            energy: Some(2),
            domain: vec!["Body".into()],
            ..fixtures::unit(DEMOLITIONIST, zone, seat, "Noxian Demolitionist", might)
        }
    }

    fn siege(might: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(demolitionist(fixtures::BF1, 0, might));
        fixture
            .table
            .cards
            .push(fixtures::gear(CHEAP, fixtures::BASE, 1, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(PRICEY, fixtures::BASE, 1, "Warhammer", 3));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Lantern", 1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DEMOLITIONIST).unwrap(),
            &CARD
        ));
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn his_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, index: 0 } if source == DEMOLITIONIST))
            .count()
    }

    #[test]
    fn the_script_is_one_may_conquer_trigger_whose_pick_comes_at_resolution() {
        assert!(std::ptr::eq(
            script_of("Noxian Demolitionist").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Conquer(Who::Me));
        assert!(ability.optional);
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
    }

    #[test]
    fn the_candidates_read_printed_energy_against_his_current_might_on_either_side() {
        let mut fixture = siege(1);
        fixture.table.cards.push(fixtures::gold(GOLD, 1, false));
        fixture.table.tokens.push(GOLD);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(energy_cost_of(&ctx, CHEAP), 1);
        assert_eq!(energy_cost_of(&ctx, PRICEY), 3);
        assert_eq!(energy_cost_of(&ctx, GOLD), 0, "a token has no printed cost");
        let mut offered = gear_costing_no_more_than_my_might(&ctx, DEMOLITIONIST);
        offered.sort_unstable();
        assert_eq!(
            offered,
            [CHEAP, MY_GEAR, GOLD],
            "your own gear and their Gold qualify; the 3-cost hammer does not; units never do"
        );
        let item = Item::new(
            7,
            ItemKind::Trigger {
                source: DEMOLITIONIST,
                index: 0,
            },
            0,
            crate::state::Origin::Board,
        );
        assert!(candidates(&ctx, &item, Stage(0)).is_empty());
        assert_eq!(candidates(&ctx, &item, Stage(STAGE_PICKED)).len(), 3);
        crate::cards::prelude::might_this_turn(&mut ctx, &item, DEMOLITIONIST, 2, None);
        assert!(
            gear_costing_no_more_than_my_might(&ctx, DEMOLITIONIST).contains(&PRICEY),
            "Might is read as it stands, buffs included"
        );
    }

    #[test]
    fn conquering_offers_the_cheap_gear_and_the_pick_kills_it() {
        let mut fixture = siege(1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        assert_eq!(his_items(&ctx), 1, "the trigger waits on the chain");
        assert!(ctx.blob.prompt.is_none(), "the pick comes at resolution");
        pass_until_parked(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_PICKED
            })
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                "skip".to_string(),
                format!("{{card {CHEAP}}}"),
                format!("{{card {MY_GEAR}}}")
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHEAP}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(CHEAP));
        assert_eq!(ctx.card(CHEAP).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(PRICEY));
        assert!(ctx.on_board(MY_GEAR));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { card, .. } if *card == CHEAP)));
        assert!(ctx.blob.log.contains(&format!("{{card {CHEAP}}} dies")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_keeps_every_gear_and_with_nothing_cheap_enough_he_never_asks() {
        let mut fixture = siege(1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        pass_until_parked(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(CHEAP) && ctx.on_board(PRICEY) && ctx.on_board(MY_GEAR));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {DEMOLITIONIST}}} demolishes nothing")));
        drop(ctx);

        let mut fixture = siege(0);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(his_items(&ctx), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DEMOLITIONIST}}} finds no gear cheap enough to demolish"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_hold_and_another_units_conquer_are_not_his() {
        let mut fixture = siege(1);
        fixture.blob.set_contested(fixtures::BF1, None);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        settle(&mut ctx).unwrap();
        assert_eq!(his_items(&ctx), 0, "383.4.d · a hold is not a conquer");
        drop(ctx);

        let mut fixture = siege(1);
        fixture.table.card_mut(DEMOLITIONIST).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(his_items(&ctx), 0, "Vi conquers; he sits in the base");
        assert_eq!(
            TOKEN_GOLD, "Gold",
            "the Gold the candidates test spawns is the engine's token name"
        );
    }
}
