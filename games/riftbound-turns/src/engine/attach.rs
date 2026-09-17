use crate::cards::{Ability, Grant, Keyword, Mirror, Static, GRANTED, IMPLICIT_FLOW, KIND_GEAR};
use crate::engine::ctx::Ctx;
use crate::state::Expiry;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const ANNOTATION_ATTACHED: &str = "attached";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attached {
    Yes,
    NotGear,
    NotAUnit,
    Already,
}

pub fn attached_to(ctx: &Ctx, gear: u32) -> Option<u32> {
    ctx.state_of(gear).and_then(|row| row.attached_to)
}

pub fn is_attached(ctx: &Ctx, gear: u32) -> bool {
    attached_to(ctx, gear).is_some()
}

pub fn attached_this_turn(ctx: &Ctx, gear: u32) -> bool {
    is_attached(ctx, gear) && ctx.attached_turn(gear) == ctx.turn()
}

pub fn attachments_of(ctx: &Ctx, unit: u32) -> Vec<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.attached_to == Some(unit))
        .map(|row| row.id)
        .collect()
}

pub fn grants_of(ctx: &Ctx, gear: u32) -> &'static [Grant] {
    ctx.script(gear)
        .map(|script| script.attached_grants())
        .unwrap_or(&[])
}

pub fn might_bonus_of(ctx: &Ctx, gear: u32) -> Option<i16> {
    grants_of(ctx, gear)
        .iter()
        .filter_map(|grant| match grant {
            Grant::Might(bonus) => Some(*bonus),
            Grant::Keyword(_)
            | Grant::Static(_)
            | Grant::MightIf(..)
            | Grant::Ability(_)
            | Grant::Copied(_)
            | Grant::Mirror(_)
            | Grant::Borrowed(_) => None,
        })
        .reduce(|a, b| a.saturating_add(b))
}

pub fn granted_abilities(ctx: &Ctx, gear: u32) -> Vec<&'static Ability> {
    grants_of(ctx, gear)
        .iter()
        .map(|grant| match grant {
            Grant::Ability(held) => *held,
            Grant::Copied(text) => text(ctx, gear),
            _ => &[],
        })
        .flat_map(|held| held.iter())
        .take(usize::from(IMPLICIT_FLOW - GRANTED))
        .collect()
}

pub fn granted_on(ctx: &Ctx, unit: u32) -> Vec<(u32, u8, &'static Ability)> {
    attachments_of(ctx, unit)
        .into_iter()
        .flat_map(|gear| {
            granted_abilities(ctx, gear)
                .into_iter()
                .enumerate()
                .map(move |(offset, ability)| (gear, GRANTED + offset as u8, ability))
        })
        .collect()
}

pub fn mirrors_on(ctx: &Ctx, unit: u32) -> Vec<Mirror> {
    attachments_of(ctx, unit)
        .into_iter()
        .flat_map(|gear| grants_of(ctx, gear).iter())
        .filter_map(|grant| match grant {
            Grant::Mirror(mirror) => Some(*mirror),
            _ => None,
        })
        .collect()
}

pub fn granted_statics(ctx: &Ctx, unit: u32) -> Vec<Static> {
    attachments_of(ctx, unit)
        .into_iter()
        .flat_map(|gear| grants_of(ctx, gear).iter())
        .filter_map(|grant| match grant {
            Grant::Static(held) => Some(*held),
            Grant::Keyword(_)
            | Grant::Might(_)
            | Grant::MightIf(..)
            | Grant::Ability(_)
            | Grant::Copied(_)
            | Grant::Mirror(_)
            | Grant::Borrowed(_) => None,
        })
        .collect()
}

pub fn has_static(ctx: &Ctx, card: u32, wanted: Static) -> bool {
    ctx.script(card)
        .is_some_and(|script| script.has_static(wanted))
        || granted_statics(ctx, card)
            .iter()
            .any(|held| held.same_kind(wanted))
}

fn mirror(ctx: &mut Ctx, gear: u32, unit: Option<u32>) {
    if ctx.card(gear).is_none() {
        return;
    }
    ctx.emit(Effect::Annotate {
        card: gear,
        key: ANNOTATION_ATTACHED.into(),
        value: unit.map(|unit| unit.to_le_bytes().to_vec()),
    });
}

