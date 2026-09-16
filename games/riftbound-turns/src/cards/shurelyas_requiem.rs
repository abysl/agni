use super::prelude::{
    done, equip, friendly_units, gear, play, ready, while_attached, with_statics, RAINBOW,
};
use super::{Card, Cost, Flow, Grant, Item, Keyword, Scope, Stage, Static};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = RAINBOW;

pub const MIGHT_BONUS: i16 = 2;

pub fn wearers_controller_owns(ctx: &Ctx, wearer: u32, unit: u32) -> bool {
    ctx.controller(unit) == ctx.controller(wearer)
}

pub static AURA: Static = Static::Aura {
    scope: Scope::UnitsHere,
    when: wearers_controller_owns,
    grants: &[Grant::Keyword(Keyword::Ganking)],
};

pub static EFFECT_TEXT: &[Grant] = &[Grant::Static(AURA), Grant::Might(MIGHT_BONUS)];

fn requiem(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let readied = friendly_units(ctx, seat)
        .into_iter()
        .filter(|unit| ready(ctx, *unit))
        .count();
    ctx.narrate(format!("{{seat {seat}}} readies {readied} units"));
    done()
}

pub static CARD: Card = with_statics(
    gear(
        "Shurelya's Requiem",
        &[Keyword::Equip(EQUIP)],
        &[play(&[], requiem), equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::{Ctx, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, attach, march, play as play_engine, priority};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const REQUIEM: u32 = 90;
    const WAILER: u32 = 95;
    const PLAY: u8 = 0;
    const EQUIP_INDEX: u8 = 1;

    fn requiem_card(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            power: Some(2),
            domain: vec!["Calm".into(), "Mind".into()],
            ..fixtures::gear(REQUIEM, zone, seat, "Shurelya's Requiem", 4)
        }
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(requiem_card(zone, 0));
        fixture
            .table
            .cards
            .push(fixtures::unit(WAILER, fixtures::BF1, 0, "Wailer", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Calm", false));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(REQUIEM).unwrap(),
            &CARD
        ));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn equip_wailer(ctx: &mut Ctx) {
        activate::activate(ctx, 0, REQUIEM, EQUIP_INDEX).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(ctx, 0, "{card 95}").unwrap();
        resolve_chain(ctx);
    }

    #[test]
    fn the_script_is_a_rainbow_equipment_with_a_ready_all_play_trigger_and_a_ganking_aura_for_units_here(
    ) {
        assert!(std::ptr::eq(
            script_of("Shurelya's Requiem").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Shurelya's Requiem");
        assert_eq!(CARD.keywords, [Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(RAINBOW));
        assert!(
            !CARD.has_aura(),
            "the aura is effect text: it belongs to the wearer, not to the loose gear"
        );
        assert_eq!(CARD.abilities.len(), 2);
        let play = &CARD.abilities[usize::from(PLAY)];
        assert_eq!(play.trigger, Trigger::Play);
        assert!(play.targets.is_empty());
        assert!(!play.optional);
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(RAINBOW));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        let [Grant::Static(Static::Aura { scope, grants, .. }), Grant::Might(2)] =
            CARD.attached_grants()
        else {
            panic!("the effect text is the aura then the bonus");
        };
        assert_eq!(*scope, Scope::UnitsHere);
        assert!(matches!(grants, [Grant::Keyword(Keyword::Ganking)]));
        assert_eq!(MIGHT_BONUS, 2);
    }

    #[test]
    fn playing_it_readies_every_unit_you_control_and_none_of_theirs() {
        let mut fixture = armed(fixtures::HAND);
        for unit in [fixtures::VI, WAILER, fixtures::THEIR_UNIT] {
            fixture.table.card_mut(unit).unwrap().exhausted = true;
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, REQUIEM).unwrap();
        assert_eq!(ctx.location(REQUIEM), Some(Location::Base(0)));
        assert_eq!(ctx.runes_of(0).len(), 4, "two runes recycled for the power");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: PLAY } if source == REQUIEM
        ));
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "nothing until the trigger resolves"
        );
        resolve_chain(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        assert!(!ctx.card(WAILER).unwrap().exhausted);
        assert!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().exhausted,
            "your units only"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} readies 2 units".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn equipping_costs_any_rune_gives_plus_two_and_the_aura_predicate_reads_the_wearers_controller()
    {
        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        equip_wailer(&mut ctx);
        assert_eq!(attached_to(&ctx, REQUIEM), Some(WAILER));
        assert_eq!(ctx.runes_of(0).len(), 5, "one rune of any domain recycled");
        assert_eq!(ctx.current_might(WAILER), 4);
        assert_eq!(
            ctx.location(REQUIEM),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(wearers_controller_owns(&ctx, WAILER, fixtures::VI));
        assert!(wearers_controller_owns(&ctx, WAILER, WAILER));
        assert!(!wearers_controller_owns(&ctx, WAILER, fixtures::THEIR_UNIT));
        assert!(
            attach::has_static(&ctx, WAILER, AURA),
            "the wearer carries the aura as a granted static"
        );
        ctx.detach(REQUIEM);
        assert_eq!(ctx.current_might(WAILER), 2);
        assert!(!attach::has_static(&ctx, WAILER, AURA));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_an_enemy_wearer_and_an_attached_requiem_are_refused() {
        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, REQUIEM, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, REQUIEM, EQUIP_INDEX).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.runes_of(0).len(), 6);
        equip_wailer(&mut ctx);
        assert_eq!(
            activate::activate(&mut ctx, 0, REQUIEM, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
    }

    #[test]
    fn while_attached_your_units_at_the_wearers_battlefield_have_ganking() {
        let mut fixture = armed(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        equip_wailer(&mut ctx);
        assert!(ctx.has_keyword(WAILER, Keyword::Ganking));
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Ganking));
        assert!(!ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Ganking));
        assert!(!ctx.has_keyword(fixtures::SPRITE, Keyword::Ganking));
        assert_eq!(
            march::legal_destination(
                &ctx,
                fixtures::VI,
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ),
            Ok(())
        );
        ctx.detach(REQUIEM);
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Ganking));
    }
}
