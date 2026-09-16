use super::prelude::{
    a_friendly_unit, banish_by, card_target, done, play, spell, LimitedPlay, Location, Price,
};
use super::{Card, Flow, Item, Keyword};
use crate::engine::ctx::Ctx;
use crate::state::Origin;

pub fn any_battlefield(ctx: &Ctx) -> Vec<Location> {
    ctx.zones
        .battlefields
        .iter()
        .copied()
        .map(Location::Battlefield)
        .collect()
}

pub fn play_from_banishment_to_any_battlefield_ignoring_cost(ctx: &mut Ctx, unit: u32) -> bool {
    if !ctx.in_banishment(unit) {
        return false;
    }
    let owner = ctx.owner(unit);
    let locations = ctx.limited_play_locations(owner, unit, &any_battlefield(ctx));
    if locations.is_empty() {
        ctx.narrate(format!(
            "{{card {unit}}} stays in Banishment · no battlefield it can be played to"
        ));
        return false;
    }
    ctx.narrate(format!(
        "{{seat {owner}}} plays {{card {unit}}} from Banishment to any battlefield, ignoring its cost"
    ));
    ctx.play_limited(LimitedPlay {
        card: unit,
        by: owner,
        origin: Origin::Banishment,
        locations,
        price: Price::Free,
    })
    .is_ok()
}

fn hunt(ctx: &mut Ctx, item: &Item, _: super::Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !banish_by(ctx, unit, item.controller) {
        return done();
    }
    if !ctx.in_banishment(unit) {
        ctx.narrate(format!(
            "{{card {unit}}} was a token · there is nothing to play back"
        ));
        return done();
    }
    play_from_banishment_to_any_battlefield_ignoring_cost(ctx, unit);
    done()
}

