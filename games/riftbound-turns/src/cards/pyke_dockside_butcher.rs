use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{done, might_this_turn, play, ready, unit, when, with_additional};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;

pub const ADDITIONAL: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};
pub const BONUS: i16 = 2;

fn gutted(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    if ready(ctx, me) {
        ctx.narrate(format!("{{card {me}}} readies"));
    }
    might_this_turn(ctx, item, me, BONUS, None);
    ctx.narrate(format!("{{card {me}}} gets +{BONUS} Might this turn"));
    done()
}

pub static CARD: Card = with_additional(
    unit(
        "Pyke - Dockside Butcher",
        &[Keyword::Hidden, Keyword::Ganking],
        &[when(play(&[], gutted), paid_additional_on_entry)],
    ),
    ADDITIONAL,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, SLOT_ADDITIONAL};
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::CardInfo;

    const PYKE: u32 = 90;
    const ENERGY: u8 = 3;
    const MIGHT: u8 = 2;
    const FURY_RUNE: u32 = 46;

    fn pyke(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: None,
            domain: vec!["Fury".into()],
            ..fixtures::unit(PYKE, zone, seat, "Pyke - Dockside Butcher", MIGHT)
        }
    }

    fn docks() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(pyke(fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(FURY_RUNE, 0, "Fury", false));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(PYKE).unwrap(), &CARD));
        fixture
    }

    fn recycled(ctx: &Ctx) -> usize {
        ctx.effects
            .iter()
            .filter(|effect| {
                matches!(
                    effect,
                    Effect::Move {
                        zone: fixtures::RUNE_DECK,
                        ..
                    }
                )
            })
            .count()
    }

    fn his_trigger(ctx: &Ctx) -> bool {
        ctx.blob
            .chain
            .iter()
            .chain(ctx.blob.queue.iter().map(|pending| &pending.item))
            .any(|held| matches!(held.kind, ItemKind::Trigger { source, index: 0 } if source == PYKE))
    }

    #[test]
    fn the_script_prints_hidden_and_ganking_a_fury_additional_cost_and_one_gated_play_trigger() {
        assert!(std::ptr::eq(
            script_of("Pyke - Dockside Butcher").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Hidden, Keyword::Ganking]);
        assert_eq!(CARD.additional, Some(ADDITIONAL));
        assert_eq!(CARD.abilities.len(), 1);
        let gut = &CARD.abilities[0];
        assert_eq!(gut.trigger, Trigger::Play);
        assert!(gut.condition.is_some());
        assert!(!gut.optional);
        assert!(gut.targets.is_empty());
        assert!(CARD.statics.is_empty());
        let mut fixture = docks();
        let ctx = fixture.ctx();
        let mut item = ChainItem::new(1, ItemKind::Permanent { card: PYKE }, 0, Origin::Hand);
        assert_eq!(cost::of_item(&ctx, &item, None).energy, ENERGY);
        assert_eq!(cost::of_item(&ctx, &item, None).power, []);
        item.set_slot(SLOT_ADDITIONAL, 1);
        assert_eq!(
            cost::of_item(&ctx, &item, None).power,
            [Need::Domain(Domain::Fury)]
        );
    }

    #[test]
    fn paid_he_readies_himself_and_gets_two_might_for_the_turn() {
        let mut fixture = docks();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PYKE).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL
        ));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(PYKE), Some(Location::Base(0)));
        assert_eq!(
            recycled(&ctx),
            1,
            "a Fury rune recycles for the additional power: {:?}",
            ctx.effects
        );
        assert!(
            his_trigger(&ctx),
            "356.4.f.1 · paid means the trigger fires"
        );
        assert!(
            ctx.card(PYKE).unwrap().exhausted,
            "exhausted until it resolves"
        );
        assert_eq!(ctx.current_might(PYKE), i32::from(MIGHT));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.card(PYKE).unwrap().exhausted,
            "readied by his own trigger"
        );
        assert_eq!(ctx.current_might(PYKE), i32::from(MIGHT) + i32::from(BONUS));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Readied { card, by: 0 } if *card == PYKE)));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PYKE}}} gets +{BONUS} Might this turn")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn unpaid_he_enters_exhausted_at_his_printed_might_with_no_trigger() {
        let mut fixture = docks();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PYKE).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(!his_trigger(&ctx), "the trigger never fires unpaid");
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(PYKE), Some(Location::Base(0)));
        assert!(ctx.card(PYKE).unwrap().exhausted);
        assert_eq!(ctx.current_might(PYKE), i32::from(MIGHT));
        assert_eq!(recycled(&ctx), 0, "no rune was recycled");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn without_a_fury_rune_to_recycle_the_additional_cost_is_not_offered() {
        let mut fixture = docks();
        fixture.table.cards.retain(|card| card.id != FURY_RUNE);
        for rune in [fixtures::RUNE_A, 41, 43] {
            fixture.table.card_mut(rune).unwrap().domain = vec!["Calm".into()];
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, PYKE).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no Fury to pay: the ask is skipped, {:?}",
            ctx.blob.why
        );
        assert!(!his_trigger(&ctx));
        assert_eq!(ctx.location(PYKE), Some(Location::Base(0)));
    }
}
