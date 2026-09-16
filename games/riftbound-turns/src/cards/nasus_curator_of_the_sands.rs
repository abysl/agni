use super::prelude::{
    asking, done, exhausting_self, legend, on_activated, on_you_play_card, optional, ready, when,
    with_candidates,
};
use super::{Card, Flow, Item, Source, Stage, KIND_SPELL};
use crate::engine::cost;
use crate::engine::ctx::{Ctx, Event};
use crate::state::TargetRef;

pub const ENERGY_AT_LEAST: u8 = 7;
pub const RUNES: u8 = 2;
const STAGE_RUNES: u8 = 1;

fn a_big_permanent(ctx: &Ctx, event: &Event, _: Source) -> bool {
    let Event::Played { card, kind, .. } = event else {
        return false;
    };
    kind != KIND_SPELL
        && ctx
            .card(*card)
            .and_then(|face| face.energy)
            .is_some_and(|energy| energy >= ENERGY_AT_LEAST)
}

fn a_big_activation(ctx: &Ctx, event: &Event, _: Source) -> bool {
    let Event::Activated { source, index, .. } = event else {
        return false;
    };
    cost::base_of_activation(ctx, *source, *index).energy >= ENERGY_AT_LEAST
}

fn exhausted_runes(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    ctx.runes_of(item.controller)
        .into_iter()
        .filter(|rune| rune.exhausted)
        .map(|rune| TargetRef::Card(rune.id))
        .collect()
}

fn curate(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        STAGE_RUNES => {
            let offered = exhausted_runes(ctx, item, stage);
            let picked: Vec<u32> = ctx
                .picks()
                .iter()
                .copied()
                .filter(|rune| offered.contains(&TargetRef::Card(*rune)))
                .take(usize::from(RUNES))
                .collect();
            for rune in picked {
                if ready(ctx, rune) {
                    ctx.narrate(format!("{{card {rune}}} readies"));
                }
            }
            done()
        }
        _ => {
            if exhausted_runes(ctx, item, Stage(STAGE_RUNES)).is_empty() {
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_RUNES, 0, RUNES))
        }
    }
}

pub static CARD: Card = legend(
    "Nasus - Curator of the Sands",
    &[],
    &[
        asking(
            with_candidates(
                optional(exhausting_self(when(
                    on_you_play_card(&[], curate),
                    a_big_permanent,
                ))),
                exhausted_runes,
            ),
            "up to two runes to ready",
        ),
        asking(
            with_candidates(
                optional(exhausting_self(when(
                    on_activated(&[], curate),
                    a_big_activation,
                ))),
                exhausted_runes,
            ),
            "up to two runes to ready",
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{activated, named, unit as unit_card};
    use crate::cards::{script_of, Cost, Timing};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::SLOT_TRIGGER_COST;
    use crate::engine::{activate, priority, prompts};
    use crate::state::{ItemKind, PromptWhy};

    const NASUS: u32 = fixtures::LEGEND_CARD;
    const HERON: u32 = 90;
    const CHEAP: u32 = 91;
    const TOWER: u32 = 92;

    static CANNON: Card = unit_card(
        "Cannon",
        &[],
        &[named(
            activated(
                Timing::Sorcery,
                Cost {
                    energy: 8,
                    power: &[],
                },
                &[],
                |_, _, _| Flow::Done,
            ),
            "fire",
        )],
    );

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(NASUS).unwrap().name = "Nasus - Curator of the Sands".into();
        let mut heron = fixtures::unit(HERON, fixtures::HAND, 0, "Astral Heron", 7);
        heron.energy = Some(7);
        fixture.table.cards.push(heron);
        let mut cheap = fixtures::unit(CHEAP, fixtures::HAND, 0, "Jinx", 2);
        cheap.energy = Some(6);
        fixture.table.cards.push(cheap);
        fixture
            .table
            .cards
            .push(fixtures::unit(TOWER, fixtures::BASE, 0, "Cannon", 1));
        for id in 46..=53 {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Calm", false));
        }
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(TOWER, &CANNON);
        fixture
    }

    #[test]
    fn a_seven_cost_unit_asks_to_exhaust_him_and_yes_readies_two_chosen_runes() {
        assert!(std::ptr::eq(
            script_of("Nasus - Curator of the Sands").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.abilities.len(), 2);
        assert!(CARD.abilities.iter().all(|ability| ability.optional));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HERON).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        assert_eq!(fixtures::labels(&ctx), ["yes", "no"]);
        assert!(!ctx.card(NASUS).unwrap().exhausted);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(
            ctx.card(NASUS).unwrap().exhausted,
            "yes exhausts him at finalization"
        );
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: 0 }) if source == NASUS
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Resume { item: 2, stage: 1 }));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Resume { item: 2, stage: 1 }),
            "{card 75}: choose up to two runes to ready (0 of 2)"
        );
        let exhausted: Vec<u32> = ctx
            .runes_of(0)
            .into_iter()
            .filter(|rune| rune.exhausted)
            .map(|rune| rune.id)
            .collect();
        assert!(exhausted.len() >= 3);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", exhausted[0])).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", exhausted[1])).unwrap();
        assert!(ctx.blob.prompt.is_none(), "a full prompt closes on its own");
        assert!(!ctx.card(exhausted[0]).unwrap().exhausted);
        assert!(!ctx.card(exhausted[1]).unwrap().exhausted);
        assert!(ctx.card(exhausted[2]).unwrap().exhausted);
    }

    #[test]
    fn no_leaves_him_ready_and_a_six_cost_unit_asks_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HERON).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(!ctx.card(NASUS).unwrap().exhausted);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 75} trigger is removed · its cost is declined".to_string()));
        let mut cheap = armed();
        let mut ctx = cheap.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CHEAP).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        let mut tired = armed();
        tired.table.card_mut(NASUS).unwrap().exhausted = true;
        let mut ctx = tired.ctx();
        fixtures::play_from_hand(&mut ctx, 0, HERON).unwrap();
        assert!(ctx.blob.prompt.is_none(), "an exhausted Nasus can't pay");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 75} trigger is removed · its source is exhausted".to_string()));
    }

    #[test]
    fn an_eight_energy_activation_asks_too_once_it_resolves_and_keeps_the_floating_discount() {
        let mut fixture = armed();
        fixture.blob.seat_mut(0).promises = vec![crate::cards::astral_heron::foreseen()];
        let mut ctx = fixture.ctx();
        assert_eq!(
            cost::of_activation(&ctx, TOWER, 0).energy,
            8,
            "206.1: Heron's next-card discount is for cards, not abilities"
        );
        activate::activate(&mut ctx, 0, TOWER, 0).unwrap();
        crate::engine::settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.blob.seat(0).promises,
            [crate::cards::astral_heron::foreseen()],
            "the discount waits for the next card"
        );
        let exhausted_runes = ctx
            .effects
            .iter()
            .filter(|effect| {
                matches!(effect, agni_plugin_sdk::decide::Effect::Annotate { card, key, .. } if *card != TOWER && key == "exhausted")
            })
            .count();
        assert_eq!(
            exhausted_runes, 8,
            "Empower-sized activations pay their full eight: {:?}",
            ctx.effects
        );
        assert!(
            ctx.blob.prompt.is_none(),
            "377.2.a: playing an activated ability counts when it resolves"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 2,
                cost: SLOT_TRIGGER_COST as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert!(ctx.card(NASUS).unwrap().exhausted);
        assert_eq!(ctx.blob.chain.len(), 1, "the ability has resolved");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == NASUS
        ));
    }
}
