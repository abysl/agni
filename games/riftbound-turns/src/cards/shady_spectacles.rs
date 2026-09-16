use super::keeper_of_masks::become_copy_of;
use super::prelude::{
    activated, asking, attach_gear, card_target, done, friendly_units, gear, named, paying_with,
    while_attached, with_candidates, with_statics, EQUIP_TARGET,
};
use super::{
    Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, SelfCost, Stage, Timing,
};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const EQUIP: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Order)],
};

pub const MIGHT_BONUS: i16 = 0;
pub const COPY: u8 = 1;
pub const QUESTION: &str = "another friendly unit for the equipped unit to become a copy of";

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

pub fn copy_candidates(ctx: &Ctx, seat: u8, wearer: Option<u32>) -> Vec<u32> {
    friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| Some(*unit) != wearer)
        .collect()
}

fn originals(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    copy_candidates(ctx, item.controller, card_target(ctx, item, 0))
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn disguise(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let spectacles = item.kind.source();
    let Some(wearer) = card_target(ctx, item, 0) else {
        return done();
    };
    if stage.0 != COPY {
        attach_gear(ctx, spectacles, wearer);
        if originals(ctx, item, stage).is_empty() {
            ctx.narrate(format!(
                "{{card {spectacles}}} finds no other friendly unit to copy"
            ));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, COPY, 1, 1));
    }
    let offered = originals(ctx, item, stage);
    let Some(original) = ctx
        .picks()
        .first()
        .copied()
        .filter(|unit| offered.contains(&TargetRef::Card(*unit)))
    else {
        return done();
    };
    become_copy_of(ctx, wearer, original);
    done()
}

pub static SHADY_EQUIP: Ability = asking(
    with_candidates(
        named(
            paying_with(
                activated(Timing::Sorcery, EQUIP, &[EQUIP_TARGET], disguise),
                SelfCost::Free,
            ),
            "equip",
        ),
        originals,
    ),
    QUESTION,
);

pub static CARD: Card = with_statics(
    gear("Shady Spectacles", &[Keyword::Equip(EQUIP)], &[SHADY_EQUIP]),
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
    use crate::engine::{activate, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const SPECTACLES: u32 = 90;
    const AHRI: u32 = 95;
    const ORDER_RUNE: u32 = 46;
    const EQUIP_INDEX: u8 = 0;

    fn spectacles(seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Order".into()],
            ..fixtures::gear(SPECTACLES, fixtures::BASE, seat, "Shady Spectacles", 4)
        }
    }

    fn armed(with_ahri: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(spectacles(0));
        if with_ahri {
            fixture
                .table
                .cards
                .push(fixtures::unit(AHRI, fixtures::BF1, 0, "Ahri - Alluring", 4));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(SPECTACLES).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_vi(ctx: &mut Ctx) {
        activate::activate(ctx, 0, SPECTACLES, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        resolve_chain(ctx);
    }

    #[test]
    fn the_script_is_a_one_energy_order_equipment_with_no_might_bonus_and_a_copy_ask_on_the_equip()
    {
        assert!(std::ptr::eq(script_of("Shady Spectacles").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(EQUIP.energy, 1);
        assert!(CARD.is_equipment());
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets.len(), 1);
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(equip.candidates.is_some());
        assert_eq!(equip.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(CARD.attached_grants(), [Grant::Might(0)]));
    }

    #[test]
    fn the_candidates_are_the_controllers_other_units_and_never_the_wearer_or_an_enemy() {
        let mut fixture = armed(true);
        let ctx = fixture.ctx();
        assert_eq!(
            copy_candidates(&ctx, 0, Some(fixtures::VI)),
            [AHRI],
            "another friendly unit: not the wearer"
        );
        assert_eq!(copy_candidates(&ctx, 0, None), [fixtures::VI, AHRI]);
        assert_eq!(
            copy_candidates(&ctx, 1, None),
            [fixtures::SPRITE, fixtures::THEIR_UNIT]
        );
        assert!(copy_candidates(&ctx, 0, Some(fixtures::VI))
            .iter()
            .all(|unit| unit != &fixtures::THEIR_UNIT));
    }

    #[test]
    fn equipping_pays_one_energy_and_an_order_rune_attaches_and_asks_for_the_original() {
        let mut fixture = armed(true);
        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, SPECTACLES), Some(fixtures::VI));
        assert!(
            ctx.card(ORDER_RUNE).unwrap().zone != Some(fixtures::RUNE_POOL),
            "the Order rune is recycled for the power"
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 1,
            "167 · the Order rune is exhausted for the energy, then recycled for the power"
        );
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            3,
            "+0 · the spectacles add no Might of their own"
        );
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: COPY
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {AHRI}}}")],
            "the wearer and the enemy's units are not offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {SPECTACLES}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {AHRI}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} would become a copy of {{card {AHRI}}} (4 Might) · the engine owes Ctx::become_copy",
            fixtures::VI
        )));
        assert_eq!(attached_to(&ctx, SPECTACLES), Some(fixtures::VI));
        assert_eq!(ctx.location(SPECTACLES), Some(Location::Base(0)));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_other_friendly_unit_the_equip_attaches_and_asks_nothing() {
        let mut fixture = armed(false);
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        assert_eq!(attached_to(&ctx, SPECTACLES), Some(fixtures::VI));
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} finds no other friendly unit to copy".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_a_missing_order_rune_an_enemy_wearer_and_a_worn_pair_are_refused() {
        let mut fixture = armed(true);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, SPECTACLES, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, SPECTACLES, EQUIP_INDEX).unwrap();
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, SPECTACLES));
        assert_eq!(ctx.runes_of(0).len(), 5, "a cancelled Equip pays nothing");
        drop(ctx);

        let mut broke = armed(true);
        broke.table.cards.retain(|card| card.id != ORDER_RUNE);
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, SPECTACLES, EQUIP_INDEX),
            Err(Refusal::NoPowerOf)
        );
        drop(ctx);

        let mut fixture = armed(true);
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {AHRI}}}")).unwrap();
        assert_eq!(
            activate::activate(&mut ctx, 0, SPECTACLES, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }

    #[test]
    #[ignore = "engine gap · tokens and copies: Ctx::become_copy is owed (477.1.b · the wearer takes the chosen unit's name, script binding and Might for as long as the spectacles stay attached, reverting on detach), so keeper_of_masks::become_copy_of narrates the promise and changes nothing"]
    fn the_wearer_becomes_a_copy_of_the_chosen_unit_until_the_spectacles_come_off() {
        let mut fixture = armed(true);
        let mut ctx = fixture.ctx();
        equip_vi(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {AHRI}}}")).unwrap();
        assert_eq!(ctx.card(fixtures::VI).unwrap().name, "Ahri - Alluring");
        assert!(std::ptr::eq(
            ctx.script(fixtures::VI).unwrap(),
            script_of("Ahri - Alluring").unwrap()
        ));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        ctx.detach(SPECTACLES);
        assert_eq!(ctx.card(fixtures::VI).unwrap().name, "Vi");
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }
}
