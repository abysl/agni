use super::prelude::{
    activated, done, might_this_turn, named, once_each_turn, paying_with, ready, unit, usable_if,
};
use super::{Card, Cost, Domain, Flow, Item, Power, SelfCost, Source, Stage, Timing};
use crate::engine::ctx::{Ctx, Event};

pub const ORDER: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Order)],
};
pub const MIGHT: i16 = 1;

pub fn chose_an_enemy_unit_this_turn(ctx: &Ctx, seat: u8) -> bool {
    ctx.events.iter().any(|event| match event {
        Event::Chosen { card, by, .. } => {
            *by == seat && ctx.is_unit(*card) && ctx.controller(*card) != seat
        }
        _ => false,
    })
}

fn fed(ctx: &Ctx, source: Source) -> bool {
    chose_an_enemy_unit_this_turn(ctx, ctx.controller(source.card))
}

fn feast(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        ctx.narrate(format!(
            "{{card {me}}} has left the board · nothing readies"
        ));
        return done();
    }
    ready(ctx, me);
    might_this_turn(ctx, item, me, MIGHT, None);
    ctx.narrate(format!("{{card {me}}} readies and gets +{MIGHT} this turn"));
    done()
}

pub static CARD: Card = unit(
    "Hungry Wolf",
    &[],
    &[named(
        usable_if(
            once_each_turn(paying_with(
                activated(Timing::Sorcery, ORDER, &[], feast),
                SelfCost::Free,
            )),
            fed,
        ),
        "ready me and +1 Might this turn",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{play, spell, this_turn, ENEMY_UNIT, FRIENDLY_UNIT};
    use crate::cards::{script_of, Once, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, FLAG_ONCE_USED};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const WOLF: u32 = 90;
    const BITE: u32 = 91;
    const PET: u32 = 92;
    const ORDER_RUNE: u32 = 46;

    static BITE_CARD: Card = spell(
        "Bite",
        &[],
        &[play(
            &[crate::cards::prelude::a_card(ENEMY_UNIT, "an enemy unit")],
            |_, _, _| Flow::Done,
        )],
    );

    static PET_CARD: Card = spell(
        "Pet",
        &[],
        &[play(
            &[crate::cards::prelude::a_card(
                FRIENDLY_UNIT,
                "a friendly unit",
            )],
            |_, _, _| Flow::Done,
        )],
    );

    fn wolf() -> CardInfo {
        CardInfo {
            energy: Some(4),
            domain: vec!["Order".into()],
            exhausted: true,
            ..fixtures::unit(WOLF, fixtures::BF1, 0, "Hungry Wolf", 4)
        }
    }

    fn den(with_order: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(wolf());
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        if with_order {
            fixture
                .table
                .cards
                .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        }
        let mut bite = fixtures::spell(BITE, fixtures::HAND, 0, "Bite", 0, 0);
        bite.domain = vec!["Order".into()];
        fixture.table.cards.push(bite);
        let mut pet = fixtures::spell(PET, fixtures::HAND, 0, "Pet", 0, 0);
        pet.domain = vec!["Order".into()];
        fixture.table.cards.push(pet);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BITE, &BITE_CARD)
            .with_script(PET, &PET_CARD);
        assert!(std::ptr::eq(fixture.scripts.of_card(WOLF).unwrap(), &CARD));
        fixture
    }

    fn cast(ctx: &mut Ctx, card: u32, pick: u32) {
        fixtures::play_from_hand(ctx, 0, card).unwrap();
        fixtures::choose(ctx, 0, &format!("{{card {pick}}}")).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    fn his_offer(ctx: &Ctx) -> Option<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .find(|offer| offer.source == WOLF)
    }

    #[test]
    fn the_script_is_one_order_power_sorcery_activation_gated_and_once_a_turn_that_never_exhausts_him(
    ) {
        assert!(std::ptr::eq(script_of("Hungry Wolf").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.cost, Some(ORDER));
        assert_eq!(ORDER.power, [Power::Domain(Domain::Order)]);
        assert_eq!(
            ability.self_cost,
            SelfCost::Free,
            "readying himself is the effect, so exhausting is no cost"
        );
        assert_eq!(ability.once, Once::PerTurn);
        assert!(ability.usable.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!(ability.label, Some("ready me and +1 Might this turn"));
        assert_eq!(MIGHT, 1);
    }

    #[test]
    fn the_gate_reads_an_enemy_unit_chosen_by_his_controller_this_turn() {
        let mut fixture = den(true);
        let mut ctx = fixture.ctx();
        assert!(!chose_an_enemy_unit_this_turn(&ctx, 0));
        assert_eq!(
            activate::activate(&mut ctx, 0, WOLF, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "nothing chosen yet · the engine's one gate reason"
        );
        assert!(his_offer(&ctx).is_none(), "no offer while the gate is shut");
        cast(&mut ctx, PET, fixtures::VI);
        assert!(
            !chose_an_enemy_unit_this_turn(&ctx, 0),
            "a friendly unit is not an enemy unit"
        );
        ctx.raise(Event::Chosen {
            card: fixtures::GROUNDS,
            by: 0,
            item: 99,
        });
        assert!(
            !chose_an_enemy_unit_this_turn(&ctx, 0),
            "a battlefield is not a unit"
        );
        ctx.raise(Event::Chosen {
            card: fixtures::THEIR_UNIT,
            by: 1,
            item: 99,
        });
        assert!(
            !chose_an_enemy_unit_this_turn(&ctx, 0),
            "the opponent choosing their own unit is not your choice"
        );
        assert!(
            !chose_an_enemy_unit_this_turn(&ctx, 1),
            "for the opponent their own unit is no enemy either"
        );
        cast(&mut ctx, BITE, fixtures::THEIR_UNIT);
        assert!(chose_an_enemy_unit_this_turn(&ctx, 0));
    }

    #[test]
    fn after_choosing_an_enemy_unit_an_order_power_readies_him_with_one_might_once_this_turn() {
        let mut fixture = den(true);
        let mut ctx = fixture.ctx();
        cast(&mut ctx, BITE, fixtures::THEIR_UNIT);
        assert!(chose_an_enemy_unit_this_turn(&ctx, 0));
        assert!(ctx.card(WOLF).unwrap().exhausted);
        let offer = his_offer(&ctx).expect("the gate is open");
        assert!(offer.enabled);
        assert_eq!(
            offer.label,
            format!("{{card {WOLF}}}: ready me and +1 Might this turn (1 Order power)")
        );
        let runes = ctx.runes_of(0).len();
        activate::activate(&mut ctx, 0, WOLF, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "the Order rune is recycled"
        );
        assert!(
            ctx.card(WOLF).unwrap().exhausted,
            "an exhausted wolf may use it · nothing until it resolves"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == WOLF
        ));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(WOLF).unwrap().exhausted);
        assert_eq!(ctx.current_might(WOLF), 5);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {WOLF}}} readies and gets +1 this turn")));
        assert!(ctx.has_flag(WOLF, FLAG_ONCE_USED));
        assert_eq!(
            activate::activate(&mut ctx, 0, WOLF, 0),
            Err(Refusal::Illegal(Reason::AlreadyActivated)),
            "only once each turn"
        );
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(WOLF), 4, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_an_order_power_or_from_the_other_seat_the_ability_is_refused() {
        let mut fixture = den(false);
        let mut ctx = fixture.ctx();
        cast(&mut ctx, BITE, fixtures::THEIR_UNIT);
        assert!(chose_an_enemy_unit_this_turn(&ctx, 0));
        assert_eq!(
            activate::activate(&mut ctx, 0, WOLF, 0),
            Err(Refusal::NoPowerOf)
        );
        assert!(his_offer(&ctx).is_some_and(|offer| !offer.enabled));
        assert!(ctx.card(WOLF).unwrap().exhausted);
        assert_eq!(
            activate::activate(&mut ctx, 1, WOLF, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(ctx.current_might(WOLF), 4);
    }

    #[test]
    #[ignore = "engine gap · per-turn counters (the Ezreal - Prodigal Explorer row): the blob keeps no record of enemy units chosen this turn and ctx.events is one request deep, while an activation is always its own request after the spell that chose — so the gate never opens in play and the ability is dead until the counter lands (the tests below pass only because they cast and activate inside one ctx); SeatState wants an enemy_choices counter fed by the Chosen event and reset at Expiration, and fed reads it"]
    fn a_choice_made_in_an_earlier_request_still_opens_the_gate() {
        let mut fixture = den(true);
        {
            let mut ctx = fixture.ctx();
            cast(&mut ctx, BITE, fixtures::THEIR_UNIT);
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        let mut ctx = fixture.ctx();
        assert!(ctx.events.is_empty(), "a fresh request");
        assert!(chose_an_enemy_unit_this_turn(&ctx, 0));
        activate::activate(&mut ctx, 0, WOLF, 0).unwrap();
    }
}
