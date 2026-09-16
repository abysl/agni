use super::prelude::{
    a_card, asking, card_target, charm_destination, deal, done, move_unit, play, spell,
    with_candidates, CHARM_DESTINATION, MOVABLE_ENEMY_UNIT,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const UNIT: usize = 0;
const DESTINATION: usize = 1;
const CLASH: u8 = 1;

fn might_of(ctx: &Ctx, unit: u32) -> u8 {
    u8::try_from(ctx.current_might(unit).max(0)).unwrap_or(u8::MAX)
}

fn moved_unit(ctx: &Ctx, item: &Item) -> Option<u32> {
    card_target(ctx, item, UNIT).filter(|unit| ctx.on_board(*unit))
}

fn others_at_its_destination(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let Some(moved) = moved_unit(ctx, item) else {
        return Vec::new();
    };
    let Some(at) = ctx.location(moved) else {
        return Vec::new();
    };
    ctx.units_at(at)
        .into_iter()
        .filter(|unit| *unit != moved && ctx.controller(*unit) != item.controller)
        .map(TargetRef::Card)
        .collect()
}

fn rage(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == CLASH {
        return clash(ctx, item);
    }
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    if let Some(to) = charm_destination(ctx, item, unit, DESTINATION) {
        move_unit(ctx, item, unit, to);
    }
    if others_at_its_destination(ctx, item, stage).is_empty() {
        ctx.narrate(format!("no other enemy unit stands with {{card {unit}}}"));
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, CLASH, 1, 1))
}

fn clash(ctx: &mut Ctx, item: &Item) -> Flow {
    let Some(moved) = moved_unit(ctx, item) else {
        return done();
    };
    let Some(other) = ctx.picks().first().copied() else {
        return done();
    };
    if !others_at_its_destination(ctx, item, Stage(CLASH)).contains(&TargetRef::Card(other)) {
        return done();
    }
    let to_other = might_of(ctx, moved);
    let to_moved = might_of(ctx, other);
    ctx.narrate(format!(
        "{{card {moved}}} and {{card {other}}} deal {to_other} and {to_moved} damage to each other"
    ));
    deal(ctx, item, other, to_other);
    deal(ctx, item, moved, to_moved);
    done()
}

