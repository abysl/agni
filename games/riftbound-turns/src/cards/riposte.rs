use super::prelude::{
    a_friendly_unit, a_spell, card_target, counter_spell, done, item_target, might_this_turn, play,
    spell,
};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::cost;
use crate::engine::ctx::Ctx;

pub fn energy_cost_of_spell(ctx: &Ctx, item: u16) -> Option<u8> {
    let card = ctx.chain_item(item)?.kind.card()?;
    Some(cost::printed_of(ctx, card, false).energy)
}

fn riposte(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let energy = item_target(ctx, item, 1).and_then(|spell| energy_cost_of_spell(ctx, spell));
    let countered = counter_spell(ctx, item, 1).is_some();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let (true, Some(energy)) = (countered, energy) else {
        ctx.narrate(format!(
            "{{card {unit}}} gains nothing · no spell was countered"
        ));
        return done();
    };
    if energy == 0 {
        ctx.narrate(format!(
            "{{card {unit}}} gains nothing · the spell cost no energy"
        ));
        return done();
    }
    might_this_turn(ctx, item, unit, i16::from(energy), None);
    ctx.narrate(format!(
        "{{card {unit}}} gets +{energy} Might this turn · the countered spell's energy cost"
    ));
    done()
}

pub static CARD: Card = spell(
    "Riposte",
    &[Keyword::Reaction],
    &[play(
        &[
            a_friendly_unit("a friendly unit"),
            a_spell("a spell to counter"),
        ],
        riposte,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, FRIENDLY_UNIT, SPELL_ON_CHAIN};
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Event, COUNTER_DAMAGE};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority};
    use crate::state::{GameBlob, Mode, Phase, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::{CardInfo, Target};

    const RIPOSTE: u32 = 90;
    const THEIR_SPELL: u32 = 91;
    const BODY_AND_ORDER: [(u32, &str); 2] = [(46, "Body"), (47, "Order")];

    static ZAP: Card = prelude::spell(
        "Zap",
        &[],
        &[play(&[prelude::a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = prelude::card_target(ctx, item, 0) {
                prelude::deal(ctx, item, unit, 3);
            }
            Flow::Done
        })],
    );

    fn their_turn(energy: u8) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 1, Mode::Enforced);
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.set_phase(Phase::Action);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.table.cards.push(CardInfo {
            domain: vec!["Body".into(), "Order".into()],
            ..fixtures::spell(RIPOSTE, fixtures::HAND, 0, "Riposte", 2, 2)
        });
        fixture.table.cards.push(CardInfo {
            domain: vec!["Mind".into()],
            ..fixtures::spell(THEIR_SPELL, fixtures::HAND, 1, "Zap", energy, 0)
        });
        for (rune, domain) in BODY_AND_ORDER {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, domain, false));
        }
        for rune in [48, 49] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Mind", false));
        }
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(THEIR_SPELL, &ZAP);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(RIPOSTE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn damage(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_DAMAGE)
            .unwrap_or(0)
    }

    fn they_zap_vi(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 1, THEIR_SPELL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 1, "{card 50}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(fixtures::VI)]);
        priority::pass(ctx, 1).unwrap();
        assert_eq!(priority::holder(ctx), Some(0));
    }

    #[test]
    fn the_script_is_a_reaction_over_a_friendly_unit_and_a_spell() {
        assert!(std::ptr::eq(script_of("Riposte").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!(ability.targets[0].kind, TargetKind::Card);
        assert_eq!(ability.targets[1].filter, SPELL_ON_CHAIN);
        assert_eq!(ability.targets[1].kind, TargetKind::Item);
    }

    #[test]
    fn the_spell_is_countered_and_the_unit_gains_its_energy_cost_this_turn() {
        let mut fixture = their_turn(4);
        let mut ctx = fixture.ctx();
        they_zap_vi(&mut ctx);
        assert_eq!(energy_cost_of_spell(&ctx, 1), Some(4));
        fixtures::play_from_hand(&mut ctx, 0, RIPOSTE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "the friendly unit first"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {THEIR_SPELL}}} on the chain"),
                "cancel".to_string()
            ],
            "then the spell · Riposte never counters itself"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SPELL}}} on the chain")).unwrap();
        assert_eq!(
            ctx.blob.chain[1].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Item(1)]
        );
        assert!(
            BODY_AND_ORDER.iter().all(|(rune, _)| ctx
                .card(*rune)
                .is_none_or(|held| held.zone == Some(fixtures::RUNE_DECK))),
            "a Body and an Order rune pay the power: {:?}",
            ctx.effects
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert_eq!(ctx.card(THEIR_SPELL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {THEIR_SPELL}}} is countered")));
        assert_eq!(damage(&ctx, fixtures::VI), 0, "Zap never resolved");
        assert_eq!(ctx.current_might(fixtures::VI), 7, "3 + the 4 energy");
        assert!(ctx.blob.log.contains(
            &"{card 50} gets +4 Might this turn · the countered spell's energy cost".to_string()
        ));
        assert_eq!(ctx.card(RIPOSTE).unwrap().zone, Some(fixtures::TRASH));
        assert!(
            !ctx.events
                .iter()
                .any(|event| matches!(event, Event::PlayedSpell { item: 1, .. })),
            "the countered spell was never played"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_hidden_spell_played_for_nothing_is_still_read_by_its_printed_energy() {
        let mut fixture = their_turn(3);
        let mut ctx = fixture.ctx();
        they_zap_vi(&mut ctx);
        ctx.blob.chain[0].origin = crate::state::Origin::Facedown {
            zone: fixtures::BF2,
        };
        assert_eq!(energy_cost_of_spell(&ctx, 1), Some(3), "printed, not paid");
        fixtures::play_from_hand(&mut ctx, 0, RIPOSTE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SPELL}}} on the chain")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_zero_cost_spell_is_countered_and_gives_nothing_and_a_gone_unit_gets_nothing() {
        let mut fixture = their_turn(0);
        let mut ctx = fixture.ctx();
        they_zap_vi(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, RIPOSTE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SPELL}}} on the chain")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.card(THEIR_SPELL).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} gains nothing · the spell cost no energy".to_string()));
        drop(ctx);

        let mut fixture = their_turn(4);
        let mut ctx = fixture.ctx();
        they_zap_vi(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, RIPOSTE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_SPELL}}} on the chain")).unwrap();
        ctx.bounce(fixtures::VI);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.card(THEIR_SPELL).unwrap().zone,
            Some(fixtures::TRASH),
            "the counter still lands on the spell that remains a legal target"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_unit_and_an_ability_on_the_chain_are_refused() {
        let mut fixture = their_turn(4);
        let mut ctx = fixture.ctx();
        they_zap_vi(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, RIPOSTE).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 2, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 2, 1, &[2]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "Riposte never counters itself"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(RIPOSTE).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.blob.chain.len(), 1);
    }
}
