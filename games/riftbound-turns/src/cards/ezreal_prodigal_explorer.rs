use super::prelude::{done, draw, exhausting_self, legend, named, reaction, usable_if};
use super::{Card, Cost, Flow, Item, Source, Stage};
use crate::engine::ctx::{Ctx, Event};
use crate::state::ItemKind;

pub const DRAWS: usize = 1;
pub const CHOICES_NEEDED: usize = 2;

fn by_a_spell_or_a_unit_ability(ctx: &Ctx, item: u16) -> bool {
    match ctx.chain_item(item).map(|held| held.kind) {
        Some(ItemKind::Spell { .. }) => true,
        Some(
            ItemKind::Ability { source, .. }
            | ItemKind::Trigger { source, .. }
            | ItemKind::Granted { holder: source, .. }
            | ItemKind::Lent { holder: source, .. },
        ) => ctx.is_unit(source),
        Some(ItemKind::Permanent { .. }) => false,
        None => true,
    }
}

pub fn enemy_choices_this_turn(ctx: &Ctx, seat: u8) -> usize {
    ctx.events
        .iter()
        .filter(|event| match event {
            Event::Chosen { card, by, item } => {
                *by == seat
                    && (ctx.is_unit(*card) || ctx.is_gear(*card))
                    && ctx.controller(*card) != seat
                    && by_a_spell_or_a_unit_ability(ctx, *item)
            }
            _ => false,
        })
        .count()
}

fn chosen_enemies_twice(ctx: &Ctx, source: Source) -> bool {
    enemy_choices_this_turn(ctx, ctx.controller(source.card)) >= CHOICES_NEEDED
}