pub static CARD: Card = spell(
    "Dragon's Rage",
    &[],
    &[asking(
        with_candidates(
            play(
                &[
                    a_card(MOVABLE_ENEMY_UNIT, "an enemy unit"),
                    CHARM_DESTINATION,
                ],
                rage,
            ),
            others_at_its_destination,
        ),
        "another enemy unit at its destination",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{Filter, Keyword, TargetKind, Trigger};
    use crate::engine::ctx::{EntryMove, Event, Location, MoveCause, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, prompts, settle};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const RAGE: u32 = 90;
    const THEIR_RAGE: u32 = 91;
    const BRUTE: u32 = 92;
    const RUNT: u32 = 93;
    const CALM_RUNE: u32 = 100;

    fn rage_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Dragon's Rage", 4, 1);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(rage_card(RAGE, 0));
        fixture.table.cards.push(rage_card(THEIR_RAGE, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BASE, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(RUNT, fixtures::BASE, 1, "Runt", 1));
        fixture.resolve();
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        settle(ctx).unwrap();
    }

    fn damage(ctx: &Ctx, unit: u32) -> i32 {
        ctx.table
            .counter(Target::Card(unit), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt { card, n, .. } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_script_moves_one_enemy_unit_and_asks_for_the_clash_partner_at_resolution() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Dragon's Rage").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Dragon's Rage");
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, MOVABLE_ENEMY_UNIT);
        assert_eq!(ability.targets[1], CHARM_DESTINATION);
        assert_eq!(ability.targets[1].kind, TargetKind::Zone);
        assert_eq!(ability.targets[1].filter, Filter::DifferentLocationFrom(0));
        assert!(
            ability.candidates.is_some(),
            "the second unit is chosen where the first one lands"
        );
        assert_eq!(
            ability.question,
            Some("another enemy unit at its destination")
        );
    }

    #[test]
    fn the_moved_unit_and_a_chosen_neighbour_deal_their_mights_to_each_other() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAGE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 92}", "{card 93}", "cancel"],
            "the enemy units"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "{zone 9}", "cancel"],
            "its own base or the other battlefield, chosen at play (352.3)"
        );
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::SPRITE),
                TargetRef::Zone(fixtures::BASE)
            ]
        );
        assert!(
            ctx.blob.prompt.is_none(),
            "the clash partner waits for resolution"
        );
        both_pass(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Base(1)),
            "the Sprite went home first"
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: fixtures::SPRITE,
            from: Some(Location::Battlefield(fixtures::BF2)),
            to: Location::Base(1),
            cause: MoveCause::Effect,
            by: Some(0)
        }));
        let item = ctx.blob.chain[0].id;
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item, stage: 1 }));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}", "{card 92}", "{card 93}"],
            "the other enemy units standing where it landed"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item, stage: 1 }),
            "{card 90}: choose another enemy unit at its destination (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            damage_events(&ctx),
            [(BRUTE, 3), (fixtures::SPRITE, 5)],
            "the Sprite's 3 to the Brute, the Brute's 5 to the Sprite"
        );
        assert_eq!(damage(&ctx, BRUTE), 3);
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "5 damage on a 3-Might token is lethal at the cleanup"
        );
        assert!(ctx.on_board(BRUTE), "3 damage on 5 Might is not");
        assert_eq!(damage(&ctx, fixtures::THEIR_UNIT), 0);
        assert_eq!(damage(&ctx, RUNT), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} and {card 92} deal 3 and 5 damage to each other".to_string()));
        assert_eq!(ctx.card(RAGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn a_lone_unit_at_its_destination_gets_the_one_neighbour_unasked_and_both_can_die() {
        let mut fixture = armed();
        fixture.table.card_mut(RUNT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        both_pass(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "the Runt is the only other enemy there: chosen unasked"
        );
        assert_eq!(damage_events(&ctx), [(RUNT, 3), (fixtures::SPRITE, 1)]);
        assert_eq!(
            ctx.card(RUNT).unwrap().zone,
            Some(fixtures::TRASH),
            "3 on a 1-Might unit is lethal"
        );
        assert!(
            ctx.on_board(fixtures::SPRITE),
            "1 on a 3-Might Sprite is not"
        );
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(damage(&ctx, fixtures::SPRITE), 1);
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn with_nobody_else_at_the_destination_the_move_still_happens_and_nothing_clashes() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        both_pass(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.location(fixtures::SPRITE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(damage_events(&ctx).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"no other enemy unit stands with {card 60}".to_string()));
        assert_eq!(ctx.card(RAGE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn a_unit_that_left_the_board_moves_nothing_and_a_friendly_neighbour_is_never_offered() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        ctx.table
            .apply_entry(
                &fixtures::move_action(fixtures::THEIR_UNIT, fixtures::TRASH, 1),
                1,
            )
            .unwrap();
        both_pass(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Moved { .. })));
        assert!(damage_events(&ctx).is_empty());
        assert_eq!(ctx.card(RAGE).unwrap().zone, Some(fixtures::TRASH));
        drop(ctx);

        let mut fixture = armed();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        both_pass(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "Vi stands there but she is the caster's: no enemy neighbour"
        );
        assert!(damage_events(&ctx).is_empty());
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            ctx.blob
                .staged
                .iter()
                .any(|staged| staged.zone == fixtures::BF1)
                || ctx.blob.showdown.is_some(),
            "428 · the enemy arriving on a held battlefield contests it"
        );
    }

    #[test]
    fn friendly_units_are_refused_and_the_spell_is_the_turn_players() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_RAGE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, RAGE).unwrap();
        for wrong in [fixtures::VI, fixtures::GROUNDS, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(RAGE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
