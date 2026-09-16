use super::prelude::{
    asking, battlefield, triggered, when, with_candidates, Location, PREDICT_QUESTION,
    PREDICT_RECYCLE,
};
use super::{Ability, Card, Source, Trigger};
use crate::engine::ctx::{Ctx, Event};

pub use super::prelude::{predict, top_of_deck};

pub const ENERGY_AT_LEAST: u8 = 4;
pub const RECYCLE: u8 = PREDICT_RECYCLE;
pub const QUESTION: &str = PREDICT_QUESTION;

pub fn energy_spent_on_the_spell(_: &Ctx, _: &Event) -> Option<u8> {
    None
}

pub fn controls_it(ctx: &Ctx, library: u32, seat: u8) -> bool {
    ctx.is_battlefield_card(library)
        && matches!(ctx.location(library), Some(Location::Battlefield(zone)) if ctx.holds(seat, zone))
}

pub fn the_holder_played_the_spell(ctx: &Ctx, event: &Event, source: Source) -> bool {
    matches!(event, Event::PlayedSpell { controller, .. } if controls_it(ctx, source.card, *controller))
}

pub fn the_holder_spent_four_or_more_on_the_spell(
    ctx: &Ctx,
    event: &Event,
    source: Source,
) -> bool {
    the_holder_played_the_spell(ctx, event, source)
        && energy_spent_on_the_spell(ctx, event).is_some_and(|energy| energy >= ENERGY_AT_LEAST)
}

pub const WHEN_YOU_PLAY_A_SPELL: Ability = asking(
    with_candidates(
        when(
            triggered(Trigger::YouPlaySpell, &[], predict),
            the_holder_spent_four_or_more_on_the_spell,
        ),
        top_of_deck,
    ),
    QUESTION,
);