fn explore(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = legend(
    "Ezreal - Prodigal Explorer",
    &[],
    &[named(
        usable_if(
            exhausting_self(reaction(Cost::FREE, &[], explore)),
            chosen_enemies_twice,
        ),
        "draw 1",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spell, target, ENEMY_UNIT, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, TargetKind, Timing, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::ItemKind;
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const EZREAL: u32 = fixtures::LEGEND_CARD;
    const VOLLEY: u32 = 90;
    const RALLY: u32 = 91;
    const SECOND_VOLLEY: u32 = 92;

    static VOLLEY_CARD: Card = spell(
        "Volley",
        &[],
        &[crate::cards::prelude::play(
            &[target(
                ENEMY_UNIT,
                1,
                2,
                TargetKind::Card,
                "one or two enemy units",
            )],
            |_, _, _| Flow::Done,
        )],
    );

    static RALLY_CARD: Card = spell(
        "Rally",
        &[],
        &[crate::cards::prelude::play(
            &[target(
                FRIENDLY_UNIT,
                1,
                2,
                TargetKind::Card,
                "one or two friendly units",
            )],
            |_, _, _| Flow::Done,
        )],
    );

    fn volley(id: u32, name: &str) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, 0, name, 0, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn expedition() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(EZREAL).unwrap().name = CARD.name.into();
        fixture.table.cards.push(volley(VOLLEY, "Volley"));
        fixture.table.cards.push(volley(SECOND_VOLLEY, "Volley"));
        fixture.table.cards.push(volley(RALLY, "Rally"));
        fixture
            .table
            .cards
            .push(fixtures::unit(93, fixtures::BASE, 0, "Scout", 1));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(VOLLEY, &VOLLEY_CARD)
            .with_script(SECOND_VOLLEY, &VOLLEY_CARD)
            .with_script(RALLY, &RALLY_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(EZREAL).unwrap(),
            &CARD
        ));
        fixture
    }

    fn cast(ctx: &mut Ctx, card: u32, picks: &[u32]) {
        fixtures::play_from_hand(ctx, 0, card).unwrap();
        for pick in picks {
            fixtures::choose(ctx, 0, &format!("{{card {pick}}}")).unwrap();
        }
        if ctx.blob.prompt.is_some() {
            fixtures::choose(ctx, 0, "done").unwrap();
        }
    }

    fn resolve_top(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_legend_has_one_free_reaction_that_exhausts_him_behind_a_usable_gate() {
        assert!(std::ptr::eq(script_of(CARD.name).unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Reaction));
        assert_eq!(ability.cost, Some(Cost::FREE));
        assert_eq!(ability.self_cost, SelfCost::Exhaust);
        assert_eq!(ability.label, Some("draw 1"));
        assert!(ability.usable.is_some());
        assert!(ability.targets.is_empty());
        assert_eq!(CHOICES_NEEDED, 2);
    }

    #[test]
    fn the_gate_counts_enemy_units_and_gear_chosen_by_his_controllers_spells_and_unit_abilities() {
        let mut fixture = expedition();
        let mut ctx = fixture.ctx();
        assert_eq!(enemy_choices_this_turn(&ctx, 0), 0);
        assert_eq!(
            activate::activate(&mut ctx, 0, EZREAL, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "nothing chosen yet"
        );
        cast(&mut ctx, RALLY, &[fixtures::VI, 93]);
        resolve_top(&mut ctx);
        assert_eq!(
            enemy_choices_this_turn(&ctx, 0),
            0,
            "friendly units are not enemy units"
        );
        cast(&mut ctx, VOLLEY, &[fixtures::THEIR_UNIT]);
        resolve_top(&mut ctx);
        assert_eq!(enemy_choices_this_turn(&ctx, 0), 1);
        assert_eq!(
            activate::activate(&mut ctx, 0, EZREAL, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "once is not twice"
        );
        assert_eq!(enemy_choices_this_turn(&ctx, 1), 0, "seat 1 chose nothing");
        ctx.raise(Event::Chosen {
            card: fixtures::THEIR_UNIT,
            by: 0,
            item: 99,
        });
        assert_eq!(
            enemy_choices_this_turn(&ctx, 0),
            2,
            "an item that has already left the chain is taken as a spell's"
        );
        ctx.raise(Event::Chosen {
            card: fixtures::SPRITE,
            by: 1,
            item: 99,
        });
        assert_eq!(enemy_choices_this_turn(&ctx, 0), 2, "not his choice");
        ctx.raise(Event::Chosen {
            card: fixtures::ROCKFALL,
            by: 0,
            item: 99,
        });
        assert_eq!(
            enemy_choices_this_turn(&ctx, 0),
            2,
            "a battlefield is neither a unit nor gear"
        );
    }

    #[test]
    fn a_legend_ability_choosing_an_enemy_is_not_a_spell_or_unit_ability() {
        let mut fixture = expedition();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, VOLLEY, &[fixtures::THEIR_UNIT]);
        let spell = ctx.blob.chain[0].id;
        assert!(by_a_spell_or_a_unit_ability(&ctx, spell));
        let legend_item = crate::state::ChainItem::new(
            50,
            ItemKind::Ability {
                source: EZREAL,
                index: 0,
            },
            0,
            crate::state::Origin::Board,
        );
        ctx.blob.chain.push(legend_item);
        assert!(!by_a_spell_or_a_unit_ability(&ctx, 50));
        ctx.raise(Event::Chosen {
            card: fixtures::SPRITE,
            by: 0,
            item: 50,
        });
        assert_eq!(
            enemy_choices_this_turn(&ctx, 0),
            1,
            "the legend's own choice is not counted"
        );
        ctx.blob.chain.pop();
        let unit_item = crate::state::ChainItem::new(
            51,
            ItemKind::Ability {
                source: fixtures::VI,
                index: 0,
            },
            0,
            crate::state::Origin::Board,
        );
        ctx.blob.chain.push(unit_item);
        assert!(by_a_spell_or_a_unit_ability(&ctx, 51));
    }

    #[test]
    fn after_two_enemy_choices_he_reacts_on_the_open_chain_exhausts_and_draws_when_it_resolves() {
        let mut fixture = expedition();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, VOLLEY, &[fixtures::THEIR_UNIT, fixtures::SPRITE]);
        assert_eq!(
            enemy_choices_this_turn(&ctx, 0),
            2,
            "two units, two choices"
        );
        assert_eq!(ctx.blob.chain.len(), 1, "the Volley waits");
        let hand = ctx.hand_of(0).len();
        let offers = activate::offers(&ctx, 0);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {EZREAL}}}: draw 1 (exhaust)")
        );
        assert!(offers[0].enabled);
        activate::activate(&mut ctx, 0, EZREAL, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.card(EZREAL).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 2, "his draw sits above the Volley");
        assert!(matches!(
            ctx.blob.chain[1].kind,
            ItemKind::Ability { source, index: 0 } if source == EZREAL
        ));
        assert_eq!(ctx.hand_of(0).len(), hand, "nothing until it resolves");
        resolve_top(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
        assert_eq!(ctx.blob.chain.len(), 1, "the Volley is still there");
        assert_eq!(
            activate::activate(&mut ctx, 0, EZREAL, 0),
            Err(Refusal::Exhausted)
        );
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_spells_on_one_enemy_each_reach_twice_and_the_wrong_seat_cannot_use_him() {
        let mut fixture = expedition();
        let mut ctx = fixture.ctx();
        cast(&mut ctx, VOLLEY, &[fixtures::THEIR_UNIT]);
        resolve_top(&mut ctx);
        cast(&mut ctx, SECOND_VOLLEY, &[fixtures::THEIR_UNIT]);
        resolve_top(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(enemy_choices_this_turn(&ctx, 0), 2);
        assert_eq!(
            activate::activate(&mut ctx, 1, EZREAL, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(activate::offers(&ctx, 1).is_empty());
        let hand = ctx.hand_of(0).len();
        activate::activate(&mut ctx, 0, EZREAL, 0).unwrap();
        assert!(
            ctx.blob.why.is_none(),
            "a reaction on an empty chain needs no prompt"
        );
        resolve_top(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand + 1);
    }

    #[test]
    #[ignore = "engine gap · per-turn counters: the blob keeps no record of enemy choices this turn, so choices in an earlier request are invisible to the gate; SeatState wants an enemy_choices counter fed by Ctx::choose and reset at Expiration"]
    fn choices_made_in_an_earlier_request_still_open_the_gate() {
        let mut fixture = expedition();
        {
            let mut ctx = fixture.ctx();
            cast(&mut ctx, VOLLEY, &[fixtures::THEIR_UNIT, fixtures::SPRITE]);
            resolve_top(&mut ctx);
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        let mut ctx = fixture.ctx();
        assert!(ctx.events.is_empty(), "a fresh request");
        assert_eq!(enemy_choices_this_turn(&ctx, 0), 2);
        activate::activate(&mut ctx, 0, EZREAL, 0).unwrap();
    }
}
