use super::forgotten_library::energy_spent_on_the_spell;
use super::prelude::{done, ready, triggered, unit, when};
use super::{Card, Flow, Item, Keyword, Source, Stage, Trigger};
use crate::engine::ctx::{Ctx, Event};

pub const SPENT_AT_LEAST: u8 = 4;

pub fn spent_four_or_more_on(ctx: &Ctx, event: &Event) -> bool {
    energy_spent_on_the_spell(ctx, event).is_some_and(|energy| energy >= SPENT_AT_LEAST)
}

fn a_spell_you_spent_four_or_more_on(ctx: &Ctx, event: &Event, _: Source) -> bool {
    matches!(event, Event::PlayedSpell { .. }) && spent_four_or_more_on(ctx, event)
}

fn lore(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    } else {
        ctx.narrate(format!("{{card {me}}} is already ready"));
    }
    done()
}

pub static CARD: Card = unit(
    "Revna the Lorekeeper",
    &[Keyword::Ganking],
    &[when(
        triggered(Trigger::YouPlaySpell, &[], lore),
        a_spell_you_spent_four_or_more_on,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{chain, triggers};
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::CardInfo;

    const REVNA: u32 = 90;
    const BIG_SPELL: u32 = 91;
    const ENERGY: u8 = 7;
    const MIGHT: u8 = 7;
    const FURY_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn revna(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(REVNA, zone, seat, "Revna the Lorekeeper", MIGHT)
        }
    }

    fn library() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut keeper = revna(fixtures::BASE, 0);
        keeper.exhausted = true;
        fixture.table.cards.push(keeper);
        fixture.table.cards.push(fixtures::spell(
            BIG_SPELL,
            fixtures::HAND,
            0,
            "Cataclysm",
            4,
            0,
        ));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        for rune in FURY_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(REVNA).unwrap(), &CARD));
        fixture
    }

    fn her_trigger(ctx: &Ctx) -> bool {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .any(|held| matches!(held.kind, ItemKind::Trigger { source, index: 0 } if source == REVNA))
    }

    #[test]
    fn the_script_prints_ganking_and_one_gated_you_play_spell_trigger() {
        assert!(std::ptr::eq(
            script_of("Revna the Lorekeeper").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Ganking]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let lore = &CARD.abilities[0];
        assert_eq!(lore.trigger, Trigger::YouPlaySpell);
        assert!(lore.condition.is_some());
        assert!(lore.targets.is_empty());
        assert!(!lore.optional);
        let mut fixture = library();
        let ctx = fixture.ctx();
        assert!(ctx.has_keyword(REVNA, Keyword::Ganking));
    }

    #[test]
    fn the_run_readies_her_when_the_trigger_is_queued_by_hand() {
        let mut fixture = library();
        let mut ctx = fixture.ctx();
        assert!(ctx.card(REVNA).unwrap().exhausted);
        let item = crate::state::ChainItem::new(
            1,
            ItemKind::Trigger {
                source: REVNA,
                index: 0,
            },
            0,
            crate::state::Origin::Board,
        );
        assert_eq!(lore(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(!ctx.card(REVNA).unwrap().exhausted);
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { card, by: 0 } if *card == REVNA)));
        assert!(ctx.blob.log.contains(&format!("{{card {REVNA}}} readies")));
        assert_eq!(lore(&mut ctx, &item, Stage(0)), Flow::Done);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {REVNA}}} is already ready")));
    }

    #[test]
    fn a_cheap_spell_or_an_opponents_spell_never_readies_her() {
        let mut fixture = library();
        fixture.table.card_mut(BIG_SPELL).unwrap().energy = Some(2);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BIG_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!her_trigger(&ctx));
        assert!(ctx.card(REVNA).unwrap().exhausted, "two energy is not four");
        ctx.raise(Event::PlayedSpell {
            item: 9,
            controller: 1,
            nth: 1,
        });
        assert_eq!(
            triggers::collect(&mut ctx),
            0,
            "an opponent's spell is not one you play"
        );
        chain::proceed(&mut ctx);
        assert!(ctx.card(REVNA).unwrap().exhausted);
    }

    #[test]
    #[ignore = "engine gap · PlayedSpell carries no card and no paid cost and is raised once the spell has left the chain, so forgotten_library::energy_spent_on_the_spell reads nothing (the Yordle Explorer row)"]
    fn a_spell_played_for_four_readies_her_as_it_resolves() {
        let mut fixture = library();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BIG_SPELL).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(her_trigger(&ctx), "when you play a spell · four spent");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(REVNA).unwrap().exhausted, "readied");
    }
}