pub static CARD: Card = battlefield("Forgotten Library", &[], &[WHEN_YOU_PLAY_A_SPELL]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{play, spell};
    use crate::cards::{script_of, Flow};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const LIBRARY: u32 = fixtures::GROUNDS;
    const TOME: u32 = 90;
    const MY_TOP: u32 = 23;

    static TOME_CARD: Card = spell("Tome", &[], &[play(&[], |_, _, _| Flow::Done)]);

    static WIRED_TO_EVERY_HELD_SPELL_FOR_THE_TEST: Card = battlefield(
        "Forgotten Library",
        &[],
        &[asking(
            with_candidates(
                when(
                    triggered(Trigger::YouPlaySpell, &[], predict),
                    the_holder_played_the_spell,
                ),
                top_of_deck,
            ),
            QUESTION,
        )],
    );

    fn library_held_by(seat: Option<u8>) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(LIBRARY).unwrap().name = "Forgotten Library".into();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::spell(TOME, fixtures::HAND, 0, "Tome", 4, 0));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, seat);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(TOME, &TOME_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(LIBRARY).unwrap(),
            &CARD
        ));
        fixture
    }

    fn wired(seat: Option<u8>) -> Fixture {
        let mut fixture = library_held_by(seat);
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(LIBRARY, &WIRED_TO_EVERY_HELD_SPELL_FOR_THE_TEST);
        fixture
    }

    fn library_items(ctx: &Ctx) -> Vec<u16> {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .filter(
                |item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == LIBRARY),
            )
            .map(|item| item.id)
            .collect()
    }

    fn deck(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .held(fixtures::MAIN_DECK, seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast_the_tome(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, TOME).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "four energy from all four runes"
        );
        fixtures::pass_until_open(ctx);
        settle(ctx).unwrap();
    }

    #[test]
    fn the_library_is_one_conditioned_spell_trigger_whose_energy_half_is_a_seam() {
        assert!(std::ptr::eq(script_of("Forgotten Library").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlaySpell);
        assert!(std::ptr::fn_addr_eq(
            ability.condition.unwrap(),
            the_holder_spent_four_or_more_on_the_spell as fn(&Ctx, &Event, Source) -> bool
        ));
        assert!(ability.targets.is_empty());
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(!ability.optional);
        assert_eq!(ENERGY_AT_LEAST, 4);
    }

    #[test]
    fn control_is_read_off_the_holder_and_the_energy_spent_is_unknown_today() {
        let mut fixture = library_held_by(Some(0));
        let ctx = fixture.ctx();
        assert!(controls_it(&ctx, LIBRARY, 0));
        assert!(!controls_it(&ctx, LIBRARY, 1));
        assert!(!controls_it(&ctx, fixtures::VI, 0), "a unit holds nothing");
        let source = Source {
            card: LIBRARY,
            ability: 0,
        };
        let mine = Event::PlayedSpell {
            item: 2,
            controller: 0,
            nth: 1,
        };
        let theirs = Event::PlayedSpell {
            item: 2,
            controller: 1,
            nth: 1,
        };
        assert!(the_holder_played_the_spell(&ctx, &mine, source));
        assert!(!the_holder_played_the_spell(&ctx, &theirs, source));
        assert_eq!(energy_spent_on_the_spell(&ctx, &mine), None);
        assert!(
            !the_holder_spent_four_or_more_on_the_spell(&ctx, &mine, source),
            "the engine records no energy spent"
        );
        drop(ctx);
        let mut unheld = library_held_by(None);
        let ctx = unheld.ctx();
        assert!(!controls_it(&ctx, LIBRARY, 0));
        assert!(
            !the_holder_played_the_spell(&ctx, &mine, source),
            "190.6.b hands an unheld battlefield's trigger to the turn player, so the hold is checked by hand"
        );
    }

    #[test]
    fn wired_to_every_held_spell_the_holder_peeks_the_top_card_and_may_recycle_it() {
        let mut fixture = wired(Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(deck(&ctx, 0), [20, 21, 22, MY_TOP]);
        cast_the_tome(&mut ctx);
        let items = library_items(&ctx);
        assert_eq!(items.len(), 1);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: items[0],
                stage: RECYCLE
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {MY_TOP}}}"), "skip".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {LIBRARY}}}: choose the top card of your deck to recycle (0 of 1)")
        );
        assert!(
            ctx.effects.contains(&Effect::Peek {
                card: MY_TOP,
                seat: 0
            }),
            "the top card is shown to the holder alone"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} looks at the top card of their deck".to_string()));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(prompt.seat, 0);
        assert_eq!(
            prompts::answer(
                &mut ctx,
                1,
                Pick {
                    prompt: prompt.id,
                    option: 0
                }
            ),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP}}}")).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.effects.contains(&Effect::Move {
            card: MY_TOP,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert_eq!(deck(&ctx, 0), [MY_TOP, 20, 21, 22]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} recycles the top card of their deck".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn skipping_keeps_the_top_card_and_an_empty_deck_predicts_nothing() {
        let mut fixture = wired(Some(0));
        let mut ctx = fixture.ctx();
        cast_the_tome(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(deck(&ctx, 0), [20, 21, 22, MY_TOP]);
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} keeps the top card of their deck".to_string()));
        drop(ctx);
        let mut bare = wired(Some(0));
        bare.table
            .cards
            .retain(|card| !(card.zone == Some(fixtures::MAIN_DECK) && card.owner == 0));
        bare.resolve();
        bare.scripts = bare
            .scripts
            .clone()
            .with_script(TOME, &TOME_CARD)
            .with_script(LIBRARY, &WIRED_TO_EVERY_HELD_SPELL_FOR_THE_TEST);
        let mut ctx = bare.ctx();
        cast_the_tome(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} has no card to predict".to_string()));
    }

    #[test]
    fn wired_an_unheld_or_enemy_held_library_stays_quiet_for_my_spell() {
        let mut unheld = wired(None);
        unheld.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        let mut ctx = unheld.ctx();
        cast_the_tome(&mut ctx);
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            None,
            "nobody stands there, so nobody holds it"
        );
        assert!(library_items(&ctx).is_empty());
        assert!(ctx.blob.prompt.is_none());
        drop(ctx);
        let mut theirs = wired(Some(1));
        theirs.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        theirs.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        theirs.scripts = theirs
            .scripts
            .clone()
            .with_script(TOME, &TOME_CARD)
            .with_script(LIBRARY, &WIRED_TO_EVERY_HELD_SPELL_FOR_THE_TEST);
        let mut ctx = theirs.ctx();
        cast_the_tome(&mut ctx);
        assert!(library_items(&ctx).is_empty());
        assert_eq!(deck(&ctx, 0), [20, 21, 22, MY_TOP]);
    }

    #[test]
    fn today_a_four_energy_spell_under_the_held_library_predicts_nothing() {
        let mut fixture = library_held_by(Some(0));
        let mut ctx = fixture.ctx();
        cast_the_tome(&mut ctx);
        assert!(library_items(&ctx).is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(deck(&ctx, 0), [20, 21, 22, MY_TOP]);
    }

    #[test]
    #[ignore = "engine gap · the energy spent on a spell: PlayedSpell carries no card and no paid cost (the Yordle Explorer row); forgotten_library::energy_spent_on_the_spell is the reader"]
    fn a_four_energy_spell_under_the_held_library_predicts_and_a_cheaper_one_does_not() {
        let mut fixture = library_held_by(Some(0));
        let mut ctx = fixture.ctx();
        cast_the_tome(&mut ctx);
        assert_eq!(library_items(&ctx).len(), 1);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {MY_TOP}}}"), "skip".to_string()]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_TOP}}}")).unwrap();
        assert_eq!(deck(&ctx, 0), [MY_TOP, 20, 21, 22]);
    }
}
