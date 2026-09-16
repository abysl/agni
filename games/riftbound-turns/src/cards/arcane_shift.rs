use super::prelude::{
    a_friendly_unit, asking, banish_by, card_target, deal, done, play, remember_card,
    remembered_cards, spell, target, with_candidates, Location,
};
use super::{Card, Filter, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;
use crate::engine::play as play_engine;
use crate::state::{Origin, TargetRef};

pub const DAMAGE: u8 = 3;
const SHIFTED: usize = 0;
const STRUCK: usize = 1;
const LOCATE: u8 = 1;

pub const ENEMY_UNIT_AT_A_BATTLEFIELD: TargetSpec = target(
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::AtBattlefield]),
    1,
    1,
    TargetKind::Card,
    "an enemy unit at a battlefield to deal 3",
);

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn where_the_owner_replays_it(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    remembered_cards(item)
        .first()
        .map(|unit| location_options(ctx, ctx.owner(*unit)))
        .unwrap_or_default()
}

pub fn replay_ignoring_cost(ctx: &mut Ctx, unit: u32, at: Location) -> bool {
    let owner = ctx.owner(unit);
    ctx.narrate(format!(
        "{{seat {owner}}} plays {{card {unit}}} from Banishment ignoring its cost"
    ));
    play_engine::begin(ctx, owner, unit, Origin::Banishment, Some(at)).is_ok()
}

fn strike_and_vanish(ctx: &mut Ctx, item: &Item) -> Flow {
    if let Some(struck) = card_target(ctx, item, STRUCK) {
        if deal(ctx, item, struck, DAMAGE) {
            ctx.narrate(format!("{{card {struck}}} takes {DAMAGE}"));
        }
    }
    banish_by(ctx, item.kind.source(), item.controller);
    done()
}

fn shift(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == LOCATE {
        if let Some(unit) = remembered_cards(item).first().copied() {
            let owner = ctx.owner(unit);
            let at = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, owner, &ctx.zones))
                .filter(|at| ctx.play_locations(owner).contains(at));
            if let Some(at) = at {
                replay_ignoring_cost(ctx, unit, at);
            }
        }
        return strike_and_vanish(ctx, item);
    }
    let Some(unit) = card_target(ctx, item, SHIFTED) else {
        return strike_and_vanish(ctx, item);
    };
    let token = ctx.is_token(unit);
    if !banish_by(ctx, unit, item.controller) || token {
        return strike_and_vanish(ctx, item);
    }
    remember_card(ctx, unit);
    let owner = ctx.owner(unit);
    match ctx.play_locations(owner).as_slice() {
        [] => strike_and_vanish(ctx, item),
        [only] => {
            replay_ignoring_cost(ctx, unit, *only);
            strike_and_vanish(ctx, item)
        }
        _ => Flow::Ask(ctx.ask_seat_resume(item, owner, LOCATE, 1, 1)),
    }
}