pub fn follow(ctx: &mut Ctx, gear: u32) -> bool {
    let Some(unit) = attached_to(ctx, gear) else {
        return false;
    };
    let Some(to) = ctx.location(unit) else {
        return false;
    };
    if ctx.location(gear) == Some(to) {
        return false;
    }
    let Some((zone, seat)) = ctx.zone_of(to) else {
        return false;
    };
    ctx.emit(Effect::Move {
        card: gear,
        zone,
        seat,
        index: TOP,
    });
    true
}

pub fn follow_unit(ctx: &mut Ctx, unit: u32) -> usize {
    attachments_of(ctx, unit)
        .into_iter()
        .filter(|gear| follow(ctx, *gear))
        .count()
}

pub fn attach(ctx: &mut Ctx, gear: u32, unit: u32) -> Attached {
    if !ctx.is_gear(gear) || (!ctx.on_board(gear) && !ctx.is_pending_play(gear)) {
        return Attached::NotGear;
    }
    if !ctx.is_unit(unit) || !ctx.on_board(unit) || gear == unit {
        return Attached::NotAUnit;
    }
    if attached_to(ctx, gear) == Some(unit) {
        return Attached::Already;
    }
    detach(ctx, gear);
    let turn = ctx.turn();
    {
        let row = ctx.state_mut(gear);
        row.attached_to = Some(unit);
        row.attached_turn = turn;
    }
    mirror(ctx, gear, Some(unit));
    for grant in grants_of(ctx, gear) {
        if let Grant::Keyword(keyword) = grant {
            ctx.grant(unit, *keyword, Expiry::WhileAttached(gear));
        }
    }
    follow(ctx, gear);
    ctx.narrate(format!("{{card {gear}}} is attached to {{card {unit}}}"));
    let printed_might = ctx.card(unit).is_some_and(|held| held.might.is_some());
    if let Some(bonus) = might_bonus_of(ctx, gear).filter(|bonus| *bonus != 0 && printed_might) {
        ctx.might(unit, bonus, Expiry::WhileAttached(gear), None, 0);
        ctx.narrate(format!(
            "{{card {unit}}} gets {bonus:+} Might while {{card {gear}}} is attached"
        ));
    }
    Attached::Yes
}

pub fn detach(ctx: &mut Ctx, gear: u32) -> bool {
    let unit = attached_to(ctx, gear);
    ctx.expire(Expiry::WhileAttached(gear));
    let Some(unit) = unit else {
        return false;
    };
    {
        let row = ctx.state_mut(gear);
        row.attached_to = None;
        row.attached_turn = 0;
    }
    mirror(ctx, gear, None);
    ctx.narrate(format!("{{card {gear}}} detaches from {{card {unit}}}"));
    true
}

pub fn detach_all(ctx: &mut Ctx, unit: u32) -> usize {
    attachments_of(ctx, unit)
        .into_iter()
        .filter(|gear| detach(ctx, *gear))
        .count()
}

fn stale_grants(ctx: &Ctx) -> Vec<u32> {
    let mut stale: Vec<u32> = ctx
        .blob
        .cards
        .iter()
        .flat_map(|row| {
            let granted = row.granted.iter().map(|(_, until)| *until);
            let modded = row.might.iter().map(|held| held.until);
            granted.chain(modded).filter_map(move |until| match until {
                Expiry::WhileAttached(gear) if attached_to(ctx, gear) != Some(row.id) => Some(gear),
                _ => None,
            })
        })
        .collect();
    stale.sort_unstable();
    stale.dedup();
    stale
}

pub fn recall_loose_gear(ctx: &mut Ctx) -> Vec<u32> {
    let loose: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|card| {
            card.is_kind(KIND_GEAR)
                && ctx
                    .face_location(card)
                    .is_some_and(|at| at.battlefield().is_some())
        })
        .map(|card| card.id)
        .filter(|card| !ctx.is_facedown(*card) && !ctx.is_pending_play(*card))
        .filter(|card| attached_to(ctx, *card).is_none())
        .collect();
    for gear in &loose {
        ctx.recall(*gear, false);
        ctx.narrate(format!("{{card {gear}}} is recalled to base"));
    }
    loose
}

