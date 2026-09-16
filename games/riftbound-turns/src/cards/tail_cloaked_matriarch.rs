use super::prelude::{card_target, done, empower, on_empowered, optional, target, unit, Location};
use super::{Card, Cost, Domain, Filter, Flow, Item, Keyword, Power, Stage, TargetKind, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::engine::play;
use crate::state::{Leave, Origin};

pub const EMPOWER: Cost = Cost {
    energy: 2,
    power: &[Power::Domain(Domain::Chaos)],
};
pub const MAX_ENERGY: u8 = 3;
pub const MAX_POWER: u8 = 1;

pub const CHEAP_UNIT_IN_TRASH: Filter = Filter::And(&[
    Filter::Kind(KIND_UNIT),
    Filter::InTrash,
    Filter::Friendly,
    Filter::EnergyAtMost(MAX_ENERGY),
    Filter::PowerAtMost(MAX_POWER),
]);

fn recruit(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !ctx.in_trash(unit) {
        return done();
    }
    let seat = item.controller;
    let me = item.kind.source();
    ctx.narrate(format!(
        "{{card {me}}} · {{seat {seat}}} plays {{card {unit}}} from the trash, ignoring its cost"
    ));
    let _ = play::begin(
        ctx,
        seat,
        unit,
        Origin::Trash {
            leave: Leave::Recycle,
        },
        Some(Location::Base(seat)),
    );
    done()
}

pub static CARD: Card = unit(
    "Tail-Cloaked Matriarch",
    &[Keyword::Empower(EMPOWER)],
    &[
        empower(EMPOWER),
        optional(on_empowered(
            &[target(
                CHEAP_UNIT_IN_TRASH,
                0,
                1,
                TargetKind::Card,
                "a unit in your trash to play to your base",
            )],
            recruit,
        )),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, Timing, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MATRIARCH: u32 = 90;
    const CHEAP: u32 = 91;
    const STEEP: u32 = 92;
    const CHAOS_RUNE: u32 = 46;

    fn matriarch() -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(MATRIARCH, fixtures::BASE, 0, "Tail-Cloaked Matriarch", 4)
        }
    }

    fn trashed(id: u32, name: &str, energy: u8, power: u8) -> CardInfo {
        CardInfo {
            energy: Some(energy),
            power: Some(power),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(id, fixtures::TRASH, 0, name, 2)
        }
    }

    fn den(with_trash: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(matriarch());
        if with_trash {
            fixture
                .table
                .cards
                .push(trashed(CHEAP, "Scuttle Crab", 3, 1));
            fixture.table.cards.push(trashed(STEEP, "Big Lizard", 4, 0));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        fixture
    }

    fn empower_her(ctx: &mut Ctx) {
        activate::activate(ctx, 0, MATRIARCH, 0).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(MATRIARCH));
    }

    #[test]
    fn the_matriarch_prints_empower_and_watches_her_own_empowerment() {
        let fixture = den(true);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MATRIARCH).unwrap(),
            &CARD
        ));
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        let recruit = &CARD.abilities[1];
        assert_eq!(recruit.trigger, Trigger::Empowered);
        assert!(recruit.optional);
        assert_eq!(recruit.targets.len(), 1);
        assert_eq!((recruit.targets[0].min, recruit.targets[0].max), (0, 1));
        assert_eq!(recruit.targets[0].filter, CHEAP_UNIT_IN_TRASH);
    }

    #[test]
    fn empowering_her_pays_two_and_a_chaos_and_a_second_empower_is_refused() {
        let mut fixture = den(false);
        let mut ctx = fixture.ctx();
        let offers: Vec<activate::Offer> = activate::offers(&ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == MATRIARCH)
            .collect();
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {MATRIARCH}}}: empower (2 energy and 1 Chaos power)")
        );
        activate::activate(&mut ctx, 0, MATRIARCH, 0).unwrap();
        assert!(
            ctx.card(CHAOS_RUNE).unwrap().zone != Some(fixtures::RUNE_POOL),
            "the Chaos rune is recycled for the power"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.is_empowered(MATRIARCH));
        assert!(ctx.blob.prompt.is_none(), "an empty trash asks nothing");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.effects.iter().all(|effect| !matches!(
                effect,
                agni_plugin_sdk::decide::Effect::Move { zone, .. } if *zone == fixtures::BASE
            )),
            "nothing was played"
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, MATRIARCH, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
    }

    #[test]
    fn once_empowered_she_offers_the_cheap_trash_unit_and_plays_it_to_the_base_exhausted() {
        let mut fixture = den(true);
        let mut ctx = fixture.ctx();
        empower_her(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CHEAP}}}"), "skip".to_string()],
            "the 4-cost unit is not offered"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHEAP}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == MATRIARCH
        ));
        let runes = ctx.ready_runes_of(0).len();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.location(CHEAP), Some(Location::Base(0)));
        assert!(
            ctx.card(CHEAP).unwrap().exhausted,
            "a played unit enters exhausted"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), runes, "ignoring its cost");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Trash { leave: Leave::Recycle }, .. } if *card == CHEAP
        )));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the Crab's own play trigger fires again"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == CHEAP
        ));
        let hand = ctx.hand_of(0).len();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.hand_of(0).len(), hand + 1, "the Crab draws one");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn declining_the_may_plays_nothing() {
        let mut fixture = den(true);
        let mut ctx = fixture.ctx();
        empower_her(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        settle(&mut ctx).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.card(CHEAP).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.chain.is_empty());
    }
}
