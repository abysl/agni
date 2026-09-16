use super::prelude::{
    card_target, done, heal, location_of, move_destinations, move_unit, play, unit, Location,
    ENEMY_UNIT_HERE,
};
use super::{Card, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

const ENEMY_HERE: TargetSpec = TargetSpec {
    filter: ENEMY_UNIT_HERE,
    min: 1,
    max: 1,
    kind: TargetKind::Card,
    label: "an enemy unit here to move home",
    min_at_level: None,
};

fn save(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let seat = item.controller;
    if let Some(here) = location_of(ctx, me) {
        let friendly: Vec<u32> = ctx
            .units_at(here)
            .into_iter()
            .filter(|unit| ctx.controller(*unit) == seat)
            .collect();
        for unit in friendly {
            if heal(ctx, unit) {
                ctx.narrate(format!("{{card {unit}}} is healed"));
            }
        }
    }
    if let Some(enemy) = card_target(ctx, item, 0) {
        let home = Location::Base(ctx.controller(enemy));
        if move_destinations(ctx, enemy).contains(&home) {
            move_unit(ctx, item, enemy, home);
        } else {
            ctx.narrate(format!("{{card {enemy}}} can't move to its base"));
        }
    }
    done()
}

pub static CARD: Card = unit(
    "Janna - Savior",
    &[Keyword::Reaction],
    &[play(&[ENEMY_HERE], save)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::ctx::{EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent, Reason};
    use crate::engine::{act, settle};
    use crate::state::{ItemKind, Origin, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::{CounterInfo, Target};

    const JANNA: u32 = 90;
    const ALLY: u32 = 91;
    const ENEMY: u32 = 92;
    const PLAIN: u32 = 54;

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().zone = Some(fixtures::BF3);
        fixture.table.cards.push(fixtures::card(
            PLAIN,
            fixtures::BF2,
            0,
            "Plain Field",
            "Battlefield",
        ));
        let mut janna = fixtures::unit(JANNA, fixtures::HAND, 0, "Janna - Savior", 3);
        janna.energy = Some(0);
        janna.domain = vec!["Calm".into()];
        fixture.table.cards.push(janna);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(ALLY, fixtures::BF1, 0, "Ally", 4));
        fixture
            .table
            .cards
            .push(fixtures::unit(ENEMY, fixtures::BF1, 1, "Jinx", 2));
        for (card, damage) in [(fixtures::VI, 2), (ALLY, 1), (fixtures::THEIR_UNIT, 1)] {
            fixture.table.counters.push(CounterInfo {
                target: Target::Card(card),
                counter: crate::engine::ctx::COUNTER_DAMAGE,
                value: damage,
            });
        }
        fixture.table.counters.sort();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, to: u16) -> EntryMove {
        EntryMove {
            card: JANNA,
            from: ctx.zones.hand,
            from_seat: 0,
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn played_as_a_reaction_to_a_held_battlefield_she_heals_her_side_there_and_sends_the_enemy_home(
    ) {
        assert!(std::ptr::eq(script_of("Janna - Savior").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Reaction));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Ok(Intent::Play {
                card: JANNA,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            }),
            "813.3.a: a Reaction unit is played mid-chain to a battlefield you control"
        );
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF2)),
            Err(Refusal::Illegal(Reason::NotHeld))
        );
        ctx.table
            .apply_entry(&fixtures::move_action(JANNA, fixtures::BF1, 0), 0)
            .unwrap();
        act(
            &mut ctx,
            0,
            Intent::Play {
                card: JANNA,
                origin: Origin::Hand,
                location: Some(Location::Battlefield(fixtures::BF1)),
                on_chain: false,
            },
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "the one enemy here is picked on its own"
        );
        let top = ctx
            .blob
            .chain
            .last()
            .expect("her trigger waits above the spell");
        assert!(matches!(top.kind, ItemKind::Trigger { source, .. } if source == JANNA));
        assert_eq!(top.targets, [TargetRef::Card(ENEMY)]);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert_eq!(ctx.damage_on(ALLY), 0);
        assert_eq!(
            ctx.damage_on(fixtures::THEIR_UNIT),
            1,
            "the unit at base is not here"
        );
        assert_eq!(ctx.location(ENEMY), Some(Location::Base(1)));
        assert_eq!(ctx.blob.contester(fixtures::BF1), None);
    }

    #[test]
    fn with_no_enemy_here_the_trigger_is_dropped_and_a_sorcery_unit_is_refused_mid_chain() {
        let mut fixture = armed();
        fixture.table.card_mut(ENEMY).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(ENEMY).unwrap().seat = 1;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.table
            .apply_entry(&fixtures::move_action(JANNA, fixtures::BF1, 0), 0)
            .unwrap();
        crate::engine::play::begin(
            &mut ctx,
            0,
            JANNA,
            Origin::Hand,
            Some(Location::Battlefield(fixtures::BF1)),
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.damage_on(fixtures::VI),
            2,
            "no enemy here, no heal either"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
        let mut plain = armed();
        plain.table.card_mut(JANNA).unwrap().name = "Ally".into();
        plain.resolve();
        let mut ctx = plain.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, fixtures::BF1)),
            Err(Refusal::Illegal(Reason::ClosedTiming))
        );
    }
}
