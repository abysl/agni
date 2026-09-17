use super::prelude::{
    asking, choosable, done, empower, is_empowered, kill, on_attack, unit, when, when_empowered,
    with_candidates, with_statics,
};
use super::{Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, Static};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const EMPOWER: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Order), Power::Domain(Domain::Order)],
};
pub const ASSAULT: u8 = 2;
pub const QUESTION: &str = "an enemy unit here with less Might than me to kill";
const STAGE_PICKED: u8 = 1;

pub fn weaker_enemies_here(ctx: &Ctx, me: u32) -> Vec<u32> {
    let Some(here) = ctx.location(me) else {
        return Vec::new();
    };
    let seat = ctx.controller(me);
    let might = ctx.current_might(me);
    ctx.units_at(here)
        .into_iter()
        .filter(|unit| ctx.controller(*unit) != seat && ctx.current_might(*unit) < might)
        .collect()
}

fn choosable_victims(ctx: &Ctx, item: &Item) -> Vec<u32> {
    weaker_enemies_here(ctx, item.kind.source())
        .into_iter()
        .filter(|unit| choosable(ctx, item, *unit))
        .collect()
}

fn victims(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 != STAGE_PICKED {
        return Vec::new();
    }
    choosable_victims(ctx, item)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn execute(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    match stage.0 {
        STAGE_PICKED => {
            let Some(unit) = ctx
                .picks()
                .first()
                .copied()
                .filter(|unit| choosable_victims(ctx, item).contains(unit))
            else {
                return done();
            };
            ctx.narrate(format!("{{card {me}}} kills {{card {unit}}}"));
            kill(ctx, item, unit);
            done()
        }
        _ => {
            if choosable_victims(ctx, item).is_empty() {
                ctx.narrate(format!(
                    "{{card {me}}} kills nothing · no enemy unit here has less Might that she can choose"
                ));
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 1, 1))
        }
    }
}

