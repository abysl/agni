use crate::cards::{Card, Grant, Scope, Static};
use crate::engine::attach;
use crate::engine::ctx::{Ctx, Location};
use agni_plugin_sdk::table::CardInfo;

pub fn in_play(ctx: &Ctx, card: u32) -> bool {
    ctx.card(card).is_some_and(|held| face_active(ctx, held))
}

fn face_active(ctx: &Ctx, held: &CardInfo) -> bool {
    !held.is_hidden()
        && ctx.face_in_play(held)
        && !ctx.is_pending_play(held.id)
        && !ctx.is_facedown(held.id)
        && !attach::is_attached(ctx, held.id)
}

fn push_active(ctx: &Ctx, grants: &mut Vec<(u32, Grant)>, card: u32, source: u32, held: &[Grant]) {
    for grant in held {
        let active = match grant {
            Grant::MightIf(when, _) => when(ctx, card, source),
            Grant::Keyword(_)
            | Grant::Static(_)
            | Grant::Might(_)
            | Grant::Ability(_)
            | Grant::Copied(_)
            | Grant::Mirror(_)
            | Grant::Borrowed(_) => true,
        };
        if active {
            grants.push((source, *grant));
        }
    }
}

pub fn defends_alone(ctx: &Ctx, _source: u32, unit: u32) -> bool {
    ctx.is_defender(unit) && ctx.alone_at(unit)
}

pub fn level_active(ctx: &Ctx, card: u32, level: u8) -> bool {
    ctx.xp(ctx.controller(card)) >= i32::from(level)
}

fn own_grants(ctx: &Ctx, grants: &mut Vec<(u32, Grant)>, card: u32) {
    let Some(script) = ctx.script(card) else {
        return;
    };
    for held in script.statics {
        conditional(ctx, grants, card, card, held);
    }
}

fn conditional(ctx: &Ctx, grants: &mut Vec<(u32, Grant)>, card: u32, source: u32, held: &Static) {
    let active = match held {
        Static::Level(level, granted) => level_active(ctx, card, *level).then_some(*granted),
        Static::While(applies, granted) => applies(ctx, card).then_some(*granted),
        _ => None,
    };
    if let Some(granted) = active {
        push_active(ctx, grants, card, source, granted);
    }
}

fn attachment_grants(ctx: &Ctx, grants: &mut Vec<(u32, Grant)>, card: u32) {
    for gear in attach::attachments_of(ctx, card) {
        for grant in attach::grants_of(ctx, gear) {
            match grant {
                Grant::MightIf(when, _) if when(ctx, card, gear) => grants.push((gear, *grant)),
                Grant::Static(held) => {
                    grants.push((gear, *grant));
                    conditional(ctx, grants, card, gear, held);
                }
                Grant::Keyword(_)
                | Grant::Might(_)
                | Grant::MightIf(_, _)
                | Grant::Ability(_)
                | Grant::Copied(_)
                | Grant::Mirror(_)
                | Grant::Borrowed(_) => {}
            }
        }
    }
}

fn friendly_seat_of(ctx: &Ctx, source: u32) -> Option<u8> {
    if ctx.is_battlefield_card(source) {
        let zone = ctx.card(source).and_then(|held| held.zone)?;
        return ctx.blob.holder(zone);
    }
    Some(ctx.controller(source))
}

fn friendly_to(ctx: &Ctx, source: u32, card: u32) -> bool {
    friendly_seat_of(ctx, source) == Some(ctx.controller(card))
}

fn in_scope(ctx: &Ctx, scope: Scope, source: u32, card: u32) -> bool {
    match scope {
        Scope::FriendlyUnits => ctx.is_unit(card) && friendly_to(ctx, source, card),
        Scope::UnitsHere => {
            let here = ctx.location(source);
            ctx.is_unit(card) && here.is_some() && ctx.location(card) == here
        }
        Scope::FriendlyTokens => ctx.is_token(card) && friendly_to(ctx, source, card),
        Scope::Legend => ctx.is_legend(card) && friendly_to(ctx, source, card),
    }
}

fn aura_sources<'c>(ctx: &'c Ctx) -> impl Iterator<Item = &'c CardInfo> {
    let known = ctx
        .scripts
        .aura_sources()
        .iter()
        .filter_map(|id| ctx.card(*id));
    let spawned = ctx
        .table
        .cards
        .iter()
        .filter(move |held| ctx.spawned > 0 && held.id >= ctx.origin.next_id)
        .filter(|held| ctx.script(held.id).is_some_and(|script| script.has_aura()));
    known.chain(spawned)
}

