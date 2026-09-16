use super::prelude::{done, draw, on_hold_me, unit, with_statics};
use super::{Card, Keyword, Static};
use crate::engine::ctx::Ctx;

pub const HOLD_DRAW: usize = 1;

fn outmuscled(ctx: &Ctx, me: u32, unit: u32) -> bool {
    ctx.location(unit) == ctx.location(me)
        && ctx.controller(unit) != ctx.controller(me)
        && ctx.current_might(unit) < ctx.current_might(me)
}

pub static CARD: Card = with_statics(
    unit(
        "Vilemaw",
        &[Keyword::Ambush],
        &[on_hold_me(&[], |ctx, item, _| {
            draw(ctx, item.controller, HOLD_DRAW);
            done()
        })],
    ),
    &[Static::NoCombatDamageFrom(outmuscled)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Trigger, Who};
    use crate::engine::combat;
    use crate::engine::ctx::{Event, Location, MoveCause, Moved, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cleanup, priority, settle, showdown};
    use crate::rules::COUNTER_POINTS;
    use crate::state::{GameBlob, Mode, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::Target;

    const VILEMAW: u32 = 90;
    const SEVEN: u32 = 91;
    const EIGHT: u32 = 92;
    const FRIEND: u32 = 93;

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture.table.cards.retain(|card| {
            ![fixtures::VI, fixtures::SPRITE, fixtures::THEIR_UNIT].contains(&card.id)
        });
        fixture
            .table
            .cards
            .push(fixtures::unit(VILEMAW, fixtures::BF1, 0, "Vilemaw", 8));
        fixture
    }

    fn damage_of(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_prints_ambush_the_suppression_and_the_hold_draw() {
        let mut fixture = arena();
        fixture.resolve();
        let script = fixture.scripts.of_card(VILEMAW).unwrap();
        assert!(std::ptr::eq(script, &CARD));
        assert_eq!(CARD.name, "Vilemaw");
        assert_eq!(CARD.keywords, [Keyword::Ambush]);
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Hold(Who::Me));
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(!CARD.abilities[0].optional);
        assert_eq!(CARD.statics.len(), 1);
        assert!(matches!(CARD.statics[0], Static::NoCombatDamageFrom(_)));
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
    }

    #[test]
    fn an_enemy_with_less_might_here_contributes_nothing_and_an_equal_one_its_full_might() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(fixtures::unit(SEVEN, fixtures::BF1, 1, "Seven", 7));
        fixture
            .table
            .cards
            .push(fixtures::unit(EIGHT, fixtures::BF1, 1, "Eight", 8));
        fixture
            .table
            .cards
            .push(fixtures::unit(FRIEND, fixtures::BF1, 0, "Friend", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(!ctx.deals_combat_damage(SEVEN));
        assert!(ctx.deals_combat_damage(EIGHT), "less than, not at most");
        assert!(
            ctx.deals_combat_damage(FRIEND),
            "a friendly unit is not an enemy"
        );
        assert!(ctx.deals_combat_damage(VILEMAW));
        assert_eq!(combat::might_sum(&ctx, &[SEVEN, EIGHT]), 8);
        assert_eq!(combat::might_sum(&ctx, &[SEVEN]), 0);
        assert_eq!(
            combat::lethal(&ctx, SEVEN),
            7,
            "it still needs its full lethal"
        );
        assert_eq!(
            ctx.move_unit(
                SEVEN,
                Location::Battlefield(fixtures::BF2),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        assert!(
            ctx.deals_combat_damage(SEVEN),
            "here means Vilemaw's location"
        );
        ctx.blob
            .card_state_mut(VILEMAW)
            .set(crate::state::FLAG_STUNNED, true);
        assert!(
            !ctx.deals_combat_damage(VILEMAW),
            "410.1.b · the stun folds in"
        );
        assert!(
            ctx.deals_combat_damage(EIGHT),
            "a stunned Vilemaw still compares"
        );
    }

    #[test]
    fn a_weaker_defender_deals_no_combat_damage_and_vilemaw_conquers_unharmed() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(fixtures::unit(SEVEN, fixtures::BF1, 1, "Seven", 7));
        fixture.resolve();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        let open = ctx.blob.showdown.clone().expect("a combat opens");
        assert!(open.combat);
        assert_eq!(
            showdown::pass(&mut ctx, 1),
            Err(Refusal::NotYourFocus),
            "the attacker holds focus first"
        );
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.blob.showdown.is_none(),
            "one candidate a side needs no prompt"
        );
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 8 might vs defenders 0 might"));
        assert_eq!(
            ctx.table.card(SEVEN).and_then(|card| card.zone),
            Some(fixtures::TRASH)
        );
        assert_eq!(damage_of(&ctx, VILEMAW), 0);
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, .. } if *card == VILEMAW
        )));
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.effects.contains(&Effect::score(0, COUNTER_POINTS, 1)));
    }

    #[test]
    fn an_equal_defender_deals_its_full_might_and_a_choice_off_the_list_is_refused() {
        let mut fixture = arena();
        fixture
            .table
            .cards
            .push(fixtures::unit(FRIEND, fixtures::BF1, 0, "Friend", 2));
        fixture
            .table
            .cards
            .push(fixtures::unit(EIGHT, fixtures::BF1, 1, "Eight", 8));
        fixture.resolve();
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        showdown::pass(&mut ctx, 0).unwrap();
        showdown::pass(&mut ctx, 1).unwrap();
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "attackers 10 might vs defenders 8 might"));
        assert_eq!(ctx.blob.why, Some(PromptWhy::Assign));
        assert_eq!(combat::assigner(&ctx), Some((1, 8)));
        assert_eq!(combat::candidates(&ctx), [VILEMAW, FRIEND]);
        ctx.blob.close_prompt();
        assert_eq!(
            combat::choose(&mut ctx, EIGHT),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the defender cannot assign its might to its own unit"
        );
        combat::choose(&mut ctx, VILEMAW).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(
            ctx.table.card(VILEMAW).and_then(|card| card.zone),
            Some(fixtures::TRASH),
            "eight damage is lethal on eight might"
        );
        assert_eq!(
            ctx.table.card(EIGHT).and_then(|card| card.zone),
            Some(fixtures::TRASH)
        );
        assert_eq!(
            ctx.table.card(FRIEND).and_then(|card| card.zone),
            Some(fixtures::BF1)
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
    }

    #[test]
    fn holding_with_vilemaw_draws_one_after_the_trigger_resolves() {
        let mut fixture = arena();
        fixture.resolve();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let before = ctx.hand_of(0).len();
        assert_eq!(cleanup::score_holds(&mut ctx, 0), [fixtures::BF1]);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Held { zone, seat: 0, units } if *zone == fixtures::BF1 && units == &vec![VILEMAW]
        )));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            crate::state::ItemKind::Trigger { source, index: 0 } if source == VILEMAW
        ));
        assert_eq!(ctx.hand_of(0).len(), before, "the draw waits for the chain");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), before + HOLD_DRAW);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert_eq!(ctx.points(0), 1);
    }
}
