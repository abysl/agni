use super::prelude::{
    activated, banish, banish_by, done, equip, gear, named, trigger_subject, while_attached,
    with_statics, Location,
};
use super::{Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, Timing};
use crate::engine::ctx::Ctx;
use crate::engine::play as play_engine;
use crate::state::Origin;

pub const EQUIP: Cost = Cost {
    energy: 1,
    power: &[Power::Domain(Domain::Mind)],
};

pub const RECLAIM: Cost = Cost {
    energy: 3,
    power: &[Power::Domain(Domain::Mind)],
};

pub const MIGHT_BONUS: i16 = 2;
pub const RECLAIM_ABILITY: u8 = 1;

pub static EFFECT_TEXT: &[Grant] = &[
    Grant::Keyword(Keyword::Deathknell),
    Grant::Might(MIGHT_BONUS),
];

pub fn units_banished_with(_: &Ctx, _: u32) -> Vec<u32> {
    Vec::new()
}

pub fn banish_me_as_the_cost_until_self_cost_banish_self_pays_it_at_finalization(
    ctx: &mut Ctx,
    me: u32,
) -> bool {
    ctx.on_board(me) && banish(ctx, me)
}

fn reclaim(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    let banished = units_banished_with(ctx, me);
    if !banish_me_as_the_cost_until_self_cost_banish_self_pays_it_at_finalization(ctx, me) {
        ctx.narrate(format!(
            "{{card {me}}} is no longer on the board · nothing is played"
        ));
        return done();
    }
    if banished.is_empty() {
        ctx.narrate(format!("no units were banished with {{card {me}}}"));
        return done();
    }
    for unit in banished {
        if !ctx.in_banishment(unit) {
            continue;
        }
        let _ = play_engine::begin(
            ctx,
            seat,
            unit,
            Origin::Banishment,
            Some(Location::Base(seat)),
        );
    }
    done()
}

pub fn the_wearers_deathknell_banishes_it(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let drive = item.kind.source();
    let Some(me) = trigger_subject(item) else {
        return done();
    };
    if banish_by(ctx, me, item.controller) {
        ctx.narrate(format!("{{card {me}}} is banished with {{card {drive}}}"));
    }
    done()
}

pub static CARD: Card = with_statics(
    gear(
        "The Zero Drive",
        &[Keyword::Equip(EQUIP)],
        &[
            equip(EQUIP),
            named(
                activated(Timing::Sorcery, RECLAIM, &[], reclaim),
                "banish this and play every unit banished with it",
            ),
        ],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::cull::tests::{equipment, GEAR};
    use crate::cards::prelude::attach_gear;
    use crate::cards::{script_of, SelfCost, Static, Trigger};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, settle};
    use crate::state::{ChainItem, ItemKind, Origin, TargetRef};
    use crate::Refusal;

    const MIND_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "The Zero Drive", 3, "Mind"));
        for rune in MIND_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn the_deathknell(dead: u32) -> ChainItem {
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: GEAR,
                index: 2,
            },
            0,
            Origin::Board,
        );
        item.subject = Some(TargetRef::Card(dead));
        item
    }

    #[test]
    fn the_script_is_a_one_energy_mind_equipment_with_two_might_deathknell_for_the_wearer_and_a_reclaim(
    ) {
        assert!(std::ptr::eq(script_of("The Zero Drive").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(EQUIP.energy, 1);
        assert!(
            !CARD.has_keyword(Keyword::Deathknell),
            "the Deathknell is the wearer's"
        );
        assert_eq!(CARD.abilities.len(), 2);
        let reclaim = &CARD.abilities[usize::from(RECLAIM_ABILITY)];
        assert_eq!(reclaim.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(reclaim.cost, Some(RECLAIM));
        assert_eq!(
            reclaim.self_cost,
            SelfCost::Auto,
            "no SelfCost banishes the source · the script pays it on resolution"
        );
        assert!(reclaim.targets.is_empty());
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Keyword(Keyword::Deathknell), Grant::Might(2)]
        ));
    }

    #[test]
    fn the_reclaim_costs_three_and_a_mind_rune_banishes_the_drive_and_plays_nothing_today() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == GEAR && offer.index == RECLAIM_ABILITY)
            .unwrap();
        assert!(offer.enabled);
        assert_eq!(
            offer.label,
            "{card 90}: banish this and play every unit banished with it (3 energy and 1 Mind power)"
        );
        assert!(units_banished_with(&ctx, GEAR).is_empty());
        let runes = ctx.runes_of(0).len();
        let ready = ctx.ready_runes_of(0).len();
        activate::activate(&mut ctx, 0, GEAR, RECLAIM_ABILITY).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "one Mind rune recycled for the power"
        );
        assert!(
            ctx.ready_runes_of(0).len() <= ready - 3,
            "three exhausted for the energy"
        );
        assert!(
            ctx.on_board(GEAR),
            "383.3.b · not banished until it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_banishment(GEAR));
        assert!(ctx
            .blob
            .log
            .contains(&"no units were banished with {card 90}".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_reclaim_is_refused_while_attached_without_a_mind_rune_and_off_the_board() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Deathknell));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(
            activate::activate(&mut ctx, 0, GEAR, RECLAIM_ABILITY),
            Err(Refusal::Illegal(Reason::Attached)),
            "use only if unattached"
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, GEAR, RECLAIM_ABILITY),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);
        let mut broke = armed();
        broke
            .table
            .cards
            .retain(|card| !MIND_RUNES.contains(&card.id));
        broke.resolve();
        let mut ctx = broke.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, GEAR, RECLAIM_ABILITY),
            Err(Refusal::NoPowerOf),
            "three ready runes for the energy but no Mind power"
        );
        assert!(ctx.banish(GEAR));
        assert!(
            !banish_me_as_the_cost_until_self_cost_banish_self_pays_it_at_finalization(
                &mut ctx, GEAR
            ),
            "a drive already off the board pays nothing"
        );
    }

    #[test]
    fn the_wearers_deathknell_run_banishes_the_dead_wearer_and_names_its_death_alone() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.kill(fixtures::VI, Cause::Combat), Killed::Yes);
        assert!(ctx.in_trash(fixtures::VI));
        assert_eq!(
            the_wearers_deathknell_banishes_it(&mut ctx, &the_deathknell(fixtures::VI), Stage(0)),
            Flow::Done
        );
        assert!(ctx.in_banishment(fixtures::VI));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} is banished with {card 90}".to_string()));
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty(),
            "today no trigger fires for the wearer's death"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · missing trigger subject and a banished-with link: no SelfCost::BanishSelf, and the engine keeps no record of which cards a source banished (units_banished_with reads nothing); the wiring is deathknell(&[], the_wearers_deathknell_banishes_it) granted through Grant::Ability, resolving as a Granted item whose source is the wearer and whose lender is the drive, and the reclaim replaying the linked units from Banishment ignoring their costs"]
    fn the_wearers_death_banishes_it_and_the_reclaim_replays_it_free_through_the_engine() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(ctx.kill(fixtures::VI, Cause::Combat), Killed::Yes);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.in_banishment(fixtures::VI));
        assert_eq!(units_banished_with(&ctx, GEAR), [fixtures::VI]);
        activate::activate(&mut ctx, 0, GEAR, RECLAIM_ABILITY).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.in_banishment(GEAR));
        assert!(ctx.on_board(fixtures::VI));
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
    }
}
