use super::prelude::{
    a_play_location, activated, costing, done, legend, named, spawn, temporary_units, zone_target,
    Location, Token,
};
use super::{Card, Cost, Flow, Item, Source, Stage, Timing};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;

pub const FULL_PRICE: u8 = 4;
const SPRITE_ARRIVES_READY: bool = true;

fn price(ctx: &Ctx, source: Source) -> Cost {
    let seat = ctx.controller(source.card);
    let discount = u8::try_from(temporary_units(ctx, seat)).unwrap_or(u8::MAX);
    Cost {
        energy: FULL_PRICE.saturating_sub(discount),
        power: &[],
    }
}

fn sprite(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let at = zone_target(item, 0)
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .unwrap_or(Location::Base(seat));
    if let Some(sprite) = spawn(ctx, seat, Token::Sprite, at, SPRITE_ARRIVES_READY) {
        ctx.narrate(format!(
            "{{seat {seat}}} plays {{card {sprite}}} to {}",
            describe(at)
        ));
    }
    done()
}

pub static CARD: Card = legend(
    "Lillia - Bashful Bloom",
    &[],
    &[named(
        costing(
            activated(
                Timing::Sorcery,
                Cost {
                    energy: FULL_PRICE,
                    power: &[],
                },
                &[a_play_location("where the Sprite is played")],
                sprite,
            ),
            price,
        ),
        "play a Sprite",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{SelfCost, TargetKind, Trigger, KIND_UNIT, TOKEN_SPRITE};
    use crate::engine::cost;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority, prompts, resume, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::CardInfo;

    const LILLIA: u32 = fixtures::LEGEND_CARD;
    const SPRITE_A: u32 = 90;
    const SPRITE_B: u32 = 91;
    const SPRITE_C: u32 = 92;
    const SPRITE_D: u32 = 93;

    fn sprite_token(id: u32, seat: u8) -> CardInfo {
        CardInfo {
            might: Some(3),
            ..fixtures::card(id, fixtures::BASE, seat, TOKEN_SPRITE, KIND_UNIT)
        }
    }

    fn court(ready: usize) -> Fixture {
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
        fixture.resolve();
        fixture
    }

    fn with_sprites(ready: usize, sprites: &[u32]) -> Fixture {
        let mut fixture = court(ready);
        for sprite in sprites {
            fixture.table.cards.push(sprite_token(*sprite, 0));
        }
        fixture.resolve();
        fixture
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

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|option| option.label.clone())
            .collect()
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
    fn the_script_is_a_legend_with_one_sorcery_activation_that_exhausts_her() {
        let mut fixture = court(4);
        assert_eq!(CARD.name, "Lillia - Bashful Bloom");
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(ability.timing(), Some(Timing::Sorcery));
        assert_eq!(ability.label, Some("play a Sprite"));
        assert!(!ability.optional);
        assert_eq!(ability.once, crate::cards::Once::Never);
        assert!(ability.condition.is_none());
        assert!(ability.candidates.is_none());
        assert_eq!(
            ability.cost,
            Some(Cost {
                energy: FULL_PRICE,
                power: &[]
            })
        );
        assert!(ability.extra.is_some());
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].kind, TargetKind::Zone);
        assert_eq!(ability.targets[0].min, 1);
        assert_eq!(ability.targets[0].max, 1);
        assert!(std::ptr::eq(
            crate::cards::script_of("Lillia - Bashful Bloom").unwrap(),
            &CARD
        ));
        let ctx = fixture.ctx();
        assert!(std::ptr::eq(ctx.script(LILLIA).unwrap(), &CARD));
        assert_eq!(ability.self_cost, SelfCost::Auto);
        assert_eq!(
            activate::self_cost(&ctx, LILLIA, ability),
            SelfCost::Exhaust,
            "401.3/170.8: a legend's Auto self-cost is exhaust me"
        );
    }

    #[test]
    fn the_ability_costs_one_less_for_each_friendly_temporary_unit_and_never_below_free() {
        let mut fixture = court(4);
        let ctx = fixture.ctx();
        assert!(ctx.is_temporary(fixtures::SPRITE));
        assert_eq!(ctx.controller(fixtures::SPRITE), 1);
        assert_eq!(
            cost::of_activation(&ctx, LILLIA, 0).energy,
            FULL_PRICE,
            "an enemy Sprite is no discount"
        );
        assert!(cost::of_activation(&ctx, LILLIA, 0).power.is_empty());
        drop(ctx);
        let mut one = with_sprites(4, &[SPRITE_A]);
        assert_eq!(cost::of_activation(&one.ctx(), LILLIA, 0).energy, 3);
        let mut two = with_sprites(4, &[SPRITE_A, SPRITE_B]);
        assert_eq!(cost::of_activation(&two.ctx(), LILLIA, 0).energy, 2);
        let mut five = with_sprites(4, &[SPRITE_A, SPRITE_B, SPRITE_C, SPRITE_D]);
        five.table.cards.push(sprite_token(94, 0));
        five.resolve();
        let ctx = five.ctx();
        assert_eq!(ctx.card(fixtures::VI).unwrap().name, "Vi");
        assert!(
            !ctx.is_temporary(fixtures::VI),
            "a plain friendly unit is no discount"
        );
        assert!(
            cost::of_activation(&ctx, LILLIA, 0).is_free(),
            "five friendly Temporary units floor the cost at nothing"
        );
    }

    #[test]
    fn activating_her_exhausts_the_legend_pays_four_and_plays_a_ready_sprite_where_the_seat_picks()
    {
        let mut fixture = court(4);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        let offers = activate::offers(&ctx, 0);
        assert_eq!(offers.len(), 1);
        assert_eq!(offers[0].source, LILLIA);
        assert_eq!(offers[0].index, 0);
        assert_eq!(
            offers[0].label,
            "{card 75}: play a Sprite (4 energy, exhaust)"
        );
        assert!(sprites_of(&ctx, 0).is_empty());
        activate::activate(&mut ctx, 0, LILLIA, 0).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 75}: choose where the Sprite is played (0 of 1)"
        );
        assert_eq!(
            labels(&ctx),
            ["{zone 8}", "{zone 9}", "cancel"],
            "the base and the held battlefield"
        );
        assert!(
            !ctx.card(LILLIA).unwrap().exhausted,
            "the cost is paid when the item finalizes"
        );
        answer(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.card(LILLIA).unwrap().exhausted);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "four ready runes, four energy"
        );
        assert!(
            !ctx.events.contains(&crate::engine::ctx::Event::Activated {
                item: 1,
                source: LILLIA,
                index: 0,
                controller: 0
            }),
            "377.2.a: the activation is raised when it resolves, not as it finalizes"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Ability { source, index: 0 } if source == LILLIA
        ));
        assert_eq!(ctx.blob.chain[0].controller, 0);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Zone(fixtures::BF1)],
            "the Sprite's location travels with the item"
        );
        assert!(
            sprites_of(&ctx, 0).is_empty(),
            "the Sprite waits for the chain"
        );
        let next = ctx.table.next_id;
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        let sprite = *sprites_of(&ctx, 0).first().expect("one Sprite");
        assert_eq!(sprite, next);
        assert!(ctx.is_unit(sprite));
        assert!(ctx.is_token(sprite));
        assert_eq!(ctx.card(sprite).unwrap().might, Some(3));
        assert_eq!(ctx.card(sprite).unwrap().kind.as_deref(), Some(KIND_UNIT));
        assert_eq!(
            ctx.location(sprite),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(
            !ctx.card(sprite).unwrap().exhausted,
            "the ability plays it ready"
        );
        assert!(ctx.is_temporary(sprite));
        assert_eq!(ctx.controller(sprite), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 75} ability resolves".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {sprite}}} to {{zone 9}}")));
        assert_eq!(
            cost::of_activation(&ctx, LILLIA, 0).energy,
            3,
            "the Sprite it played discounts the next activation"
        );
    }

    #[test]
    fn a_sprite_played_to_the_base_lands_there_and_the_legend_cannot_go_twice_in_one_turn() {
        let mut fixture = court(4);
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, LILLIA, 0).unwrap();
        assert_eq!(labels(&ctx), ["{zone 8}", "cancel"], "seat 0 holds nothing");
        answer(&mut ctx, 0, 0).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        let sprite = *sprites_of(&ctx, 0).first().expect("one Sprite");
        assert_eq!(ctx.location(sprite), Some(Location::Base(0)));
        assert_eq!(
            activate::activate(&mut ctx, 0, LILLIA, 0),
            Err(Refusal::Exhausted),
            "the exhaust cost gates the second activation"
        );
        assert!(activate::offers(&ctx, 0).is_empty());
        assert_eq!(sprites_of(&ctx, 0).len(), 1);
    }

    #[test]
    fn the_activation_is_refused_for_the_wrong_seat_a_missing_ability_short_runes_and_a_busy_chain()
    {
        let mut short = court(3);
        let mut ctx = short.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 0, LILLIA, 0),
            Err(Refusal::NotEnoughRunes {
                needed: 4,
                ready: 3
            }),
            "three ready runes cannot pay four energy"
        );
        let offers = activate::offers(&ctx, 0);
        assert_eq!(offers.len(), 1, "the price is still shown");
        assert!(
            !offers[0].enabled,
            "greyed out rather than gone, so the seat sees what it costs"
        );
        assert!(ctx.blob.queue.is_empty());
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);
        let mut fixture = court(4);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, LILLIA, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            activate::activate(&mut ctx, 0, LILLIA, 1),
            Err(Refusal::Illegal(Reason::NoSuchAbility))
        );
        assert!(activate::offers(&ctx, 1).is_empty());
        activate::activate(&mut ctx, 0, LILLIA, 0).unwrap();
        answer(&mut ctx, 0, 0).unwrap();
        assert_eq!(
            activate::activate(&mut ctx, 0, LILLIA, 0),
            Err(Refusal::Illegal(Reason::ClosedTiming)),
            "374/144.2: no activation while the chain is open"
        );
        assert!(sprites_of(&ctx, 0).is_empty());
    }
}