fn project(ctx: &Ctx, grants: &mut Vec<(u32, Grant)>, card: u32, source: u32, aura: &Static) {
    let Static::Aura {
        scope,
        when,
        grants: granted,
    } = aura
    else {
        return;
    };
    if in_scope(ctx, *scope, source, card) && when(ctx, source, card) {
        push_active(ctx, grants, card, source, granted);
    }
}

fn aura_grants(ctx: &Ctx, grants: &mut Vec<(u32, Grant)>, card: u32) {
    for held in aura_sources(ctx) {
        if !face_active(ctx, held) {
            continue;
        }
        let Some(script) = ctx.script(held.id) else {
            continue;
        };
        for own in script.statics {
            project(ctx, grants, card, held.id, own);
        }
    }
    for row in &ctx.blob.cards {
        let Some(wearer) = row.attached_to else {
            continue;
        };
        if !ctx.card(wearer).is_some_and(|held| face_active(ctx, held)) {
            continue;
        }
        for grant in attach::grants_of(ctx, row.id) {
            if let Grant::Static(aura) = grant {
                project(ctx, grants, card, wearer, aura);
            }
        }
    }
}

pub fn sourced_grants_on(ctx: &Ctx, card: u32) -> Vec<(u32, Grant)> {
    let mut grants = Vec::new();
    if !in_play(ctx, card) {
        return grants;
    }
    own_grants(ctx, &mut grants, card);
    attachment_grants(ctx, &mut grants, card);
    if ctx.is_unit(card) || ctx.is_token(card) || ctx.is_legend(card) {
        aura_grants(ctx, &mut grants, card);
    }
    grants
}

fn play_location_sources<'c>(ctx: &'c Ctx) -> impl Iterator<Item = &'c CardInfo> {
    let known = ctx
        .scripts
        .play_location_sources()
        .iter()
        .filter_map(|id| ctx.card(*id));
    let spawned = ctx
        .table
        .cards
        .iter()
        .filter(move |held| ctx.spawned > 0 && held.id >= ctx.origin.next_id)
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| script.grants_play_locations())
        });
    known.chain(spawned)
}

fn base_lock_sources<'c>(ctx: &'c Ctx) -> impl Iterator<Item = &'c CardInfo> {
    let known = ctx
        .scripts
        .base_lock_sources()
        .iter()
        .filter_map(|id| ctx.card(*id));
    let spawned = ctx
        .table
        .cards
        .iter()
        .filter(move |held| ctx.spawned > 0 && held.id >= ctx.origin.next_id)
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| script.mentions_static(Static::OpponentsPlayUnitsOnlyToBase))
        });
    known.chain(spawned)
}

pub fn has_active_static(ctx: &Ctx, card: u32, wanted: Static) -> bool {
    in_play(ctx, card)
        && (ctx
            .script(card)
            .is_some_and(|script| script.has_static(wanted))
            || ctx
                .projected_statics(card)
                .iter()
                .any(|held| held.same_kind(wanted)))
}

pub fn units_only_to_base(ctx: &Ctx, seat: u8) -> bool {
    base_lock_sources(ctx).any(|held| {
        ctx.controller(held.id) != seat
            && has_active_static(ctx, held.id, Static::OpponentsPlayUnitsOnlyToBase)
    })
}

fn push_script(scripts: &mut Vec<&'static Card>, script: &'static Card) {
    if !scripts.iter().any(|held| std::ptr::eq(*held, script)) {
        scripts.push(script);
    }
}

pub fn granted_play_locations(ctx: &Ctx, seat: u8, card: u32) -> Vec<Location> {
    let mut scripts: Vec<&'static Card> = Vec::new();
    if let Some(script) = ctx.script(card) {
        if script.grants_play_locations() {
            push_script(&mut scripts, script);
        }
    }
    for held in play_location_sources(ctx) {
        if held.id == card || !face_active(ctx, held) {
            continue;
        }
        if let Some(script) = ctx.script(held.id) {
            push_script(&mut scripts, script);
        }
    }
    let mut locations = Vec::new();
    for script in scripts {
        for grant in script.play_location_grants() {
            for at in grant(ctx, seat, card) {
                if !locations.contains(&at) {
                    locations.push(at);
                }
            }
        }
    }
    locations
}

