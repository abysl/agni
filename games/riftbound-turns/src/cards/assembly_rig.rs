use super::ferrous_forerunner::spawn_mech;
use super::prelude::{
    activated, asking, done, exhausting_self, gear, named, usable_if, with_candidates, Location,
};
use super::{Card, Cost, Domain, Flow, Item, Power, Source, Stage, Timing};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const RECYCLES: usize = 1;
pub const PICK: u8 = 1;
pub const ASSEMBLE: u8 = 0;
pub const FUEL: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Fury)],
};

pub fn units_in_trash(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.trash_of(seat)
        .into_iter()
        .filter(|card| ctx.is_unit(*card))
        .collect()
}

pub fn recycle_cost_payable(ctx: &Ctx, source: Source) -> bool {
    units_in_trash(ctx, ctx.controller(source.card)).len() >= RECYCLES
}

pub fn pay_recycle_cost(ctx: &mut Ctx, seat: u8, picked: &[u32]) -> bool {
    let units = units_in_trash(ctx, seat);
    let Some(unit) = picked.iter().copied().find(|card| units.contains(card)) else {
        return false;
    };
    ctx.recycle_to_bottom(unit);
    ctx.narrate(format!("{{seat {seat}}} recycles {{card {unit}}}"));
    true
}

fn trash_units(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    units_in_trash(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn assemble(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 != PICK {
        if !recycle_cost_payable(
            ctx,
            Source {
                card: item.kind.source(),
                ability: ASSEMBLE,
            },
        ) {
            ctx.narrate(format!(
                "{{seat {seat}}} has no unit in the trash to recycle"
            ));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, PICK, RECYCLES as u8, RECYCLES as u8));
    }
    let picked = ctx.picks().to_vec();
    if pay_recycle_cost(ctx, seat, &picked) {
        spawn_mech(ctx, seat, Location::Base(seat));
    }
    done()
}

pub static CARD: Card = gear(
    "Assembly Rig",
    &[],
    &[usable_if(
        named(
            asking(
                with_candidates(
                    exhausting_self(activated(Timing::Sorcery, FUEL, &[], assemble)),
                    trash_units,
                ),
                "a unit from your trash to recycle",
            ),
            "play a 3 Might Mech to your base",
        ),
        recycle_cost_payable,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::ferrous_forerunner::{mech_face_until_token_mech_lands, MECH_MIGHT};
    use crate::cards::rumble_mechanized_menace::{is_mech, MECH_TOKEN};
    use crate::cards::{script_of, SelfCost, Trigger, KIND_UNIT};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, prompts, settle};
    use crate::state::{Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const RIG: u32 = 90;
    const SCRAPPED: [u32; 2] = [91, 92];
    const SPENT_SPELL: u32 = 93;
    const THEIR_SCRAP: u32 = 94;

    fn rig(seat: u8, exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Fury".into()],
            exhausted,
            ..fixtures::gear(RIG, fixtures::BASE, seat, "Assembly Rig", 4)
        }
    }

    fn workshop(scrap: usize, exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rig(0, exhausted));
        for id in SCRAPPED.iter().take(scrap) {
            fixture
                .table
                .cards
                .push(fixtures::unit(*id, fixtures::TRASH, 0, "Scrap", 2));
        }
        fixture.table.cards.push(fixtures::spell(
            SPENT_SPELL,
            fixtures::TRASH,
            0,
            "Spark",
            1,
            0,
        ));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SCRAP, fixtures::TRASH, 1, "Scrap", 2));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(RIG).unwrap(), &CARD));
        fixture
    }

    fn mechs_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == MECH_TOKEN && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .map(|card| card.id)
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
                } if *zone == fixtures::MAIN_DECK => Some(*card),
                _ => None,
            })
            .collect()
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_rig_is_a_paid_exhaust_activation_gated_on_a_unit_in_your_trash() {
        assert!(std::ptr::eq(script_of("Assembly Rig").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[usize::from(ASSEMBLE)];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(FUEL));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert!(
            ability.usable.is_some(),
            "416.3 · the recycle must be payable"
        );
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some("a unit from your trash to recycle"));
        assert_eq!(ability.label, Some("play a 3 Might Mech to your base"));
        assert!(ability.targets.is_empty());
        assert_eq!(FUEL.energy, 1);
        assert_eq!(FUEL.power, [Power::Domain(Domain::Fury)]);
        let face = mech_face_until_token_mech_lands();
        assert_eq!(face.name, MECH_TOKEN);
        assert_eq!(face.kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(face.might, Some(MECH_MIGHT));
        assert!(face.domain.is_empty(), "187.1 · domainless");
    }

    #[test]
    fn with_a_unit_in_the_trash_the_rig_recycles_the_pick_and_plays_an_exhausted_mech_to_the_base()
    {
        let mut fixture = workshop(2, false);
        let mut ctx = fixture.ctx();
        let offers: Vec<(String, bool)> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == RIG)
            .map(|offer| (offer.label, offer.enabled))
            .collect();
        assert_eq!(
            offers,
            [(
                format!(
                    "{{card {RIG}}}: play a 3 Might Mech to your base (1 energy and 1 Fury power, exhaust)"
                ),
                true
            )]
        );
        activate::activate(&mut ctx, 0, RIG, ASSEMBLE).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.card(RIG).unwrap().exhausted, "the exhaust is the cost");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "the spent Fury rune is recycled for the power, a ready rune exhausts for the energy"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(mechs_of(&ctx, 0).is_empty());
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: PICK
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {RIG}}}: choose a unit from your trash to recycle (0 of 1)")
        );
        assert_eq!(
            fixtures::labels(&ctx),
            SCRAPPED.map(|card| format!("{{card {card}}}")),
            "your units in the trash, not the spell and not theirs"
        );
        let next = ctx.table.next_id;
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", SCRAPPED[1])).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(recycled(&ctx), [SCRAPPED[1]]);
        assert_eq!(
            ctx.card(SCRAPPED[1]).unwrap().zone,
            Some(fixtures::MAIN_DECK)
        );
        assert_eq!(ctx.trash_of(0), [SCRAPPED[0], SPENT_SPELL]);
        assert_eq!(mechs_of(&ctx, 0), [next]);
        let mech = next;
        assert!(is_mech(&ctx, mech));
        assert!(!is_mech(&ctx, fixtures::VI));
        assert!(ctx.is_token(mech));
        assert!(ctx.is_unit(mech));
        assert_eq!(ctx.current_might(mech), i32::from(MECH_MIGHT));
        assert_eq!(ctx.location(mech), Some(Location::Base(0)));
        assert_eq!(ctx.controller(mech), 0);
        assert!(ctx.card(mech).unwrap().exhausted, "it enters exhausted");
        assert!(ctx.card(mech).unwrap().domain.is_empty());
        assert!(!ctx.is_temporary(mech));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == mech
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} recycles {{card {}}}", SCRAPPED[1])));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {mech}}} to their base")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_unit_in_your_trash_the_activation_is_refused_and_so_is_a_spent_rig() {
        let mut fixture = workshop(0, false);
        let ctx = fixture.ctx();
        assert!(
            !ctx.trash_of(0).is_empty(),
            "a spell in the trash is not a unit"
        );
        assert_eq!(
            activate::legal(&ctx, 0, RIG, ASSEMBLE).err(),
            Some(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "416.3 · a cost that can't be completed can't be paid"
        );
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == RIG));
        drop(ctx);
        let mut spent = workshop(1, true);
        let ctx = spent.ctx();
        assert_eq!(
            activate::legal(&ctx, 0, RIG, ASSEMBLE).err(),
            Some(Refusal::Exhausted)
        );
        drop(ctx);
        let mut theirs = workshop(1, false);
        let ctx = theirs.ctx();
        assert_eq!(
            activate::legal(&ctx, 1, RIG, ASSEMBLE).err(),
            Some(Refusal::Illegal(Reason::NotYourCard))
        );
    }

    #[test]
    fn the_recycle_takes_only_your_own_units_and_a_unit_gone_from_the_trash_builds_no_mech() {
        let mut fixture = workshop(1, false);
        let mut ctx = fixture.ctx();
        assert!(!pay_recycle_cost(&mut ctx, 0, &[THEIR_SCRAP]));
        assert!(!pay_recycle_cost(&mut ctx, 0, &[SPENT_SPELL]));
        assert!(recycled(&ctx).is_empty());
        assert!(pay_recycle_cost(&mut ctx, 0, &[SCRAPPED[0]]));
        assert_eq!(recycled(&ctx), [SCRAPPED[0]]);
        drop(ctx);
        let mut fixture = workshop(1, false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, RIG, ASSEMBLE).unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.banish(SCRAPPED[0]), "a response banishes the only unit");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "nothing to pick");
        assert!(ctx.blob.chain.is_empty());
        assert!(mechs_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no unit in the trash to recycle".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_mech_is_a_token_that_despawns_when_it_dies() {
        let mut fixture = workshop(1, false);
        let mut ctx = fixture.ctx();
        let mech = spawn_mech(&mut ctx, 1, Location::Base(1)).unwrap();
        assert!(is_mech(&ctx, mech));
        assert_eq!(ctx.controller(mech), 1);
        assert_eq!(ctx.location(mech), Some(Location::Base(1)));
        assert_eq!(ctx.current_might(mech), 3);
        ctx.kill(mech, Cause::Rule);
        assert!(!ctx.on_board(mech));
        assert!(ctx.effects.contains(&Effect::Despawn { card: mech }));
        assert!(
            ctx.card(mech).is_none(),
            "185 · a token leaving the board ceases to exist"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · non-resource costs: the recycle belongs at the pay stage of the activation, before the ability reaches the chain, not at its resolution"]
    fn the_recycle_is_paid_before_the_ability_reaches_the_chain() {
        let mut fixture = workshop(1, false);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, RIG, ASSEMBLE).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            recycled(&ctx),
            [SCRAPPED[0]],
            "the unit was recycled as the cost"
        );
    }

    #[test]
    #[ignore = "engine gap · Token::Mech: engine/ctx.rs has no Mech face, so ferrous_forerunner::mech_face_until_token_mech_lands builds it and spawn_mech replays Ctx::spawn; with the token, spawn_mech is spawn(ctx, owner, Token::Mech, at, MECH_ARRIVES_READY) and cards/mod.rs knows the name"]
    fn the_engine_knows_the_mech_as_a_token_name() {
        assert!(crate::cards::is_token_name(MECH_TOKEN));
    }
}
