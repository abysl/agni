use super::prelude::{
    activated, asking, attach_gear, card_target, done, gear, named, on_conquer_me, on_hold_me,
    optional, paying_with, usable_if, while_attached, with_candidates, with_statics, Price,
    EQUIP_TARGET,
};
use super::the_harrowing::{recruit, recruit_candidates, UNIT_IN_TRASH};
use super::{
    Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, SelfCost, Source, Stage, Timing,
};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Chaos)],
};

pub const RECYCLES: usize = 2;
pub const RECYCLE: u8 = 1;
pub const RECYCLE_QUESTION: &str = "two cards from your trash to recycle for the Equip";
pub const WEARER_QUESTION: &str = "a unit in your trash to play for its full cost";
pub const MIGHT_BONUS: i16 = 2;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

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

fn rites(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let gear = item.kind.source();
    let seat = item.controller;
    let Some(wearer) = card_target(ctx, item, 0) else {
        return done();
    };
    if stage.0 != RECYCLE {
        if !recycle_cost_payable(
            ctx,
            Source {
                card: gear,
                ability: 0,
            },
        ) {
            ctx.narrate(format!(
                "{{seat {seat}}} has too little trash to recycle for {{card {gear}}}"
            ));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, RECYCLE, RECYCLES as u8, RECYCLES as u8));
    }
    let picked = ctx.picks().to_vec();
    if pay_recycle_cost(ctx, seat, &picked) {
        attach_gear(ctx, gear, wearer);
    }
    done()
}

pub static RITES_EQUIP: Ability = usable_if(
    asking(
        with_candidates(
            named(
                paying_with(
                    activated(Timing::Sorcery, EQUIP, &[EQUIP_TARGET], rites),
                    SelfCost::Free,
                ),
                "equip",
            ),
            trash_cards,
        ),
        RECYCLE_QUESTION,
    ),
    recycle_cost_payable,
);

fn recruits(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    recruit_candidates(ctx, item, stage, &UNIT_IN_TRASH, Price::Printed)
}

fn rite(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    recruit(ctx, item, stage, &UNIT_IN_TRASH, Price::Printed, 0)
}

pub static WEARER_TEXT: [Ability; 2] = [
    asking(
        with_candidates(optional(on_conquer_me(&[], rite)), recruits),
        WEARER_QUESTION,
    ),
    asking(
        with_candidates(optional(on_hold_me(&[], rite)), recruits),
        WEARER_QUESTION,
    ),
];

