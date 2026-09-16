use super::prelude::{done, spawn, triggered, unit, Location, Token};
use super::{Card, Flow, Item, Keyword, Stage, Trigger, Where, Who};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

const SPRITE_ARRIVES_READY: bool = false;

pub fn location_left_lands_when_the_core_notes_the_moved_from(
    ctx: &Ctx,
    item: &Item,
) -> Option<Location> {
    let noted = item.noted?;
    Location::of_zone(noted.zone, item.controller, &ctx.zones)
}

fn playable(ctx: &Ctx, at: Location) -> bool {
    match at {
        Location::Battlefield(zone) => ctx.units_played_here(zone),
        Location::Base(_) => true,
    }
}

fn sprite(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let source = item.kind.source();
    let Some(at) = location_left_lands_when_the_core_notes_the_moved_from(ctx, item) else {
        ctx.narrate(format!(
            "{{card {source}}} left no noted location · no Sprite"
        ));
        return done();
    };
    if !playable(ctx, at) {
        ctx.narrate(format!(
            "no Sprite · units can't be played at {}",
            describe(at)
        ));
        return done();
    }
    if let Some(sprite) = spawn(ctx, seat, Token::Sprite, at, SPRITE_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {sprite}}} to {}",
            describe(at)
        ));
    }
    done()
}

