use super::prelude::{
    a_card, activated, attach_gear, battlefield, card_target, done, exhausting_self, named,
    with_statics, Location, FRIENDLY_EQUIPMENT, FRIENDLY_UNIT,
};
use super::{
    Ability, Card, Cost, Flow, Grant, Item, Scope, Stage, Static, TargetSpec, Timing, KIND_LEGEND,
};
use crate::engine::ctx::Ctx;
use crate::engine::statics;

pub const AN_EQUIPMENT_YOU_CONTROL: TargetSpec =
    a_card(FRIENDLY_EQUIPMENT, "an Equipment you control to attach");
pub const A_UNIT_YOU_CONTROL: TargetSpec = a_card(FRIENDLY_UNIT, "a unit you control to equip");

fn attach_the_equipment(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let (Some(gear), Some(unit)) = (card_target(ctx, item, 0), card_target(ctx, item, 1)) else {
        return done();
    };
    attach_gear(ctx, gear, unit);
    done()
}

pub const ATTACH_AN_EQUIPMENT: Ability = named(
    exhausting_self(activated(
        Timing::Sorcery,
        Cost::FREE,
        &[AN_EQUIPMENT_YOU_CONTROL, A_UNIT_YOU_CONTROL],
        attach_the_equipment,
    )),
    "attach an Equipment",
);

pub static LENT: [Ability; 1] = [ATTACH_AN_EQUIPMENT];

pub fn controls_it(ctx: &Ctx, forge: u32, seat: u8) -> bool {
    statics::in_play(ctx, forge)
        && matches!(ctx.location(forge), Some(Location::Battlefield(zone)) if ctx.holds(seat, zone))
}

pub fn is_forge(ctx: &Ctx, card: u32) -> bool {
    ctx.script(card)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
}

pub fn lends_to(ctx: &Ctx, forge: u32, legend: u32) -> bool {
    is_forge(ctx, forge)
        && ctx.card(legend).is_some_and(|held| {
            held.is_kind(KIND_LEGEND)
                && ctx.face_in_play(held)
                && controls_it(ctx, forge, ctx.controller(legend))
        })
}

pub fn legends_granted(ctx: &Ctx, forge: u32) -> Vec<u32> {
    let mut legends: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .map(|held| held.id)
        .filter(|legend| lends_to(ctx, forge, *legend))
        .collect();
    legends.sort_unstable();
    legends
}

pub fn lent_ability(ctx: &Ctx, forge: u32, legend: u32) -> Option<&'static Ability> {
    lends_to(ctx, forge, legend).then_some(&LENT[0])
}