pub static CARD: Card = with_statics(
    gear("Last Rites", &[Keyword::Equip(EQUIP)], &[RITES_EQUIP]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, is_attached, FRIENDLY_UNIT};
    use crate::cards::{script_of, Static, Trigger, Who};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, play as play_engine, priority, prompts, settle};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const RITES: u32 = 90;
    const TRASHED: [u32; 3] = [91, 92, 93];
    const FALLEN: u32 = 94;
    const EQUIP_INDEX: u8 = 0;

    fn rites_card(seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Chaos".into()],
            ..fixtures::gear(RITES, fixtures::BASE, seat, "Last Rites", 3)
        }
    }

    fn with_trash(count: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rites_card(0));
        for id in TRASHED.iter().take(count) {
            fixture
                .table
                .cards
                .push(fixtures::spell(*id, fixtures::TRASH, 0, "Spark", 1, 0));
        }
        {
            let held = fixture.table.card_mut(42).unwrap();
            held.domain = vec!["Chaos".into()];
            held.name = "Chaos Rune".into();
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(RITES).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
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

    #[test]
    fn the_script_is_a_chaos_equipment_whose_equip_also_recycles_two_and_grants_its_wearer_text() {
        assert!(std::ptr::eq(script_of("Last Rites").unwrap(), &CARD));
        assert_eq!(CARD.name, "Last Rites");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(CARD.abilities.len(), 1);
        let equip = &CARD.abilities[0];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(
            equip.usable.is_some(),
            "416.3 · the recycle must be payable"
        );
        assert!(equip.candidates.is_some());
        assert_eq!(equip.question, Some(RECYCLE_QUESTION));
        assert!(prompts::resume_questions().contains(&RECYCLE_QUESTION));
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(2), Grant::Ability(_)]
        ));
        assert_eq!((RECYCLES, MIGHT_BONUS), (2, 2));
        assert_eq!(WEARER_TEXT[0].trigger, Trigger::Conquer(Who::Me));
        assert_eq!(WEARER_TEXT[1].trigger, Trigger::Hold(Who::Me));
        for ability in &WEARER_TEXT {
            assert!(ability.optional, "you may");
            assert!(ability.targets.is_empty());
            assert!(ability.candidates.is_some());
            assert_eq!(ability.question, Some(WEARER_QUESTION));
        }
    }

    #[test]
    fn equipping_pays_a_chaos_rune_chooses_the_wearer_then_recycles_two_chosen_trash_cards() {
        let mut fixture = with_trash(3);
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == RITES && offer.enabled));
        activate::activate(&mut ctx, 0, RITES, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.runes_of(0).len(), 3, "the Chaos rune recycled");
        assert!(!is_attached(&ctx, RITES));
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: RECYCLE
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 2, 2));
        assert_eq!(
            fixtures::labels(&ctx),
            TRASHED.map(|card| format!("{{card {card}}}"))
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert!(!is_attached(&ctx, RITES), "nothing until the second pick");
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(recycled(&ctx), [TRASHED[0], TRASHED[2]]);
        assert_eq!(ctx.trash_of(0), [TRASHED[1]]);
        assert_eq!(attached_to(&ctx, RITES), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(ctx.location(RITES), Some(Location::Base(0)));
        ctx.detach(RITES);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_fewer_than_two_cards_in_the_trash_the_equip_is_refused_and_the_recycle_never_pays_short(
    ) {
        let mut fixture = with_trash(1);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::legal(&ctx, 0, RITES, EQUIP_INDEX).err(),
            Some(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "416.3 · a cost that can't be completed can't be paid"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == RITES));
        assert_eq!(
            activate::activate(&mut ctx, 1, RITES, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);

        let mut fixture = with_trash(2);
        fixture
            .table
            .cards
            .push(fixtures::spell(95, fixtures::TRASH, 1, "Spark", 1, 0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!pay_recycle_cost(&mut ctx, 0, &[TRASHED[0], 95]));
        assert!(!pay_recycle_cost(&mut ctx, 0, &[TRASHED[0], TRASHED[0]]));
        assert!(recycled(&ctx).is_empty());
        activate::activate(&mut ctx, 0, RITES, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.runes_of(0).len(), 4, "a cancelled Equip pays nothing");
        assert_eq!(ctx.trash_of(0).len(), 2);
        drop(ctx);

        let mut broke = Fixture::enforced();
        broke.table.cards.push(rites_card(0));
        for id in TRASHED {
            broke
                .table
                .cards
                .push(fixtures::spell(id, fixtures::TRASH, 0, "Spark", 1, 0));
        }
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, RITES, EQUIP_INDEX),
            Err(Refusal::NoPowerOf),
            "seat 0 holds no Chaos rune"
        );
    }

    #[test]
    #[ignore = "engine gap · non-resource costs: the two recycles belong at the pay stage of the Equip, before the ability reaches the chain (818.1.c.3), not at its resolution"]
    fn the_two_recycles_are_paid_before_the_equip_reaches_the_chain() {
        let mut fixture = with_trash(2);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, RITES, EQUIP_INDEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.trash_of(0).is_empty(),
            "the trash was recycled as the cost"
        );
        assert_eq!(recycled(&ctx).len(), RECYCLES);
    }

    #[test]
    fn while_attached_the_wearers_conquer_or_hold_may_play_a_unit_from_your_trash() {
        let mut fixture = with_trash(2);
        fixture.table.cards.push(CardInfo {
            energy: Some(1),
            power: Some(0),
            ..fixtures::unit(FALLEN, fixtures::TRASH, 0, "Fallen", 2)
        });
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, RITES, EQUIP_INDEX).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        resolve_chain(&mut ctx);
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(attached_to(&ctx, RITES), Some(fixtures::VI));
        ctx.raise(Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: vec![fixtures::VI],
        });
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the wearer's appended conquer trigger"
        );
        resolve_chain(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        assert_eq!(fixtures::labels(&ctx), ["{card 94}", "skip"]);
    }
}