pub fn sync(ctx: &mut Ctx) {
    for gear in stale_grants(ctx) {
        ctx.expire(Expiry::WhileAttached(gear));
    }
    let attached: Vec<(u32, u32)> = ctx
        .blob
        .cards
        .iter()
        .filter_map(|row| row.attached_to.map(|unit| (row.id, unit)))
        .collect();
    for (gear, unit) in attached {
        if !ctx.is_gear(gear) || !ctx.on_board(gear) {
            {
                let row = ctx.state_mut(gear);
                row.attached_to = None;
                row.attached_turn = 0;
            }
            ctx.expire(Expiry::WhileAttached(gear));
            continue;
        }
        if !ctx.is_unit(unit) || !ctx.on_board(unit) {
            detach(ctx, gear);
            continue;
        }
        follow(ctx, gear);
    }
    recall_loose_gear(ctx);
    ctx.blob.cards.retain(|row| !row.is_default());
}

impl Ctx<'_> {
    pub fn attach(&mut self, gear: u32, unit: u32) -> Attached {
        attach(self, gear, unit)
    }

    pub fn detach(&mut self, gear: u32) -> bool {
        detach(self, gear)
    }

    pub fn detach_all(&mut self, unit: u32) -> usize {
        detach_all(self, unit)
    }

    pub fn attached_to(&self, gear: u32) -> Option<u32> {
        attached_to(self, gear)
    }
}