pub static CARD: Card = with_statics(
    battlefield("Forge of the Fluft", &[], &[]),
    &[Static::Aura {
        scope: Scope::Legend,
        when: lends_to,
        grants: &[Grant::Ability(&LENT)],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::legend;
    use crate::cards::{script_of, Resolved, SelfCost, Trigger, GRANTED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, attach, priority, prompts};
    use crate::state::{GameBlob, ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const FORGE: u32 = fixtures::GROUNDS;
    const LEGEND: u32 = fixtures::LEGEND_CARD;
    const BOOTS: u32 = fixtures::HAND_GEAR;
    const SECOND_BOOTS: u32 = 90;
    const TURRET: u32 = 91;
    const JINX: u32 = 92;
    const THEIR_LEGEND: u32 = 93;
    const THEIR_BOOTS: u32 = 94;

    static LENT_LILLIA: Card = legend("Lillia - Bashful Bloom", &[], &LENT);

    fn forge_held_by(seat: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(FORGE).unwrap().name = "Forge of the Fluft".into();
        fixture.table.card_mut(BOOTS).unwrap().zone = Some(fixtures::BASE);
        fixture.table.cards.push(fixtures::gear(
            SECOND_BOOTS,
            fixtures::BASE,
            0,
            "Boots of Swiftness",
            2,
        ));
        fixture
            .table
            .cards
            .push(fixtures::gear(TURRET, fixtures::BASE, 0, "Turret", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(JINX, fixtures::BASE, 0, "Jinx", 2));
        fixture.table.cards.push(fixtures::card(
            THEIR_LEGEND,
            fixtures::LEGEND,
            1,
            "Kha'Zix - Voidreaver",
            KIND_LEGEND,
        ));
        fixture.table.cards.push(fixtures::gear(
            THEIR_BOOTS,
            fixtures::BASE,
            1,
            "Boots of Swiftness",
            2,
        ));
        fixture.blob.set_holder(fixtures::BF1, seat);
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FORGE).unwrap(), &CARD));
        fixture
    }

    fn lent(seat: Option<u8>) -> Fixture {
        let mut fixture = forge_held_by(seat);
        fixture.scripts = fixture.scripts.clone().with_script(LEGEND, &LENT_LILLIA);
        fixture
    }

    fn card_label(card: u32) -> String {
        format!("{{card {card}}}")
    }

    #[test]
    fn the_forge_is_a_blank_battlefield_beside_the_ability_it_lends() {
        assert!(std::ptr::eq(
            script_of("Forge of the Fluft").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::Legend,
                grants: [Grant::Ability(lent)],
                ..
            }] if std::ptr::eq(*lent, &LENT)
        ));
        assert!(CARD.replacement.is_none());
        assert_eq!(
            ATTACH_AN_EQUIPMENT.trigger,
            Trigger::Activated(Timing::Sorcery)
        );
        assert_eq!(ATTACH_AN_EQUIPMENT.cost, Some(Cost::FREE));
        assert_eq!(ATTACH_AN_EQUIPMENT.self_cost, SelfCost::Exhaust);
        assert_eq!(
            ATTACH_AN_EQUIPMENT.targets,
            &[AN_EQUIPMENT_YOU_CONTROL, A_UNIT_YOU_CONTROL]
        );
        assert_eq!(AN_EQUIPMENT_YOU_CONTROL.filter, FRIENDLY_EQUIPMENT);
        assert_eq!(A_UNIT_YOU_CONTROL.filter, FRIENDLY_UNIT);
        assert_eq!(ATTACH_AN_EQUIPMENT.label, Some("attach an Equipment"));
        assert!(ATTACH_AN_EQUIPMENT.condition.is_none());
    }

    #[test]
    fn while_you_control_it_your_legends_are_lent_the_ability_and_nobody_elses_are() {
        let mut fixture = forge_held_by(Some(0));
        let ctx = fixture.ctx();
        assert!(controls_it(&ctx, FORGE, 0));
        assert!(is_forge(&ctx, FORGE));
        assert!(!is_forge(&ctx, fixtures::ROCKFALL));
        assert!(lends_to(&ctx, FORGE, LEGEND));
        assert!(
            !lends_to(&ctx, FORGE, THEIR_LEGEND),
            "the opponent does not hold it"
        );
        assert!(!lends_to(&ctx, FORGE, fixtures::VI), "legends only");
        assert!(!lends_to(&ctx, FORGE, fixtures::CHAMPION_CARD));
        assert_eq!(legends_granted(&ctx, FORGE), [LEGEND]);
        assert!(std::ptr::eq(
            lent_ability(&ctx, FORGE, LEGEND).unwrap(),
            &LENT[0]
        ));
        assert_eq!(LENT[0].label, ATTACH_AN_EQUIPMENT.label);
        assert!(lent_ability(&ctx, FORGE, THEIR_LEGEND).is_none());
        drop(ctx);
        let mut theirs = forge_held_by(Some(1));
        let ctx = theirs.ctx();
        assert_eq!(legends_granted(&ctx, FORGE), [THEIR_LEGEND]);
        assert!(!lends_to(&ctx, FORGE, LEGEND));
        drop(ctx);
        let mut nobody = forge_held_by(None);
        let ctx = nobody.ctx();
        assert!(legends_granted(&ctx, FORGE).is_empty());
        assert!(
            legends_granted(&ctx, fixtures::ROCKFALL).is_empty(),
            "Rockfall Path lends nothing"
        );
    }

    #[test]
    fn a_legend_carrying_the_ability_exhausts_to_attach_a_friendly_equipment_to_a_friendly_unit() {
        let mut fixture = lent(Some(0));
        let mut ctx = fixture.ctx();
        assert!(activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == LEGEND && offer.label.contains("attach an Equipment")));
        activate::activate(&mut ctx, 0, LEGEND, 0).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                card_label(BOOTS),
                card_label(SECOND_BOOTS),
                "cancel".to_string()
            ],
            "friendly Equipment only: the Turret and the enemy Boots are not offered"
        );
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, &card_label(BOOTS)).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 1, .. })
        ));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                card_label(fixtures::VI),
                card_label(JINX),
                "cancel".to_string()
            ],
            "friendly units anywhere"
        );
        fixtures::choose(&mut ctx, 0, &card_label(JINX)).unwrap();
        assert!(
            ctx.card(LEGEND).unwrap().exhausted,
            "the legend pays the exhaust"
        );
        assert!(ctx.blob.chain.iter().any(
            |item| matches!(item.kind, ItemKind::Ability { source, index: 0 } if source == LEGEND)
        ));
        assert!(!attach::is_attached(&ctx, BOOTS), "not before it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attach::attached_to(&ctx, BOOTS), Some(JINX));
        assert_eq!(ctx.card(BOOTS).unwrap().zone, Some(fixtures::BASE));
        assert!(!attach::is_attached(&ctx, SECOND_BOOTS));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "the ability costs nothing but the exhaust"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BOOTS}}} is attached to {{card {JINX}}}")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_attached_equipment_can_be_moved_to_another_unit_and_the_activation_is_once_per_ready() {
        let mut fixture = lent(Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            attach::attach(&mut ctx, BOOTS, fixtures::VI),
            attach::Attached::Yes
        );
        activate::activate(&mut ctx, 0, LEGEND, 0).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(BOOTS)).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(JINX)).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(attach::attached_to(&ctx, BOOTS), Some(JINX));
        assert!(attach::attachments_of(&ctx, fixtures::VI).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, LEGEND, 0),
            Err(Refusal::Exhausted),
            "the legend is spent for the turn"
        );
    }

    #[test]
    fn the_opponent_an_exhausted_legend_and_a_board_without_equipment_are_refused() {
        let mut fixture = lent(Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, LEGEND, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        ctx.exhaust(LEGEND);
        assert_eq!(
            activate::activate(&mut ctx, 0, LEGEND, 0),
            Err(Refusal::Exhausted)
        );
        drop(ctx);
        let mut bare = lent(Some(0));
        bare.table
            .cards
            .retain(|card| card.id != BOOTS && card.id != SECOND_BOOTS);
        bare.resolve();
        bare.scripts = bare.scripts.clone().with_script(LEGEND, &LENT_LILLIA);
        let mut ctx = bare.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, LEGEND, 0),
            Err(Refusal::Illegal(Reason::NoLegalTargets)),
            "the Turret is gear but not an Equipment and the enemy Boots are not yours"
        );
        assert!(!ctx.card(LEGEND).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn while_the_forge_is_held_the_holders_legend_lists_and_activates_the_lent_ability() {
        let mut fixture = forge_held_by(Some(0));
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == LEGEND && offer.label.contains("attach an Equipment"))
            .expect("the lent ability on Lillia's strip");
        activate::activate(&mut ctx, 0, LEGEND, offer.index).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(BOOTS)).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(JINX)).unwrap();
        assert!(ctx.card(LEGEND).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(attach::attached_to(&ctx, BOOTS), Some(JINX));
        drop(ctx);
        let mut unheld = forge_held_by(Some(1));
        let ctx = unheld.ctx();
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == LEGEND && offer.label.contains("attach an Equipment")));
    }

    #[test]
    fn a_pending_activation_resolves_after_the_forge_changed_hands() {
        let mut fixture = forge_held_by(Some(0));
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == LEGEND && offer.label.contains("attach an Equipment"))
            .unwrap();
        activate::activate(&mut ctx, 0, LEGEND, offer.index).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(BOOTS)).unwrap();
        fixtures::choose(&mut ctx, 0, &card_label(JINX)).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Lent { holder, lender, index: GRANTED } if holder == LEGEND && lender == FORGE
        ));
        priority::pass(&mut ctx, 0).unwrap();
        ctx.blob.set_holder(fixtures::BF1, Some(1));
        assert!(!lends_to(&ctx, FORGE, LEGEND));
        assert!(activate::lent_at(&ctx, LEGEND, GRANTED).is_none());
        let saved_item = ctx.blob.chain[0].clone();
        let saved_table = ctx.table.clone();
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        fixture.table = saved_table;
        fixture.blob = GameBlob::decode(&saved_blob).unwrap();
        fixture.scripts = Resolved::of(&fixture.table);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.blob.chain[0], saved_item);
        assert!(activate::lent_at(&ctx, LEGEND, GRANTED).is_none());
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            attach::attached_to(&ctx, BOOTS),
            Some(JINX),
            "727.1.c.3.a: the pending item proceeds as normal once the ability is gone"
        );
        assert!(ctx.fault.is_none());
    }
}