pub fn enters_ready(ctx: &Ctx, card: u32) -> bool {
    let own = ctx.script(card).is_some_and(|script| {
        script
            .statics
            .iter()
            .any(|held| matches!(held, Static::EntersReady(applies) if applies(ctx, card)))
    });
    own || grants_on(ctx, card).into_iter().any(
        |grant| matches!(grant, Grant::Static(Static::EntersReady(applies)) if applies(ctx, card)),
    )
}

pub fn grants_on(ctx: &Ctx, card: u32) -> Vec<Grant> {
    sourced_grants_on(ctx, card)
        .into_iter()
        .map(|(_, grant)| grant)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, equip, while_attached, CHAOS};
    use crate::cards::{Card, Keyword};
    use crate::engine::ctx::{Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{attach, march};
    use crate::state::FLAG_DEFENDER;
    use crate::Refusal;

    static LEVELED: Card = prelude::with_statics(
        prelude::unit("Leveled", &[], &[]),
        &[Static::Level(
            6,
            &[
                Grant::Keyword(Keyword::Deflect(1)),
                Grant::Keyword(Keyword::Ganking),
            ],
        )],
    );

    fn afield(ctx: &Ctx, card: u32) -> bool {
        ctx.at_battlefield(card)
    }

    static ROAMER: Card = prelude::with_statics(
        prelude::unit("Roamer", &[], &[]),
        &[Static::While(afield, &[Grant::Might(2)])],
    );

    fn always(_: &Ctx, _: u32, _: u32) -> bool {
        true
    }

    static RALLYING_LEGEND: Card = prelude::with_statics(
        prelude::legend("Rallying", &[], &[]),
        &[Static::Aura {
            scope: Scope::FriendlyUnits,
            when: always,
            grants: &[Grant::Might(1)],
        }],
    );

    static WASTELAND: Card = prelude::with_statics(
        prelude::battlefield("Wasteland", &[], &[]),
        &[Static::Aura {
            scope: Scope::UnitsHere,
            when: always,
            grants: &[Grant::Might(-4)],
        }],
    );

    fn fresh(ctx: &Ctx, _unit: u32, gear: u32) -> bool {
        attach::attached_this_turn(ctx, gear)
    }

    static CLUB: Card = prelude::with_statics(
        prelude::gear("Club", &[Keyword::Equip(CHAOS)], &[equip(CHAOS)]),
        &[while_attached(&[Grant::Might(1), Grant::MightIf(fresh, 2)])],
    );

    const CLUB_ID: u32 = 90;

    static RELIC: Card = prelude::with_statics(
        prelude::gear("Relic", &[Keyword::Equip(CHAOS)], &[equip(CHAOS)]),
        &[while_attached(&[
            Grant::Static(Static::Level(2, &[Grant::Might(1)])),
            Grant::Static(Static::Aura {
                scope: Scope::FriendlyUnits,
                when: always,
                grants: &[Grant::Keyword(Keyword::Ganking)],
            }),
            Grant::Static(Static::NoMoveByEnemy),
        ])],
    );

    fn script(fixture: &mut Fixture, card: u32, held: &'static Card) {
        fixture.scripts = fixture.scripts.clone().with_script(card, held);
    }

    fn keywords(ctx: &Ctx, card: u32) -> Vec<Keyword> {
        grants_on(ctx, card)
            .into_iter()
            .filter_map(|grant| match grant {
                Grant::Keyword(keyword) => Some(keyword),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_level_grant_is_read_from_the_controllers_xp_at_query_time() {
        let mut fixture = Fixture::enforced();
        script(&mut fixture, fixtures::VI, &LEVELED);
        fixture.set_xp(0, 5);
        let mut ctx = fixture.ctx();
        assert!(!level_active(&ctx, fixtures::VI, 6));
        assert!(keywords(&ctx, fixtures::VI).is_empty());
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(ctx.deflect_of(fixtures::VI), 0);
        ctx.score_xp(0, 1);
        assert_eq!(ctx.xp(0), 6);
        assert!(
            level_active(&ctx, fixtures::VI, 6),
            "824.1.c · Level 6 at 6 XP"
        );
        assert_eq!(
            keywords(&ctx, fixtures::VI),
            [Keyword::Deflect(1), Keyword::Ganking]
        );
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(ctx.deflect_of(fixtures::VI), 1);
        assert!(ctx.spend_xp(0, 2));
        assert_eq!(ctx.xp(0), 4);
        assert!(
            keywords(&ctx, fixtures::VI).is_empty(),
            "the projection has nothing cached to rebuild"
        );
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(ctx.deflect_of(fixtures::VI), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_level_grant_reaches_ganking_and_deflect_readers_and_a_card_off_the_board_has_none() {
        let mut fixture = Fixture::enforced();
        script(&mut fixture, fixtures::VI, &LEVELED);
        script(&mut fixture, fixtures::HAND_UNIT, &LEVELED);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.set_xp(0, 6);
        let ctx = fixture.ctx();
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Ok(()),
            "736 · the granted Ganking lets the standard move go battlefield to battlefield"
        );
        assert!(
            grants_on(&ctx, fixtures::HAND_UNIT).is_empty(),
            "a card in hand is not in play, so its Level grants nothing"
        );
        assert!(!in_play(&ctx, fixtures::HAND_UNIT));
        assert!(in_play(&ctx, fixtures::VI));
        assert!(in_play(&ctx, fixtures::LEGEND_CARD));
        assert!(in_play(&ctx, fixtures::GROUNDS));
        drop(ctx);
        fixture.table.counters.clear();
        fixture.set_xp(0, 5);
        let ctx = fixture.ctx();
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "at 5 XP the move is refused"
        );
    }

    #[test]
    fn a_control_changed_card_reads_the_new_controllers_xp() {
        let mut fixture = Fixture::enforced();
        script(&mut fixture, fixtures::THEIR_UNIT, &LEVELED);
        fixture.set_xp(0, 6);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 1);
        assert!(
            keywords(&ctx, fixtures::THEIR_UNIT).is_empty(),
            "seat 1 has no XP"
        );
        assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, fixtures::VI));
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 0);
        assert_eq!(
            keywords(&ctx, fixtures::THEIR_UNIT),
            [Keyword::Deflect(1), Keyword::Ganking],
            "824.1.c.1 · Level reads the controller"
        );
    }

    #[test]
    fn a_while_grant_flips_with_its_predicate_inside_one_decide() {
        let mut fixture = Fixture::enforced();
        script(&mut fixture, fixtures::VI, &ROAMER);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert!(grants_on(&ctx, fixtures::VI).is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert!(matches!(
            grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::Might(2)]
        ));
        assert_eq!(ctx.projected_might(fixtures::VI), 2);
        ctx.recall(fixtures::VI, false);
        assert!(grants_on(&ctx, fixtures::VI).is_empty());
        assert_eq!(ctx.projected_might(fixtures::VI), 0);
    }

    #[test]
    fn a_friendly_units_aura_on_a_legend_reaches_only_its_controllers_units() {
        let mut fixture = Fixture::enforced();
        script(&mut fixture, fixtures::LEGEND_CARD, &RALLYING_LEGEND);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.projected_might(fixtures::VI), 1, "at base");
        assert!(
            ctx.blob.card_state(fixtures::VI).is_none(),
            "a never-flagged unit has no row"
        );
        assert_eq!(
            ctx.current_might(fixtures::VI),
            ctx.printed_might(fixtures::VI) + 1,
            "the projection reaches a unit without a CardState row"
        );
        assert_eq!(
            ctx.projected_might(fixtures::THEIR_UNIT),
            0,
            "the other seat's unit"
        );
        assert_eq!(ctx.projected_might(fixtures::SPRITE), 0);
        assert_eq!(
            ctx.projected_might(fixtures::LEGEND_CARD),
            0,
            "the legend is not a unit"
        );
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(ctx.projected_might(fixtures::VI), 1, "and at a battlefield");
        assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, fixtures::VI));
        assert_eq!(
            ctx.projected_might(fixtures::THEIR_UNIT),
            1,
            "friendly follows the controller"
        );
    }

    #[test]
    fn a_units_here_aura_on_a_battlefield_reaches_both_seats_units_there_and_nothing_at_base() {
        let mut fixture = Fixture::enforced();
        script(&mut fixture, fixtures::GROUNDS, &WASTELAND);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(95, fixtures::BASE, 0, "Homebody", 2));
        fixture
            .blob
            .card_state_mut(fixtures::THEIR_UNIT)
            .set(FLAG_DEFENDER, true);
        let ctx = fixture.ctx();
        assert!(matches!(
            grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::Might(-4)]
        ));
        assert!(matches!(
            grants_on(&ctx, fixtures::THEIR_UNIT).as_slice(),
            [Grant::Might(-4)]
        ));
        assert!(
            grants_on(&ctx, fixtures::SPRITE).is_empty(),
            "another battlefield"
        );
        assert!(grants_on(&ctx, 95).is_empty(), "base");
        assert!(
            grants_on(&ctx, fixtures::GROUNDS).is_empty(),
            "the battlefield does not grant to itself"
        );
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            0,
            "a 2-Might unit under a -4 aura reads 0, never less"
        );
        assert_eq!(ctx.projected_might(fixtures::VI), -4);
    }

    #[test]
    fn a_might_if_on_attached_gear_counts_while_its_predicate_holds_and_the_stored_part_stays() {
        let mut fixture = Fixture::enforced();
        let mut club = fixtures::gear(CLUB_ID, fixtures::BASE, 0, "Club", 2);
        club.domain = vec!["Chaos".into()];
        fixture.table.cards.push(club);
        script(&mut fixture, CLUB_ID, &CLUB);
        let mut ctx = fixture.ctx();
        assert_eq!(
            attach::attach(&mut ctx, CLUB_ID, fixtures::VI),
            attach::Attached::Yes
        );
        assert!(matches!(
            grants_on(&ctx, fixtures::VI).as_slice(),
            [Grant::MightIf(_, 2)]
        ));
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        ctx.blob.core_mut().unwrap().advance();
        assert!(grants_on(&ctx, fixtures::VI).is_empty());
        assert_eq!(
            ctx.current_might(fixtures::VI),
            4,
            "the stored +1 stays while the projected +2 lapses"
        );
        assert!(
            grants_on(&ctx, CLUB_ID).is_empty(),
            "an attached gear projects nothing of its own"
        );
    }

    #[test]
    fn a_gears_granted_level_aura_and_rule_static_are_read_off_the_wearer() {
        let mut fixture = Fixture::enforced();
        let mut relic = fixtures::gear(CLUB_ID, fixtures::BASE, 0, "Relic", 2);
        relic.domain = vec!["Chaos".into()];
        fixture.table.cards.push(relic);
        script(&mut fixture, CLUB_ID, &RELIC);
        fixture.set_xp(0, 1);
        let mut ctx = fixture.ctx();
        assert!(grants_on(&ctx, fixtures::VI).is_empty());
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Ganking));
        assert_eq!(
            attach::attach(&mut ctx, CLUB_ID, fixtures::THEIR_UNIT),
            attach::Attached::Yes
        );
        assert!(
            ctx.has_static(fixtures::THEIR_UNIT, Static::NoMoveByEnemy),
            "the plain static projects onto the wearer"
        );
        assert!(!ctx.has_static(fixtures::VI, Static::NoMoveByEnemy));
        assert_eq!(
            ctx.projected_might(fixtures::THEIR_UNIT),
            0,
            "seat 1 has no XP for the granted Level"
        );
        assert!(
            ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Ganking)
                && ctx.has_keyword(fixtures::SPRITE, Keyword::Ganking),
            "the granted aura reads the wearer as its source"
        );
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, fixtures::VI));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert!(!ctx.has_keyword(fixtures::SPRITE, Keyword::Ganking));
        assert_eq!(ctx.projected_might(fixtures::THEIR_UNIT), 0, "1 XP");
        ctx.score_xp(0, 1);
        assert_eq!(
            ctx.projected_might(fixtures::THEIR_UNIT),
            1,
            "the granted Level reads the wearer's controller"
        );
        assert!(attach::detach(&mut ctx, CLUB_ID));
        assert!(grants_on(&ctx, fixtures::THEIR_UNIT).is_empty());
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
    }

    #[test]
    fn an_attached_gear_a_facedown_card_and_a_pending_play_are_no_aura_sources() {
        let mut fixture = Fixture::enforced();
        let mut club = fixtures::gear(CLUB_ID, fixtures::BASE, 0, "Club", 2);
        club.domain = vec!["Chaos".into()];
        fixture.table.cards.push(club);
        script(&mut fixture, CLUB_ID, &RALLYING_LEGEND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.projected_might(fixtures::VI),
            1,
            "a loose gear with an aura projects"
        );
        assert_eq!(
            attach::attach(&mut ctx, CLUB_ID, fixtures::THEIR_UNIT),
            attach::Attached::Yes
        );
        assert_eq!(
            ctx.projected_might(fixtures::VI),
            0,
            "134.4 · an attached gear's rules text is inactive"
        );
    }
}
