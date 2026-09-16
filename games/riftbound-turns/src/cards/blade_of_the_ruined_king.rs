use super::prelude::{
    activated, asking, attach_gear, card_target, done, friendly_units, gear, named, paying_with,
    usable_if, while_attached, with_candidates, with_statics, EQUIP_TARGET,
};
use super::{
    Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, SelfCost, Source, Stage, Timing,
};
use crate::engine::ctx::{Cause, Ctx, Killed};
use crate::state::TargetRef;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Order)],
};

pub const MIGHT_BONUS: i16 = 4;
pub const QUESTION: &str = "a friendly unit to kill for the Equip";
pub const KILL: u8 = 1;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

pub fn kill_candidates(ctx: &Ctx, seat: u8, wearer: Option<u32>) -> Vec<u32> {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| Some(*unit) != wearer)
        .collect()
}

pub fn kill_cost_payable(ctx: &Ctx, source: Source) -> bool {
    kill_candidates(ctx, ctx.controller(source.card), None).len() >= 2
}

fn victims(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    kill_candidates(ctx, item.controller, card_target(ctx, item, 0))
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn ruin(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let blade = item.kind.source();
    let Some(wearer) = card_target(ctx, item, 0) else {
        return done();
    };
    if stage.0 != KILL {
        if victims(ctx, item, stage).is_empty() {
            ctx.narrate(format!(
                "{{card {blade}}} finds no friendly unit to kill for its Equip"
            ));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, KILL, 1, 1));
    }
    let Some(victim) = ctx.picks().first().copied() else {
        return done();
    };
    if ctx.kill(victim, Cause::Cost) != Killed::Yes {
        return done();
    }
    ctx.narrate(format!(
        "{{card {victim}}} is killed for the {{card {blade}}} Equip"
    ));
    attach_gear(ctx, blade, wearer);
    done()
}

pub static RUINED_EQUIP: Ability = usable_if(
    asking(
        with_candidates(
            named(
                paying_with(
                    activated(Timing::Sorcery, EQUIP, &[EQUIP_TARGET], ruin),
                    SelfCost::Free,
                ),
                "equip",
            ),
            victims,
        ),
        QUESTION,
    ),
    kill_cost_payable,
);

pub static CARD: Card = with_statics(
    gear(
        "Blade of the Ruined King",
        &[Keyword::Equip(EQUIP)],
        &[RUINED_EQUIP],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, is_attached, FRIENDLY_UNIT};
    use crate::cards::{script_of, Static, Trigger};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, play as play_engine, priority, prompts, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const BLADE: u32 = 90;
    const WAILER: u32 = 95;
    const SCOUT: u32 = 96;
    const EQUIP_INDEX: u8 = 0;

    fn blade(seat: u8) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::gear(BLADE, fixtures::BASE, seat, "Blade of the Ruined King", 3)
        }
    }

    fn armed(units: &[u32]) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(blade(0));
        for unit in units {
            fixture
                .table
                .cards
                .push(fixtures::unit(*unit, fixtures::BF1, 0, "Wailer", 2));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        {
            let held = fixture.table.card_mut(42).unwrap();
            held.domain = vec!["Order".into()];
            held.name = "Order Rune".into();
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(BLADE).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_is_an_order_equipment_whose_equip_also_kills_a_friendly_unit_for_plus_four() {
        assert!(std::ptr::eq(
            script_of("Blade of the Ruined King").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Blade of the Ruined King");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert!(CARD.is_equipment());
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[0];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets.len(), 1);
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(equip.usable.is_some(), "416.3 · the kill must be payable");
        assert!(equip.candidates.is_some());
        assert_eq!(equip.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(CARD.attached_grants(), [Grant::Might(4)]));
        assert_eq!(MIGHT_BONUS, 4);
    }

    #[test]
    fn equipping_pays_an_order_rune_chooses_the_wearer_then_kills_another_friendly_unit() {
        let mut fixture = armed(&[WAILER, SCOUT]);
        let mut ctx = fixture.ctx();
        assert_eq!(
            kill_candidates(&ctx, 0, Some(fixtures::VI)),
            [WAILER, SCOUT],
            "the wearer is not offered as its own price"
        );
        assert!(kill_cost_payable(
            &ctx,
            Source {
                card: BLADE,
                ability: 0
            }
        ));
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == BLADE && offer.enabled));
        activate::activate(&mut ctx, 0, BLADE, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 95}", "{card 96}", "cancel"]
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.runes_of(0).len(), 3, "the Order rune recycled");
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!is_attached(&ctx, BLADE));
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: KILL
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 95}", "{card 96}"],
            "every friendly unit but the chosen wearer"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {BLADE}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, "{card 95}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(WAILER), "the price is paid");
        assert_eq!(ctx.card(WAILER).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.on_board(SCOUT));
        assert_eq!(attached_to(&ctx, BLADE), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 7);
        assert_eq!(ctx.location(BLADE), Some(Location::Base(0)));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 95} is killed for the {card 90} Equip".to_string()));
        ctx.detach(BLADE);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_second_friendly_unit_the_equip_is_not_offered_and_the_usual_refusals_hold() {
        let mut fixture = armed(&[]);
        let mut ctx = fixture.ctx();
        assert!(!kill_cost_payable(
            &ctx,
            Source {
                card: BLADE,
                ability: 0
            }
        ));
        assert_eq!(
            activate::legal(&ctx, 0, BLADE, EQUIP_INDEX).err(),
            Some(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "416.3 · a cost that can't be completed can't be paid"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == BLADE));
        assert_eq!(
            activate::activate(&mut ctx, 1, BLADE, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);

        let mut fixture = armed(&[WAILER]);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, BLADE, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        assert!(ctx.on_board(WAILER));
        drop(ctx);

        let mut broke = Fixture::enforced();
        broke.table.cards.push(blade(0));
        broke
            .table
            .cards
            .push(fixtures::unit(WAILER, fixtures::BASE, 0, "Wailer", 2));
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, BLADE, EQUIP_INDEX),
            Err(Refusal::NoPowerOf),
            "seat 0 holds no Order rune"
        );
        drop(ctx);

        let mut fixture = armed(&[WAILER, SCOUT]);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, BLADE, EQUIP_INDEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        resolve_chain(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 95}").unwrap();
        assert_eq!(
            activate::activate(&mut ctx, 0, BLADE, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }

    #[test]
    #[ignore = "engine gap · non-resource costs: the kill belongs at the pay stage of the Equip, before the ability reaches the chain (818.1.c.3), not at its resolution"]
    fn the_friendly_unit_is_killed_before_the_equip_reaches_the_chain() {
        let mut fixture = armed(&[WAILER]);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, BLADE, EQUIP_INDEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(!ctx.on_board(WAILER), "the kill was paid as the cost");
    }
}