pub static CARD: Card = with_statics(
    unit(
        "Ambessa, Respected and Feared",
        &[Keyword::Empower(EMPOWER)],
        &[
            empower(EMPOWER),
            asking(
                with_candidates(when(on_attack(&[], execute), when_empowered), victims),
                QUESTION,
            ),
        ],
    ),
    &[Static::While(
        is_empowered,
        &[Grant::Keyword(Keyword::Assault(ASSAULT))],
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority, prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const AMBESSA: u32 = 90;
    const BRUTE: u32 = 91;
    const EQUAL: u32 = 92;
    const GIANT: u32 = 93;
    const RUNT: u32 = 94;
    const MIGHT: u8 = 5;
    const ORDER_RUNES: [u32; 2] = [100, 101];

    fn ambessa() -> CardInfo {
        CardInfo {
            energy: Some(5),
            domain: vec!["Order".into()],
            ..fixtures::unit(
                AMBESSA,
                fixtures::BF1,
                0,
                "Ambessa, Respected and Feared",
                MIGHT,
            )
        }
    }

    fn war_council(when_empowered: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ambessa());
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 6));
        fixture
            .table
            .cards
            .push(fixtures::unit(EQUAL, fixtures::BF1, 1, "Equal", 7));
        fixture
            .table
            .cards
            .push(fixtures::unit(GIANT, fixtures::BF1, 1, "Giant", 9));
        fixture
            .table
            .cards
            .push(fixtures::unit(RUNT, fixtures::BF1, 0, "Runt", 1));
        for rune in ORDER_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Order", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(AMBESSA).unwrap(),
            &CARD
        ));
        if when_empowered {
            let mut ctx = fixture.ctx();
            activate::activate(&mut ctx, 0, AMBESSA, 0).unwrap();
            fixtures::settle_rune_payments(&mut ctx, 0).unwrap();
            priority::pass(&mut ctx, 0).unwrap();
            priority::pass(&mut ctx, 1).unwrap();
            assert!(ctx.is_empowered(AMBESSA));
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        fixture
    }

    fn attacks(ctx: &mut Ctx) {
        assert!(ctx.mark_attacker(AMBESSA));
        settle(ctx).unwrap();
        fixtures::settle_rune_payments(ctx, 0).unwrap();
    }

    #[test]
    fn the_script_prints_empower_with_an_empowered_assault_and_an_empowered_gated_attack_trigger() {
        assert!(std::ptr::eq(
            script_of("Ambessa, Respected and Feared").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert!(
            !CARD.has_keyword(Keyword::Assault(0)),
            "the Assault is the Empowered ability's, not printed as her own"
        );
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(
            CARD.statics[0],
            Static::While(_, [Grant::Keyword(Keyword::Assault(ASSAULT))])
        ));
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        let attack = &CARD.abilities[1];
        assert_eq!(attack.trigger, Trigger::Attacks(Who::Me));
        assert!(!attack.optional);
        assert!(attack.condition.is_some());
        assert!(attack.targets.is_empty());
        assert!(attack.candidates.is_some());
        assert_eq!(attack.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(ASSAULT, 2);
    }

    #[test]
    fn empowered_she_has_assault_two_while_attacking_and_the_lesser_enemies_are_read_off_that() {
        let mut fixture = war_council(true);
        let mut ctx = fixture.ctx();
        assert!(ctx.has_keyword(AMBESSA, Keyword::Assault(0)));
        assert_eq!(
            ctx.current_might(AMBESSA),
            5,
            "807.1.c · only as an attacker"
        );
        let mut weaker = weaker_enemies_here(&ctx, AMBESSA);
        weaker.sort_unstable();
        assert_eq!(weaker, [fixtures::THEIR_UNIT], "2 against her 5");
        assert!(ctx.mark_attacker(AMBESSA));
        assert_eq!(ctx.current_might(AMBESSA), 7);
        let mut weaker = weaker_enemies_here(&ctx, AMBESSA);
        weaker.sort_unstable();
        assert_eq!(
            weaker,
            [fixtures::THEIR_UNIT, BRUTE],
            "the Brute at 6 is under her 7 · Equal at 7 is not, the Runt is hers"
        );
        drop(ctx);

        let mut plain = war_council(false);
        let ctx = plain.ctx();
        assert!(!ctx.has_keyword(AMBESSA, Keyword::Assault(0)));
    }

    #[test]
    fn empowered_and_attacking_she_asks_which_lesser_enemy_here_to_kill_and_kills_it() {
        let mut fixture = war_council(true);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == AMBESSA
        ));
        assert!(
            ctx.blob.prompt.is_none(),
            "nothing to choose as it triggers"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                stage: STAGE_PICKED,
                ..
            })
        ));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {AMBESSA}}}: choose {QUESTION} (0 of 1)")
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {BRUTE}}}")
            ],
            "the two enemies under her 7 · no skip"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(BRUTE));
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.on_board(EQUAL));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {AMBESSA}}} kills {{card {BRUTE}}}")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_shrouded_lesser_enemy_cannot_be_chosen_and_when_every_lesser_enemy_hides_she_kills_nothing(
    ) {
        let mut fixture = war_council(true);
        let mut ctx = fixture.ctx();
        assert!(ctx.shroud(BRUTE));
        attacks(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::THEIR_UNIT)],
            "the shrouded Brute is no choice"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(ctx.on_board(BRUTE));
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        drop(ctx);

        let mut hidden = war_council(true);
        let mut ctx = hidden.ctx();
        assert!(ctx.shroud(BRUTE));
        assert!(ctx.shroud(fixtures::THEIR_UNIT));
        attacks(&mut ctx);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "no empty mandatory prompt");
        assert!(ctx.on_board(BRUTE) && ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {AMBESSA}}} kills nothing · no enemy unit here has less Might that she can choose"
        )));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn not_empowered_she_triggers_nothing_and_without_a_lesser_enemy_here_she_kills_nothing() {
        let mut plain = war_council(false);
        let mut ctx = plain.ctx();
        attacks(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty(),
            "828.1.c · no Empowered, no ability"
        );
        assert!(ctx.blob.prompt.is_none());
        drop(ctx);

        let mut outmatched = war_council(true);
        outmatched
            .table
            .cards
            .retain(|card| ![fixtures::THEIR_UNIT, BRUTE].contains(&card.id));
        outmatched.resolve();
        let mut ctx = outmatched.ctx();
        attacks(&mut ctx);
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the trigger still goes on the chain"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none(), "no one to choose");
        assert!(ctx.on_board(EQUAL) && ctx.on_board(GIANT) && ctx.on_board(RUNT));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {AMBESSA}}} kills nothing · no enemy unit here has less Might that she can choose"
        )));
        drop(ctx);

        let mut fixture = war_council(true);
        let mut ctx = fixture.ctx();
        attacks(&mut ctx);
        let pump = ctx.blob.chain[0].clone();
        crate::cards::prelude::might_this_turn(&mut ctx, &pump, BRUTE, 3, None);
        assert_eq!(ctx.current_might(BRUTE), 9);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::THEIR_UNIT)],
            "the Brute that grew to 9 is no lesser unit as she chooses"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::THEIR_UNIT)).unwrap();
        assert!(ctx.on_board(BRUTE));
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert!(ctx.blob.chain.is_empty());
    }
}
