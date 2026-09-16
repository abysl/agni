use super::prelude::{
    a_card, attach_gear, card_target, detach_gear, done, is_attached, play, unit, when,
    with_additional, ENEMY_GEAR,
};
use super::{Card, Cost, Domain, Event, Flow, Item, Keyword, Power, Source, Stage};
use crate::engine::ctx::Ctx;

pub const ADDITIONAL: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Body), Power::Domain(Domain::Body)],
};

pub fn paid_additional_on_entry(_ctx: &Ctx, event: &Event, source: Source) -> bool {
    matches!(
        event,
        Event::Played {
            card,
            paid_additional: true,
            ..
        } if *card == source.card
    )
}

pub fn steal(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(gear) = card_target(ctx, item, 0) else {
        return done();
    };
    if !ctx.is_gear(gear) || !ctx.on_board(gear) || ctx.controller(gear) == item.controller {
        return done();
    }
    if is_attached(ctx, gear) {
        detach_gear(ctx, gear);
    }
    ctx.set_controller(gear, item.controller, me);
    let equipment = ctx.script(gear).is_some_and(|script| script.is_equipment());
    if equipment && ctx.on_board(me) {
        attach_gear(ctx, gear, me);
    }
    done()
}

pub static CARD: Card = with_additional(
    unit(
        "Akshan - Mischievous",
        &[Keyword::Weaponmaster],
        &[when(
            play(&[a_card(ENEMY_GEAR, "an enemy gear to take")], steal),
            paid_additional_on_entry,
        )],
    ),
    ADDITIONAL,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::WEAPONMASTER;
    use crate::cards::Trigger;
    use crate::engine::attach;
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::Location;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef, SLOT_ADDITIONAL};
    use agni_plugin_sdk::table::CardInfo;

    const AKSHAN: u32 = 90;
    const THEIR_GEAR: u32 = 91;
    const MY_GEAR: u32 = 92;
    const BODY_RUNES: [u32; 2] = [46, 47];

    fn akshan(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Body".into()],
            ..fixtures::unit(AKSHAN, zone, 0, "Akshan - Mischievous", 4)
        }
    }

    fn heist() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(akshan(fixtures::BASE));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Loot", 2));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        for rune in BODY_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn akshan_prints_weaponmaster_a_double_body_additional_cost_and_a_conditional_steal() {
        let fixture = heist();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(AKSHAN).unwrap(),
            &CARD
        ));
        assert!(CARD.has_keyword(Keyword::Weaponmaster));
        assert_eq!(CARD.additional, Some(ADDITIONAL));
        assert_eq!(
            CARD.abilities.len(),
            1,
            "821 · Weaponmaster is the engine's"
        );
        assert_eq!(WEAPONMASTER.trigger, Trigger::Play);
        assert_eq!(WEAPONMASTER.label, Some("weaponmaster"));
        let steal = &CARD.abilities[0];
        assert_eq!(steal.trigger, Trigger::Play);
        assert!(steal.condition.is_some());
        assert!(!steal.optional);
        assert_eq!(steal.targets.len(), 1);
        assert_eq!(steal.targets[0].filter, ENEMY_GEAR);
    }

    #[test]
    fn the_additional_cost_reads_two_body_needs_on_top_of_the_printed_four() {
        let mut fixture = heist();
        let ctx = fixture.ctx();
        let mut item = ChainItem::new(1, ItemKind::Permanent { card: AKSHAN }, 0, Origin::Hand);
        assert_eq!(cost::of_item(&ctx, &item, None).power, []);
        item.set_slot(SLOT_ADDITIONAL, 1);
        let paid = cost::of_item(&ctx, &item, None);
        assert_eq!(paid.energy, 4);
        assert_eq!(
            paid.power,
            [Need::Domain(Domain::Body), Need::Domain(Domain::Body)]
        );
        assert!(crate::engine::pay::affordable(&ctx, 0, &paid));
    }

    #[test]
    fn an_unpaid_akshan_played_from_hand_has_no_steal_prompt() {
        let mut fixture = heist();
        fixture.table.card_mut(AKSHAN).unwrap().zone = Some(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, AKSHAN).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL),
            "the additional cost is asked after Accelerate: {:?}",
            ctx.blob.why
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.location(AKSHAN), Some(Location::Base(0)));
        assert!(ctx.blob.prompt.is_none(), "no target prompt");
        assert!(
            !ctx.blob.chain.iter().chain(ctx.blob.queue.iter().map(|pending| &pending.item)).any(
                |held| matches!(held.kind, ItemKind::Trigger { source, index: 0 } if source == AKSHAN)
            ),
            "the steal never triggers unpaid"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.controller(THEIR_GEAR), 1, "nothing was stolen");
        assert_eq!(ctx.location(THEIR_GEAR), Some(Location::Base(1)));
    }

    #[test]
    fn a_paid_akshan_from_hand_steals_an_enemy_equipment_and_wears_it() {
        let mut fixture = heist();
        fixture.table.card_mut(AKSHAN).unwrap().zone = Some(fixtures::HAND);
        fixture.table.card_mut(THEIR_GEAR).unwrap().name = "Brutalizer".into();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, AKSHAN).unwrap();
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL)
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(AKSHAN), Some(Location::Base(0)));
        assert!(
            ctx.effects
                .iter()
                .filter(|effect| matches!(effect, agni_plugin_sdk::decide::Effect::Move { card, .. } if BODY_RUNES.contains(card)))
                .count()
                >= 2,
            "both Body runes are spent: {:?}",
            ctx.effects
        );
        for _ in 0..8 {
            fixtures::pass_until_open(&mut ctx);
            let labels = fixtures::labels(&ctx);
            let Some(first) = labels.first() else {
                break;
            };
            let gear = format!("{{card {THEIR_GEAR}}}");
            let pick = if labels.contains(&gear) {
                gear
            } else {
                first.clone()
            };
            fixtures::choose(&mut ctx, 0, &pick).unwrap();
        }
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert_eq!(
            ctx.controller(THEIR_GEAR),
            0,
            "356.4.f.1: paid means the steal fires"
        );
        assert_eq!(ctx.owner(THEIR_GEAR), 1);
        assert!(attach::is_attached(&ctx, THEIR_GEAR));
        assert_eq!(
            ctx.state_of(THEIR_GEAR).unwrap().attached_to,
            Some(AKSHAN),
            "an Equipment is attached to Akshan"
        );
    }

    #[test]
    fn the_steal_takes_control_moves_the_gear_to_his_base_and_leaves_a_friendly_gear_alone() {
        let mut fixture = heist();
        let mut ctx = fixture.ctx();
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: AKSHAN,
                index: 0,
            },
            0,
            Origin::Board,
        );
        item.targets.push(TargetRef::Card(THEIR_GEAR));
        assert_eq!(steal(&mut ctx, &item, Stage(0)), Flow::Done);
        assert_eq!(ctx.controller(THEIR_GEAR), 0);
        assert_eq!(ctx.owner(THEIR_GEAR), 1, "ownership never changes");
        assert_eq!(ctx.location(THEIR_GEAR), Some(Location::Base(0)));
        assert_eq!(
            ctx.state_of(THEIR_GEAR).unwrap().control_source,
            Some(AKSHAN)
        );
        assert!(
            !attach::is_attached(&ctx, THEIR_GEAR),
            "Loot prints no Equip, so it is not attached"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} takes control of {{card {THEIR_GEAR}}}"
        )));
        let mut own = ChainItem::new(
            2,
            ItemKind::Trigger {
                source: AKSHAN,
                index: 0,
            },
            0,
            Origin::Board,
        );
        own.targets.push(TargetRef::Card(MY_GEAR));
        steal(&mut ctx, &own, Stage(0));
        assert!(ctx
            .state_of(MY_GEAR)
            .is_none_or(|row| row.controlled_by.is_none()));
    }
}