pub static CARD: Card = spell(
    "Thrill of the Hunt",
    &[Keyword::Reaction],
    &[play(
        &[a_friendly_unit("a friendly unit to banish and play back")],
        hunt,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_step, priority, prompts};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const THRILL: u32 = 90;
    const THEIR_THRILL: u32 = 91;
    const WARDEN: u32 = 92;
    const BODY_RUNE: u32 = 46;
    const THEIR_RUNES: [u32; 3] = [47, 48, 49];

    static DRAWS: Card = crate::cards::prelude::unit(
        "Vi",
        &[],
        &[play(&[], |ctx, item, _| {
            crate::cards::prelude::draw(ctx, item.controller, 1);
            Flow::Done
        })],
    );

    fn thrill(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Thrill of the Hunt", 2, 1);
        card.domain = vec!["Fury".into(), "Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().name = "Plain Field".into();
        fixture.table.cards.push(thrill(THRILL, 0));
        fixture.table.cards.push(thrill(THEIR_THRILL, 1));
        {
            let vi = fixture.table.card_mut(fixtures::VI).unwrap();
            vi.energy = Some(4);
            vi.power = Some(2);
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        for rune in THEIR_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Fury", false));
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_reaction_over_one_friendly_unit_whose_owner_is_asked_where_by_the_engine() {
        assert!(std::ptr::eq(
            script_of("Thrill of the Hunt").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Thrill of the Hunt");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT);
        assert!(ability.candidates.is_none());
        assert!(ability.question.is_none());
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            any_battlefield(&ctx),
            [
                Location::Battlefield(fixtures::BF1),
                Location::Battlefield(fixtures::BF2)
            ],
            "every battlefield with a card in it, held or not, occupied or not"
        );
    }

    #[test]
    fn vi_is_banished_then_her_owner_picks_any_battlefield_and_she_lands_there_for_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, THRILL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        let ready = ctx.ready_runes_of(0).len();
        assert_eq!(
            ready, 2,
            "two energy off four ready runes, one of which then recycles for the power"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.events.iter().any(|event| matches!(
                event,
                Event::Banished { card, owner: 0, .. } if *card == fixtures::VI
            )),
            "banished first"
        );
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::CHAIN),
            "then the play begins: the card waits on the chain for its location"
        );
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert!(!prompt.cancel);
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BF1),
                format!("{{zone {}}}", fixtures::BF2)
            ],
            "any battlefield: the base is not offered"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("where does {{card {}}} enter?", fixtures::VI)
        );
        assert!(
            ctx.blob.chain.is_empty(),
            "the spell has resolved · the play waits on its location"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF2)).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF2)),
            "the other seat's held, occupied battlefield: {:?}",
            ctx.blob.log
        );
        assert!(ctx.card(fixtures::VI).unwrap().exhausted, "369.3");
        assert!(!ctx.in_banishment(fixtures::VI));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready,
            "ignoring its cost: four energy and two power went unpaid"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Banishment, .. } if *card == fixtures::VI
        )));
        assert!(ctx.blob.log.contains(
            &"{seat 0} plays {card 50} from Banishment to any battlefield, ignoring its cost"
                .to_string()
        ));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card 50}} to {{zone {}}}",
            fixtures::BF2
        )));
        assert_eq!(ctx.card(THRILL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_replayed_unit_is_a_fresh_play_whose_play_trigger_fires_again() {
        let mut fixture = armed();
        fixture.scripts = fixture.scripts.clone().with_script(fixtures::VI, &DRAWS);
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        assert!(ctx.damage(fixtures::VI, 1, crate::engine::ctx::Cause::Rule));
        fixtures::play_from_hand(&mut ctx, 0, THRILL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.damage_on(fixtures::VI), 0, "a new object");
        assert_eq!(ctx.blob.chain.len(), 1, "Vi's own play trigger");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == fixtures::VI
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + 1);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_other_seat_reacts_on_my_turn_and_its_unit_lands_at_my_held_battlefield() {
        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_THRILL).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 60}", "{card 81}", "cancel"]);
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Banished { card, owner: 1, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::CHAIN)
        );
        assert_eq!(
            ctx.blob.prompt.as_ref().map(|prompt| prompt.seat),
            Some(1),
            "the owner picks, not the turn player"
        );
        fixtures::choose(&mut ctx, 1, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert_eq!(
            ctx.location(fixtures::THEIR_UNIT),
            Some(Location::Battlefield(fixtures::BF1)),
            "a battlefield I hold is still any battlefield"
        );
        assert_eq!(ctx.controller(fixtures::THEIR_UNIT), 1);
        assert_eq!(ctx.blob.chain.len(), 1, "my spell still waits beneath");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_sprite_is_banished_and_nothing_comes_back() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::SPRITE).unwrap().owner = 0;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, THRILL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "179 · a banished token ceases to exist"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} was a token · there is nothing to play back".to_string()));
    }

    #[test]
    fn an_enemy_unit_a_gear_and_a_unit_that_left_are_refused_or_skipped() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, THRILL).unwrap();
        for wrong in [
            fixtures::THEIR_UNIT,
            fixtures::HAND_GEAR,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_step::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit on the board"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(THRILL).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        fixtures::play_from_hand(&mut ctx, 0, THRILL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(fixtures::VI, fixtures::HAND, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.card(fixtures::VI).unwrap().zone,
            Some(fixtures::HAND),
            "a unit that left the board is neither banished nor replayed"
        );
        assert_eq!(ctx.card(THRILL).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn rockfall_path_is_not_offered_and_under_the_wardens_lock_she_stays_in_banishment() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::ROCKFALL).unwrap().name = "Rockfall Path".into();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.limited_play_locations(0, fixtures::VI, &any_battlefield(&ctx)),
            [Location::Battlefield(fixtures::BF1)],
            "359.3.e.6 · units can't be played to Rockfall Path, so any battlefield is the other one"
        );
        fixtures::play_from_hand(&mut ctx, 0, THRILL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "one battlefield left: no question"
        );
        assert_eq!(
            ctx.location(fixtures::VI),
            Some(Location::Battlefield(fixtures::BF1)),
            "{:?}",
            ctx.blob.log
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        drop(ctx);

        let mut fixture = armed();
        fixture.table.cards.push(fixtures::unit(
            WARDEN,
            fixtures::BF1,
            1,
            "Mageseeker Warden",
            5,
        ));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.units_only_to_base(0));
        fixtures::play_from_hand(&mut ctx, 0, THRILL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Banished { card, owner: 0, .. } if *card == fixtures::VI
        )));
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.in_banishment(fixtures::VI),
            "the lock leaves no battlefield to play her to · {:?}",
            ctx.blob.log
        );
        assert!(ctx.blob.log.contains(
            &"{card 50} stays in Banishment · no battlefield it can be played to".to_string()
        ));
        assert!(!ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, .. } if *card == fixtures::VI
        )));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_engine_serves_the_any_battlefield_prompt_itself() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, THRILL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::PlayLocation { .. })));
    }
}
