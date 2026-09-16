use super::prelude::{a_card, card_target, done, play, ready, unit};
use super::rumble_mechanized_menace::MECH;
use super::{Card, Filter, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ANOTHER_FRIENDLY_MECH: Filter =
    Filter::And(&[Filter::Unit, Filter::Friendly, Filter::NotSelf, MECH]);

pub const MECH_TO_READY: TargetSpec =
    a_card(ANOTHER_FRIENDLY_MECH, "another friendly Mech to ready");

fn bubble(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(mech) = card_target(ctx, item, 0) else {
        return done();
    };
    if ready(ctx, mech) {
        ctx.narrate(format!("{{card {mech}}} readies"));
    } else {
        ctx.narrate(format!("{{card {mech}}} is already ready"));
    }
    done()
}

pub static CARD: Card = unit("Bubble Bot", &[], &[play(&[MECH_TO_READY], bubble)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::rumble_mechanized_menace::{friendly_mechs, is_mech, your_mech};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};
    use agni_plugin_sdk::table::CardInfo;

    const BOT: u32 = 90;
    const JAMMER: u32 = 91;
    const MEGA: u32 = 92;
    const THEIR_MECH: u32 = 93;

    fn bot(zone: u16) -> CardInfo {
        let mut card = fixtures::unit(BOT, zone, 0, "Bubble Bot", 3);
        card.domain = vec!["Mind".into()];
        card.energy = Some(1);
        card
    }

    fn exhausted_unit(id: u32, zone: u16, seat: u8, name: &str) -> CardInfo {
        let mut card = fixtures::unit(id, zone, seat, name, 2);
        card.exhausted = true;
        card
    }

    fn workshop() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(bot(fixtures::HAND));
        fixture
            .table
            .cards
            .push(exhausted_unit(JAMMER, fixtures::BASE, 0, "Gem Jammer"));
        fixture
            .table
            .cards
            .push(exhausted_unit(MEGA, fixtures::BF1, 0, "Mega-Mech"));
        fixture
            .table
            .cards
            .push(exhausted_unit(THEIR_MECH, fixtures::BASE, 1, "Adaptatron"));
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(BOT).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_is_a_unit_whose_play_trigger_chooses_another_friendly_mech() {
        assert!(std::ptr::eq(script_of("Bubble Bot").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional);
        assert_eq!(ability.targets, [MECH_TO_READY]);
        assert_eq!((MECH_TO_READY.min, MECH_TO_READY.max), (1, 1));
        assert_eq!(
            ANOTHER_FRIENDLY_MECH,
            Filter::And(&[Filter::Unit, Filter::Friendly, Filter::NotSelf, MECH])
        );
    }

    #[test]
    fn the_mech_seam_reads_the_printed_name_of_a_unit_on_the_board() {
        let mut fixture = workshop();
        fixture.table.card_mut(MEGA).unwrap().name = "Mega-Mech (Alternate Art)".into();
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(is_mech(&ctx, JAMMER));
        assert!(is_mech(&ctx, MEGA), "the print suffix is not the name");
        assert!(is_mech(&ctx, THEIR_MECH), "any controller");
        assert!(!is_mech(&ctx, fixtures::VI));
        assert!(!is_mech(&ctx, fixtures::SPRITE));
        assert!(
            is_mech(&ctx, BOT),
            "the tag is printed on the card wherever it is"
        );
        assert!(your_mech(&ctx, JAMMER, JAMMER), "including me");
        assert!(your_mech(&ctx, JAMMER, MEGA));
        assert!(!your_mech(&ctx, JAMMER, THEIR_MECH));
        assert_eq!(
            friendly_mechs(&ctx, 0),
            [JAMMER, MEGA],
            "a Mech in hand is not a friendly unit"
        );
        assert_eq!(friendly_mechs(&ctx, 1), [THEIR_MECH]);
        drop(ctx);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BOT).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 91}", "{card 92}"],
            "the MECH target filter reads the same base name the aura seam does"
        );
    }

    #[test]
    fn playing_the_bot_offers_the_other_friendly_mechs_and_readies_the_chosen_one() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BOT).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::PlayLocation { item: 1 }));
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 91}", "{card 92}"],
            "the friendly Mechs anywhere · not Vi, not the bot, not the enemy Mech · a trigger has no cancel"
        );
        assert_eq!(
            crate::engine::prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: 2,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.on_board(BOT));
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == BOT
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(MEGA)]);
        assert!(
            ctx.card(MEGA).unwrap().exhausted,
            "the ready waits for the trigger"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(MEGA).unwrap().exhausted);
        assert!(ctx.effects.contains(&Effect::ready(MEGA)));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Readied { card, by: 0 } if *card == MEGA
        )));
        assert!(ctx.card(JAMMER).unwrap().exhausted);
        assert!(ctx.card(BOT).unwrap().exhausted, "the bot enters exhausted");
        assert!(ctx.blob.log.contains(&"{card 92} readies".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_other_friendly_mech_the_trigger_fizzles_and_the_bot_still_lands() {
        let mut fixture = workshop();
        fixture
            .table
            .cards
            .retain(|card| card.id != JAMMER && card.id != MEGA);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BOT).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        assert!(ctx.blob.prompt.is_none(), "{:?}", ctx.blob.why);
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert!(ctx.on_board(BOT));
        assert!(ctx.card(fixtures::VI).unwrap().exhausted, "Vi is no Mech");
        assert!(
            ctx.card(THEIR_MECH).unwrap().exhausted,
            "theirs is not friendly"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} trigger fizzles · no legal target".to_string()));
    }

    #[test]
    fn a_mech_that_left_before_resolution_readies_nothing() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BOT).unwrap();
        fixtures::choose(&mut ctx, 0, "your base").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        ctx.kill(JAMMER, crate::engine::ctx::Cause::Rule);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(JAMMER));
        assert!(
            ctx.card(MEGA).unwrap().exhausted,
            "the other Mech was not the target"
        );
        assert!(!ctx.effects.contains(&Effect::ready(JAMMER)));
        assert!(!ctx.effects.contains(&Effect::ready(MEGA)));
    }
}