pub fn has_granted_keyword(ctx: &Ctx, unit: u32, keyword: Keyword) -> bool {
    ctx.state_of(unit).is_some_and(|row| {
        row.granted.iter().any(|(held, until)| {
            held.same_kind(keyword) && matches!(until, Expiry::WhileAttached(_))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, equip, while_attached, CHAOS};
    use crate::cards::{Card, Keyword};
    use crate::engine::ctx::{Cause, Killed, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, cleanup, march, priority, prompts, resume, settle};
    use crate::state::{Expiry, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::Pick;

    pub const BOOTS: u32 = 90;

    pub static BOOTS_CARD: Card = prelude::with_statics(
        prelude::gear("Boots", &[Keyword::Equip(CHAOS)], &[equip(CHAOS)]),
        &[while_attached(&[Grant::Keyword(Keyword::Ganking)])],
    );

    fn shod() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut boots = fixtures::gear(BOOTS, fixtures::BASE, 0, "Boots", 3);
        boots.domain = vec!["Chaos".into()];
        fixture.table.cards.push(boots);
        for rune in [fixtures::RUNE_A, 41] {
            fixture.table.card_mut(rune).unwrap().domain = vec!["Chaos".into()];
            fixture.table.card_mut(rune).unwrap().name = "Chaos Rune".into();
        }
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(BOOTS, &BOOTS_CARD);
        fixture
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    pub static HEAVY_BOOTS_CARD: Card = prelude::with_statics(
        prelude::gear("Boots", &[Keyword::Equip(CHAOS)], &[equip(CHAOS)]),
        &[while_attached(&[
            Grant::Keyword(Keyword::Ganking),
            Grant::Might(2),
            Grant::Might(1),
        ])],
    );

    #[test]
    fn a_might_bonus_rides_the_attachment_and_leaves_with_it() {
        let mut fixture = shod();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BOOTS, &HEAVY_BOOTS_CARD);
        let mut ctx = fixture.ctx();
        assert_eq!(might_bonus_of(&ctx, BOOTS), Some(3));
        assert_eq!(might_bonus_of(&ctx, fixtures::VI), None);
        let printed = ctx.current_might(fixtures::VI);
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Yes);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            printed + 3,
            "136 · the Might Bonus"
        );
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("gets +3 Might while {card 90} is attached")));
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Already);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            printed + 3,
            "never doubled"
        );
        assert!(detach(&mut ctx, BOOTS));
        assert_eq!(ctx.current_might(fixtures::VI), printed);
        assert!(ctx.state_of(fixtures::VI).unwrap().might.is_empty());
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Yes);
        assert_eq!(ctx.current_might(fixtures::VI), printed + 3);
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::THEIR_UNIT), Attached::Yes);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            printed,
            "re-equipping moves the bonus"
        );
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            i32::from(ctx.card(fixtures::THEIR_UNIT).unwrap().might.unwrap()) + 3
        );
    }

    #[test]
    fn attaching_grants_ganking_moves_the_gear_along_and_detaching_takes_it_back() {
        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Yes);
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Already);
        assert_eq!(attach(&mut ctx, fixtures::VI, BOOTS), Attached::NotGear);
        assert_eq!(
            attach(&mut ctx, BOOTS, fixtures::GROUNDS),
            Attached::NotAUnit
        );
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert!(has_granted_keyword(&ctx, fixtures::VI, Keyword::Ganking));
        assert_eq!(attached_to(&ctx, BOOTS), Some(fixtures::VI));
        assert_eq!(attachments_of(&ctx, fixtures::VI), [BOOTS]);
        assert_eq!(
            ctx.card(BOOTS).unwrap().annotation(ANNOTATION_ATTACHED),
            Some(&fixtures::VI.to_le_bytes()[..]),
            "the mirror kai draws"
        );
        assert_eq!(
            ctx.state_of(fixtures::VI).unwrap().granted,
            [(Keyword::Ganking, Expiry::WhileAttached(BOOTS))]
        );
        assert_eq!(
            ctx.move_unit(
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert_eq!(follow_unit(&mut ctx, fixtures::VI), 1);
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Battlefield(fixtures::BF1)),
            "719.3.a · attached gear moves with its unit"
        );
        assert_eq!(follow_unit(&mut ctx, fixtures::VI), 0);
        assert!(detach(&mut ctx, BOOTS));
        assert!(!detach(&mut ctx, BOOTS));
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(attached_to(&ctx, BOOTS), None);
        assert_eq!(
            ctx.card(BOOTS).unwrap().annotation(ANNOTATION_ATTACHED),
            None
        );
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Battlefield(fixtures::BF1)),
            "422.4 · it detaches where the unit stood"
        );
        assert_eq!(recall_loose_gear(&mut ctx), [BOOTS]);
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Base(0)),
            "435.1 · a loose gear at a battlefield is recalled"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_ganking_unit_marches_between_battlefields_and_one_without_is_refused() {
        let mut fixture = shod();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
        attach(&mut ctx, BOOTS, fixtures::VI);
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Ok(()),
            "143.4.c / 736 · Ganking lets the standard move go battlefield to battlefield"
        );
        detach(&mut ctx, BOOTS);
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Err(Refusal::Illegal(Reason::NeedsGanking))
        );
    }

    #[test]
    fn an_equip_activation_pays_the_rune_targets_a_friendly_unit_and_attaches_on_resolution() {
        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        let offers = activate::offers(&ctx, 0);
        let offer = offers
            .iter()
            .find(|offer| offer.source == BOOTS)
            .unwrap_or_else(|| panic!("the loose gear offers its Equip: {offers:?}"));
        assert!(offer.enabled);
        assert_eq!(offer.label, "{card 90}: equip (1 Chaos power)");
        activate::activate(&mut ctx, 0, BOOTS, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        let labels: Vec<String> = prompts::offered(&ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect();
        assert_eq!(
            labels,
            ["{card 50}", "cancel"],
            "only the friendly unit is on offer"
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "744.1 · Equip is an activated ability"
        );
        assert!(
            ctx.events.iter().any(|event| matches!(
                event,
                crate::engine::ctx::Event::Chosen { card, by: 0, .. } if *card == fixtures::VI
            )),
            "744.1.b.1 · Equip's choice is a target"
        );
        assert_eq!(
            ctx.runes_of(0).len(),
            3,
            "the spent Chaos rune is recycled for the power"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert!(
            !ctx.card(BOOTS).unwrap().exhausted,
            "gear does not exhaust to equip"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, BOOTS), Some(fixtures::VI));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert_eq!(
            activate::activate(&mut ctx, 0, BOOTS, 0),
            Err(Refusal::Illegal(Reason::Attached)),
            "134.4 · an attached gear's rules text is inactive"
        );
        assert!(activate::offers(&ctx, 0)
            .iter()
            .all(|offer| offer.source != BOOTS));
    }

    #[test]
    fn gear_is_recalled_at_cleanup_when_its_unit_dies_and_its_grant_dies_with_it() {
        let mut fixture = shod();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        attach(&mut ctx, BOOTS, fixtures::VI);
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.kill(fixtures::VI, Cause::Rule), Killed::Yes);
        cleanup::run(&mut ctx, None);
        assert_eq!(attached_to(&ctx, BOOTS), None);
        assert_eq!(
            ctx.location(BOOTS),
            Some(Location::Base(0)),
            "435.1 · the equipment its dead unit left behind is recalled at cleanup"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 90} is recalled to base"));
        assert!(ctx
            .blob
            .card_state(BOOTS)
            .is_none_or(|row| row.is_default()));
        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        attach(&mut ctx, BOOTS, fixtures::VI);
        assert_eq!(ctx.kill(BOOTS, Cause::Rule), Killed::Yes);
        sync(&mut ctx);
        assert!(
            !ctx.has_keyword(fixtures::VI, Keyword::Ganking),
            "the grant leaves with the gear"
        );
        assert!(ctx
            .state_of(fixtures::VI)
            .is_none_or(|row| row.granted.is_empty()));
        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        attach(&mut ctx, BOOTS, fixtures::VI);
        assert!(ctx.bounce(fixtures::VI));
        sync(&mut ctx);
        assert_eq!(
            attached_to(&ctx, BOOTS),
            None,
            "719.5 · a bounced unit sheds its gear"
        );
        assert!(ctx.fault.is_none());
    }

    fn fresh(_ctx: &Ctx, _unit: u32, _gear: u32) -> bool {
        true
    }

    pub static FRESH_BOOTS_CARD: Card = prelude::with_statics(
        prelude::gear("Boots", &[Keyword::Equip(CHAOS)], &[equip(CHAOS)]),
        &[while_attached(&[Grant::Might(1), Grant::MightIf(fresh, 2)])],
    );

    #[test]
    fn the_attach_turn_is_recorded_on_every_attach_and_a_detach_leaves_it_behind() {
        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.attached_turn(BOOTS), 0);
        assert!(!attached_this_turn(&ctx, BOOTS));
        let turn = ctx.turn();
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Yes);
        assert_eq!(ctx.attached_turn(BOOTS), turn);
        assert!(attached_this_turn(&ctx, BOOTS));
        ctx.blob.core_mut().unwrap().advance();
        assert_eq!(ctx.turn(), turn + 1);
        assert_eq!(
            ctx.attached_turn(BOOTS),
            turn,
            "the record is the turn of the attach"
        );
        assert!(
            !attached_this_turn(&ctx, BOOTS),
            "it lapses when the turn counter advances, whoever's turn it is"
        );
        assert!(detach(&mut ctx, BOOTS));
        assert_eq!(
            ctx.attached_turn(BOOTS),
            0,
            "detach clears the turn so a loose gear's row can be pruned"
        );
        assert!(
            !attached_this_turn(&ctx, BOOTS),
            "a loose gear was not attached this turn"
        );
        sync(&mut ctx);
        assert!(ctx
            .blob
            .card_state(BOOTS)
            .is_none_or(|row| row.is_default()));
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Yes);
        assert_eq!(
            ctx.attached_turn(BOOTS),
            turn + 1,
            "434.1.f · a re-attach on the new turn overwrites it"
        );
        assert!(attached_this_turn(&ctx, BOOTS));
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Already);
        assert_eq!(ctx.attached_turn(BOOTS), turn + 1);
        sync(&mut ctx);
        assert_eq!(
            ctx.attached_turn(BOOTS),
            turn + 1,
            "the row survives a sync while the gear stays attached"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_dead_gears_might_bonus_is_expired_by_sync_even_without_a_keyword_grant() {
        let mut fixture = shod();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BOOTS, &FRESH_BOOTS_CARD);
        let mut ctx = fixture.ctx();
        let printed = ctx.current_might(fixtures::VI);
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Yes);
        assert_eq!(ctx.current_might(fixtures::VI), printed + 3);
        assert_eq!(ctx.kill(BOOTS, Cause::Rule), Killed::Yes);
        assert!(
            ctx.state_of(BOOTS).is_none(),
            "the trash drops the gear's row, so nothing points at the wearer"
        );
        sync(&mut ctx);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            printed,
            "136.3.a · the stored bonus is stale once the gear is gone"
        );
        assert!(ctx
            .state_of(fixtures::VI)
            .is_none_or(|row| row.might.is_empty()));
    }

    #[test]
    fn a_might_if_grant_is_a_projection_fact_beside_the_materialized_bonus() {
        let mut fixture = shod();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BOOTS, &FRESH_BOOTS_CARD);
        let mut ctx = fixture.ctx();
        assert_eq!(
            might_bonus_of(&ctx, BOOTS),
            Some(1),
            "only the plain Might is materialized on attach"
        );
        let printed = ctx.current_might(fixtures::VI);
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Yes);
        assert_eq!(ctx.state_of(fixtures::VI).unwrap().might.len(), 1);
        assert_eq!(ctx.state_of(fixtures::VI).unwrap().might[0].delta, 1);
        assert_eq!(
            ctx.current_might(fixtures::VI),
            printed + 3,
            "the +1 is stored, the +2 is read from the gear"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("gets +1 Might while {card 90} is attached")));
        assert!(detach(&mut ctx, BOOTS));
        assert_eq!(ctx.current_might(fixtures::VI), printed);
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::THEIR_UNIT), Attached::Yes);
        assert_eq!(ctx.current_might(fixtures::VI), printed);
        assert_eq!(
            ctx.current_might(fixtures::THEIR_UNIT),
            i32::from(ctx.card(fixtures::THEIR_UNIT).unwrap().might.unwrap()) + 3,
            "both parts move with the gear"
        );
    }

    #[test]
    fn a_boots_wearer_reads_ganking_once() {
        let mut fixture = shod();
        let mut ctx = fixture.ctx();
        assert_eq!(attach(&mut ctx, BOOTS, fixtures::VI), Attached::Yes);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert!(has_granted_keyword(&ctx, fixtures::VI, Keyword::Ganking));
        assert!(
            !ctx.projected_keyword(fixtures::VI, Keyword::Ganking),
            "the attach materialized it, so the projection does not repeat it"
        );
        assert_eq!(
            crate::engine::statics::grants_on(&ctx, fixtures::VI).len(),
            0
        );
    }

    const MASTER: u32 = 92;
    const BLADE: u32 = 93;
    const HEIRLOOM: u32 = 94;

    pub static MASTER_CARD: Card = prelude::unit("Weaponmaster", &[Keyword::Weaponmaster], &[]);

    const ONE_AND_RAINBOW: crate::cards::Cost = crate::cards::Cost {
        energy: 1,
        power: &[crate::cards::Power::Rainbow],
    };

    const CALM: crate::cards::Cost = crate::cards::Cost {
        energy: 0,
        power: &[crate::cards::Power::Domain(crate::cards::Domain::Calm)],
    };

    pub static BLADE_CARD: Card = prelude::with_statics(
        prelude::gear(
            "Blade",
            &[Keyword::Equip(ONE_AND_RAINBOW)],
            &[equip(ONE_AND_RAINBOW)],
        ),
        &[while_attached(&[Grant::Might(1)])],
    );

    pub static HEIRLOOM_CARD: Card = prelude::with_statics(
        prelude::gear("Heirloom", &[Keyword::Equip(CALM)], &[equip(CALM)]),
        &[while_attached(&[Grant::Might(2)])],
    );

    fn armory() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut master = fixtures::unit(MASTER, fixtures::HAND, 0, "Weaponmaster", 3);
        master.energy = Some(0);
        fixture.table.cards.push(master);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(MASTER, &MASTER_CARD);
        fixture
    }

    fn with_blade(fixture: &mut Fixture) {
        let mut blade = fixtures::gear(BLADE, fixtures::BASE, 0, "Blade", 2);
        blade.domain = vec!["Calm".into()];
        fixture.table.cards.push(blade);
        fixture.scripts = fixture.scripts.clone().with_script(BLADE, &BLADE_CARD);
    }

    fn with_heirloom(fixture: &mut Fixture) {
        let mut heirloom = fixtures::gear(HEIRLOOM, fixtures::BASE, 0, "Heirloom", 2);
        heirloom.domain = vec!["Calm".into()];
        fixture.table.cards.push(heirloom);
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(HEIRLOOM, &HEIRLOOM_CARD);
    }

    fn play_master(ctx: &mut Ctx) {
        crate::engine::play::begin(
            ctx,
            0,
            MASTER,
            crate::state::Origin::Hand,
            Some(Location::Base(0)),
        )
        .unwrap();
        settle(ctx).unwrap();
    }

    fn answer(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx.blob.prompt.as_ref().map(|held| held.id).unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn weaponmaster_attaches_a_chosen_equipment_for_its_equip_cost_less_one_rainbow() {
        let mut fixture = armory();
        with_blade(&mut fixture);
        let action = fixtures::move_action(MASTER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        play_master(&mut ctx);
        assert_eq!(ctx.location(MASTER), Some(Location::Base(0)));
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BLADE}}}"), "skip".to_string()],
            "821.1.c · an Equipment you control, or nothing"
        );
        answer(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            !is_attached(&ctx, BLADE),
            "nothing until the trigger resolves"
        );
        let cost = prelude::weaponmaster_cost(&ctx, BLADE).unwrap();
        assert_eq!(cost.energy, 1);
        assert!(cost.power.is_empty(), "821.1.c.3 · the [A] is struck");
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, BLADE), Some(MASTER));
        assert_eq!(ctx.attached_turn(BLADE), ctx.turn());
        assert_eq!(ctx.current_might(MASTER), 4);
        assert_eq!(ctx.ready_runes_of(0).len(), 2, "one energy paid");
        assert_eq!(
            ctx.runes_of(0).len(),
            4,
            "no rune recycled for the struck [A]"
        );
        assert!(
            !ctx.events.iter().any(|event| matches!(
                event,
                crate::engine::ctx::Event::Chosen { card, .. } if *card == MASTER
            )),
            "821.1.c.6 · the wearer is not chosen"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn weaponmaster_pays_a_domain_need_in_full_and_leaves_the_gear_when_it_cannot() {
        let mut fixture = armory();
        with_heirloom(&mut fixture);
        let action = fixtures::move_action(MASTER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_master(&mut ctx);
        let cost = prelude::weaponmaster_cost(&ctx, HEIRLOOM).unwrap();
        assert_eq!(
            cost.power,
            [crate::engine::cost::Need::Domain(
                crate::cards::Domain::Calm
            )],
            "821.1.c.3 · a cost without [A] is not reduced"
        );
        answer(&mut ctx, 0, 0).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, HEIRLOOM), Some(MASTER));
        assert_eq!(ctx.runes_of(0).len(), 3, "the Calm rune was recycled");
        assert_eq!(ctx.current_might(MASTER), 5);
        drop(ctx);

        let mut fixture = armory();
        with_heirloom(&mut fixture);
        let held = fixture.table.card_mut(42).unwrap();
        held.domain = vec!["Fury".into()];
        held.name = "Fury Rune".into();
        let action = fixtures::move_action(MASTER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_master(&mut ctx);
        answer(&mut ctx, 0, 0).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(
            attached_to(&ctx, HEIRLOOM),
            None,
            "821.1.c.5 · the gear stays where it is"
        );
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
        assert_eq!(ctx.current_might(MASTER), 3);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 94} stays where it is · its Equip cost can't be paid"));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn weaponmaster_moves_a_gear_off_its_previous_wearer_and_asks_nothing_without_equipment() {
        let mut fixture = armory();
        with_blade(&mut fixture);
        fixture.blob.card_state_mut(BLADE).attached_to = Some(fixtures::VI);
        let action = fixtures::move_action(MASTER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(attached_to(&ctx, BLADE), Some(fixtures::VI));
        play_master(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BLADE}}}"), "skip".to_string()],
            "an attached Equipment is still a choice"
        );
        answer(&mut ctx, 0, 0).unwrap();
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, BLADE), Some(MASTER), "434.1.f · it moves");
        assert_eq!(attachments_of(&ctx, fixtures::VI), Vec::<u32>::new());
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{card 93} detaches from {card 50}"));
        drop(ctx);

        let mut fixture = armory();
        fixture
            .table
            .cards
            .push(fixtures::gear(HEIRLOOM, fixtures::BASE, 0, "Trinket", 1));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(MASTER, &MASTER_CARD);
        let action = fixtures::move_action(MASTER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_master(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "a gear without Equip is not Equipment, so there is nothing to ask"
        );
        if !ctx.blob.chain.is_empty() {
            resolve_chain(&mut ctx);
        }
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, HEIRLOOM));
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "nothing was paid");
        assert!(ctx.fault.is_none());
        drop(ctx);

        let mut fixture = armory();
        with_blade(&mut fixture);
        fixture.table.card_mut(BLADE).unwrap().owner = 1;
        fixture.table.card_mut(BLADE).unwrap().seat = 1;
        let action = fixtures::move_action(MASTER, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        play_master(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "an enemy Equipment is no choice");
        if !ctx.blob.chain.is_empty() {
            resolve_chain(&mut ctx);
        }
        assert!(!is_attached(&ctx, BLADE));
    }
}