pub static CARD: Card = unit(
    "Lillia - Fae Fawn",
    &[Keyword::Accelerate],
    &[triggered(
        Trigger::Move {
            of: Who::Me,
            to: Where::FromLocation,
        },
        &[],
        sprite,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{KIND_UNIT, TOKEN_SPRITE};
    use crate::engine::ctx::{Event, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{act, legal, priority, prompts, resume, settle};
    use crate::state::{ItemKind, Noted, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::CardInfo;

    const FAWN: u32 = 90;
    const CHAMPION_FAWN: u32 = fixtures::CHAMPION_CARD;
    const HER_MIGHT: u8 = 3;

    fn fawn(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(3),
            domain: vec!["Mind".into()],
            ..fixtures::unit(id, zone, seat, "Lillia - Fae Fawn", HER_MIGHT)
        }
    }

    fn glade(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        let runes: Vec<u32> = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.is_kind("Rune") && card.owner == 0)
            .map(|card| card.id)
            .collect();
        for (index, rune) in runes.into_iter().enumerate() {
            fixture.table.card_mut(rune).unwrap().exhausted = index >= ready;
        }
        fixture.table.cards.push(fawn(FAWN, fixtures::BASE, 0));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        fixture
    }

    fn march(ctx: &mut Ctx, seat: u8) -> Result<(), Refusal> {
        let entry = ctx.entry.ok_or(Refusal::Illegal(Reason::NoSuchCard))?;
        let intent = legal::classify(ctx, seat, &entry)?;
        act(ctx, seat, intent)?;
        settle(ctx)
    }

    fn answer(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)
    }

    fn note(ctx: &mut Ctx, item: usize, zone: u16) {
        ctx.blob.chain[item].noted = Some(Noted {
            zone,
            might: i32::from(HER_MIGHT),
            controller: 0,
            alone: false,
            buffed: false,
        });
    }

    fn sprites_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SPRITE && card.owner == seat)
            .map(|card| card.id)
            .collect()
    }

    #[test]
    fn the_script_is_an_accelerate_unit_whose_only_ability_fires_on_a_move_from_a_location() {
        let mut fixture = glade(4);
        assert_eq!(CARD.name, "Lillia - Fae Fawn");
        assert_eq!(CARD.keywords, [Keyword::Accelerate]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::FromLocation
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert!(ability.extra.is_none());
        assert!(ability.condition.is_none());
        assert!(ability.timing().is_none());
        assert!(std::ptr::eq(
            crate::cards::script_of("Lillia - Fae Fawn").unwrap(),
            &CARD
        ));
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(FAWN).unwrap(), &CARD));
        assert!(std::ptr::eq(ctx.script(CHAMPION_FAWN).unwrap(), &CARD));
        assert!(ctx.has_keyword(FAWN, Keyword::Accelerate));
        assert!(
            crate::engine::activate::offers(&ctx, 0)
                .iter()
                .all(|offer| offer.source != FAWN),
            "she has no activated ability of her own"
        );
    }

    #[test]
    fn a_standard_move_queues_her_trigger_and_the_sprite_lands_exhausted_at_the_location_she_left()
    {
        let mut fixture = glade(4);
        let action = fixtures::move_action(FAWN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march(&mut ctx, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.location(FAWN),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.events.contains(&Event::Moved {
            card: FAWN,
            from: Some(Location::Base(0)),
            to: Location::Battlefield(fixtures::BF1),
            cause: MoveCause::Standard,
            by: None
        }));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == FAWN
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert!(
            sprites_of(&ctx, 0).is_empty(),
            "the Sprite waits for the chain"
        );
        note(&mut ctx, 0, fixtures::BASE);
        let next = ctx.table.next_id;
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        let sprite = *sprites_of(&ctx, 0).first().expect("one Sprite");
        assert_eq!(sprite, next);
        assert_eq!(ctx.location(sprite), Some(Location::Base(0)));
        assert_eq!(
            ctx.location(FAWN),
            Some(Location::Battlefield(fixtures::BF1)),
            "she keeps walking; the Sprite holds the ground she left"
        );
        assert!(ctx.is_unit(sprite));
        assert!(ctx.is_token(sprite));
        assert_eq!(ctx.card(sprite).unwrap().might, Some(3));
        assert_eq!(ctx.card(sprite).unwrap().kind.as_deref(), Some(KIND_UNIT));
        assert!(
            ctx.card(sprite).unwrap().exhausted,
            "179.1.d: her text never says ready, so the token enters exhausted"
        );
        assert!(ctx.is_temporary(sprite));
        assert_eq!(ctx.controller(sprite), 0);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {sprite}}} to their base")));
    }

    #[test]
    fn a_companion_fawn_puts_her_trigger_on_the_chain_before_the_showdown_opens() {
        let mut fixture = glade(4);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = false;
        let action = fixtures::move_action(fixtures::VI, fixtures::BF2, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march(&mut ctx, 0).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::GroupMove {
                unit: fixtures::VI,
                to: fixtures::BF2
            })
        );
        answer(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            ctx.location(FAWN),
            Some(Location::Battlefield(fixtures::BF2))
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "her Move trigger is finalized first"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == FAWN
        ));
        assert!(
            ctx.blob.showdown.is_none(),
            "322.12 before 322.14 · the combat waits for the chain"
        );
        assert_eq!(ctx.blob.staged.len(), 1);
        let triggers = ctx
            .blob
            .log
            .iter()
            .position(|line| line == &format!("{{card {FAWN}}} triggers"))
            .expect("her trigger is narrated");
        assert!(
            !ctx.blob
                .log
                .iter()
                .any(|line| line.starts_with("combat at")),
            "{:?}",
            ctx.blob.log
        );
        note(&mut ctx, 0, fixtures::BASE);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        let showdown = ctx.blob.showdown.clone().expect("then the combat opens");
        assert!(showdown.combat);
        assert_eq!(showdown.zone, fixtures::BF2);
        let combat = ctx
            .blob
            .log
            .iter()
            .position(|line| line.starts_with("combat at"))
            .expect("the combat is narrated");
        assert!(triggers < combat);
        assert_eq!(sprites_of(&ctx, 0).len(), 1);
    }

    #[test]
    fn an_effect_move_pays_her_too_and_the_sprite_lands_on_the_battlefield_she_left() {
        let mut fixture = glade(4);
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.move_unit(
                FAWN,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        note(&mut ctx, 0, fixtures::BASE);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(sprites_of(&ctx, 0).len(), 1);
        ctx.actor = 1;
        assert_eq!(
            ctx.move_unit(FAWN, Location::Base(0), MoveCause::Effect),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the walk home is a move too");
        assert_eq!(
            ctx.blob.chain[0].controller, 0,
            "her controller plays the Sprite, not the seat that moved her"
        );
        note(&mut ctx, 0, fixtures::BF1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let sprites = sprites_of(&ctx, 0);
        assert_eq!(sprites.len(), 2);
        assert_eq!(
            ctx.location(sprites[1]),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            sprites_of(&ctx, 1).len() == 1,
            "the fixture Sprite is seat 1's"
        );
    }

    #[test]
    fn a_recall_is_no_move_and_a_trigger_without_a_noted_origin_plays_nothing() {
        let mut fixture = glade(4);
        let mut ctx = fixture.ctx();
        ctx.move_unit(
            FAWN,
            Location::Battlefield(fixtures::BF1),
            MoveCause::Effect,
        );
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].noted.map(|noted| noted.zone),
            Some(fixtures::BASE),
            "the core notes the location she left"
        );
        ctx.blob.chain[0].noted = None;
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            sprites_of(&ctx, 0).is_empty(),
            "no origin, no Sprite: the seam is loud in the log"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {FAWN}}} left no noted location · no Sprite"
        )));
        ctx.recall(FAWN, true);
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty(), "434.1: a recall is not a move");
        assert!(ctx.blob.queue.is_empty());
        assert!(sprites_of(&ctx, 0).is_empty());
    }

    #[test]
    fn units_cant_be_played_at_rockfall_path_so_the_sprite_she_left_there_is_never_played() {
        let mut fixture = glade(4);
        fixture.table.card_mut(FAWN).unwrap().zone = Some(fixtures::BF2);
        fixture.table.card_mut(FAWN).unwrap().seat = 0;
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.move_unit(FAWN, Location::Base(0), MoveCause::Effect),
            Moved::Moved
        );
        settle(&mut ctx).unwrap();
        note(&mut ctx, 0, fixtures::BF2);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(sprites_of(&ctx, 0).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"no Sprite · units can't be played at {zone 10}".to_string()));
    }

    #[test]
    fn accelerate_is_offered_as_she_is_played_and_lets_her_enter_ready() {
        let mut fixture = glade(4);
        fixture.table.cards.retain(|card| card.id != FAWN);
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Mind", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Mind", false));
        fixture.resolve();
        let action = fixtures::move_action(CHAMPION_FAWN, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        assert_eq!(
            intent,
            legal::Intent::Play {
                card: CHAMPION_FAWN,
                origin: Origin::Champion,
                location: Some(Location::Base(0)),
                on_chain: false
            }
        );
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { item: 1, cost: 0 }),
            "731.2: Accelerate is an optional additional cost as she is played"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::OptionalCost { item: 1, cost: 0 }),
            "accelerate {card 74} for 1 energy and 1 Mind power?"
        );
        answer(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.location(CHAMPION_FAWN), Some(Location::Base(0)));
        assert!(
            !ctx.card(CHAMPION_FAWN).unwrap().exhausted,
            "731.6: she enters ready"
        );
        assert!(sprites_of(&ctx, 0).is_empty(), "entering is not moving");
        drop(ctx);
        let mut plain = fixture;
        let mut ctx = plain.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        answer(&mut ctx, 0, 1).unwrap();
        assert!(
            ctx.card(CHAMPION_FAWN).unwrap().exhausted,
            "declined, she enters exhausted like any unit"
        );
    }

    #[test]
    fn the_sprite_lands_where_she_left_without_a_hand_stamped_note() {
        let mut fixture = glade(4);
        let action = fixtures::move_action(FAWN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let sprite = *sprites_of(&ctx, 0).first().expect("one Sprite");
        assert_eq!(ctx.location(sprite), Some(Location::Base(0)));
    }
}