pub static CARD: Card = spell(
    "Arcane Shift",
    &[Keyword::Action],
    &[asking(
        with_candidates(
            play(
                &[
                    a_friendly_unit("a friendly unit to banish and replay"),
                    ENEMY_UNIT_AT_A_BATTLEFIELD,
                ],
                shift,
            ),
            where_the_owner_replays_it,
        ),
        "where its owner plays it",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{stun, FRIENDLY_UNIT};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, prompts};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const SHIFT: u32 = 90;
    const THEIR_SHIFT: u32 = 91;
    const MIND_RUNE: u32 = 100;
    const CHAOS_RUNE: u32 = 101;
    const EXTRA_RUNE: u32 = 102;

    fn shift(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Arcane Shift", 3, 1);
        card.domain = vec!["Mind".into(), "Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(shift(SHIFT, 0));
        fixture.table.cards.push(shift(THEIR_SHIFT, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(EXTRA_RUNE, 0, "Fury", false));
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

    fn cast_on_vi(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, SHIFT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(ctx), ["{card 50}", "cancel"]);
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(
            fixtures::labels(ctx),
            ["{card 60}", "cancel"],
            "Jinx in her base is out of reach"
        );
        fixtures::choose(ctx, 0, "{card 60}").unwrap();
        assert!(
            ctx.on_board(fixtures::VI),
            "nothing happens before it resolves"
        );
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_is_an_action_over_a_friendly_unit_and_an_enemy_unit_at_a_battlefield() {
        assert!(std::ptr::eq(script_of("Arcane Shift").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!(ability.targets[1], ENEMY_UNIT_AT_A_BATTLEFIELD);
        assert_eq!(ability.question, Some("where its owner plays it"));
        assert!(prompts::resume_questions().contains(&"where its owner plays it"));
    }

    #[test]
    fn the_unit_is_banished_replayed_free_to_its_only_location_the_enemy_takes_three_and_the_spell_banishes_itself(
    ) {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        stun(&mut ctx, fixtures::VI);
        ctx.damage(fixtures::VI, 2, Cause::Cleanup { last_item: None });
        let ready = ctx.ready_runes_of(0).len();
        cast_on_vi(&mut ctx);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 3,
            "the spell's three energy alone: the replay ignores Vi's cost"
        );
        assert!(ctx.blob.prompt.is_none(), "one play location needs no pick");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Banished {
                card,
                owner: 0,
                by: 0,
                ..
            } if *card == fixtures::VI
        )));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Banishment, .. } if *card == fixtures::VI
        )));
        assert_eq!(ctx.location(fixtures::VI), Some(Location::Base(0)));
        assert!(
            ctx.card(fixtures::VI).unwrap().exhausted,
            "369.3 · played, so exhausted"
        );
        assert!(!ctx.is_stunned(fixtures::VI), "a new object");
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: fixtures::SPRITE,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(
            !ctx.on_board(fixtures::SPRITE),
            "three damage kills the 3-Might Sprite"
        );
        assert_eq!(
            ctx.card(SHIFT).unwrap().zone,
            Some(fixtures::BANISHMENT),
            "Banish this: not the trash"
        );
        assert!(ctx.effects.contains(&Effect::Move {
            card: SHIFT,
            zone: fixtures::BANISHMENT,
            seat: 0,
            index: TOP
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} plays {card 50} from Banishment ignoring its cost".to_string()));
        assert!(ctx.blob.log.contains(&"{card 60} takes 3".to_string()));
        assert!(ctx.blob.log.contains(&"{card 90} is banished".to_string()));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_a_held_battlefield_the_owner_picks_where_the_unit_is_replayed() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        cast_on_vi(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: LOCATE
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "{zone 9}"],
            "the base and the held battlefield"
        );
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::BANISHMENT),
            "banished while the owner chooses"
        );
        assert!(
            ctx.on_board(fixtures::SPRITE),
            "the damage waits for the replay"
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(matches!(
            ctx.blob.chain.first().map(|held| held.kind),
            None | Some(ItemKind::Permanent { .. })
        ));
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert_eq!(ctx.card(SHIFT).unwrap().zone, Some(fixtures::BANISHMENT));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_banished_token_vanishes_and_is_not_replayed_but_the_rest_still_happens() {
        let mut fixture = armed();
        let sprite = fixture.table.card_mut(fixtures::SPRITE).unwrap();
        sprite.zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.set_controller(fixtures::SPRITE, 0, SHIFT);
        fixtures::play_from_hand(&mut ctx, 0, SHIFT).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.effects.contains(&Effect::Despawn {
            card: fixtures::SPRITE
        }));
        assert!(ctx.card(fixtures::SPRITE).is_none());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.damage_on(fixtures::THEIR_UNIT),
            0,
            "2 Might: 3 damage kills"
        );
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert_eq!(ctx.card(SHIFT).unwrap().zone, Some(fixtures::BANISHMENT));
    }

    #[test]
    fn enemy_units_and_units_in_a_base_are_refused_and_the_action_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_SHIFT)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, SHIFT).unwrap();
        for wrong in [fixtures::THEIR_UNIT, fixtures::SPRITE, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        for wrong in [fixtures::THEIR_UNIT, fixtures::VI, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 1, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy unit at a battlefield"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(SHIFT).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut poor = armed();
        poor.table.card_mut(MIND_RUNE).unwrap().domain = vec!["Fury".into()];
        poor.table.card_mut(CHAOS_RUNE).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, SHIFT)),
            Err(Refusal::NoPowerOf),
            "one power in Mind or Chaos"
        );
    }
}
