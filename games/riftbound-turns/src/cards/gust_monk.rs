use super::akshan_mischievous::paid_additional_on_entry;
use super::prelude::{
    a_card, a_unit, card_target, done, grant_this_turn, paying_with, play, unit, when,
    with_additional, IN_TRASH, ONE_ENERGY,
};
use super::{Card, Flow, Item, Keyword, SelfCost, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 2;
pub const GRANTED: Keyword = Keyword::Assault(ASSAULT);

pub const KINDLING: TargetSpec = a_card(IN_TRASH, "a card in any trash to banish");
pub const GUSTED: TargetSpec = a_unit("a unit to give Assault 2 this turn");

fn gust(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 1) else {
        return done();
    };
    if grant_this_turn(ctx, unit, GRANTED) {
        ctx.narrate(format!(
            "{{card {unit}}} gets [Assault {ASSAULT}] this turn"
        ));
    }
    done()
}

pub static CARD: Card = with_additional(
    unit(
        "Gust Monk",
        &[],
        &[paying_with(
            when(play(&[KINDLING, GUSTED], gust), paid_additional_on_entry),
            SelfCost::BanishTarget,
        )],
    ),
    ONE_ENERGY,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, Cost, Filter, TargetKind, Trigger};
    use crate::engine::cost;
    use crate::engine::ctx::{Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef, SLOT_ADDITIONAL};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MONK: u32 = 90;
    const MY_BURIED: u32 = 91;
    const THEIR_BURIED: u32 = 92;

    fn monk(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(1),
            power: Some(0),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(MONK, zone, 0, "Gust Monk", 2)
        }
    }

    fn monastery(buried: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(monk(fixtures::HAND));
        if buried {
            fixture.table.cards.push(fixtures::spell(
                MY_BURIED,
                fixtures::TRASH,
                0,
                "Spark",
                1,
                0,
            ));
            fixture
                .table
                .cards
                .push(fixtures::unit(THEIR_BURIED, fixtures::TRASH, 1, "Brute", 4));
        }
        fixture.resolve();
        fixture
    }

    fn additional_confirm(ctx: &Ctx) -> bool {
        matches!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost { cost, .. }) if usize::from(cost) == SLOT_ADDITIONAL
        )
    }

    fn pending(ctx: &Ctx) -> u16 {
        ctx.blob
            .queue
            .first()
            .map(|pending| pending.item.id)
            .expect("the play trigger is pending")
    }

    fn ready_runes(ctx: &Ctx) -> usize {
        ctx.ready_runes_of(0).len()
    }

    #[test]
    fn the_script_prints_an_optional_energy_cost_and_a_two_target_trigger_gated_on_paying_it() {
        assert!(std::ptr::eq(script_of("Gust Monk").unwrap(), &CARD));
        assert_eq!(CARD.name, "Gust Monk");
        assert_eq!(CARD.additional, Some(ONE_ENERGY));
        assert_eq!(
            ONE_ENERGY,
            Cost {
                energy: 1,
                power: &[]
            }
        );
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(!ability.optional, "the may is the cost, not the gust");
        assert!(ability.condition.is_some());
        assert_eq!(
            ability.self_cost,
            SelfCost::BanishTarget,
            "383.3.b · the banish heads the effect, so it is the trigger's base cost paid at finalization"
        );
        assert_eq!(ability.targets, &[KINDLING, GUSTED]);
        assert_eq!((KINDLING.min, KINDLING.max), (1, 1));
        assert_eq!(KINDLING.kind, TargetKind::Card);
        assert_eq!(KINDLING.filter, Filter::InTrash, "any trash");
        assert_eq!((GUSTED.min, GUSTED.max), (1, 1));
        assert_eq!(GUSTED.filter, Filter::Unit);
        assert_eq!(GRANTED, Keyword::Assault(2));
    }

    #[test]
    fn the_additional_cost_adds_one_energy_only_when_the_slot_says_paid() {
        let mut fixture = monastery(true);
        let ctx = fixture.ctx();
        let mut held = ChainItem::new(1, ItemKind::Permanent { card: MONK }, 0, Origin::Hand);
        assert_eq!(cost::of_item(&ctx, &held, None).energy, 1);
        held.set_slot(SLOT_ADDITIONAL, 1);
        let paid = cost::of_item(&ctx, &held, None);
        assert_eq!(paid.energy, 2);
        assert!(paid.power.is_empty());
        assert!(held.paid_additional());
    }

    #[test]
    fn paying_the_energy_banishes_a_card_from_any_trash_and_the_unit_gets_assault_two() {
        let mut fixture = monastery(true);
        let mut ctx = fixture.ctx();
        let runes = ready_runes(&ctx);
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        assert!(additional_confirm(&ctx), "{:?}", ctx.blob.why);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(MONK), Some(Location::Base(0)));
        assert_eq!(
            ready_runes(&ctx),
            runes - 2,
            "two energy: the print and the extra"
        );
        let item = pending(&ctx);
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {MY_BURIED}}}"),
                format!("{{card {THEIR_BURIED}}}")
            ],
            "any trash · yours and theirs"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, item, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit on the board is not in a trash"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {THEIR_BURIED}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {}}}", fixtures::SPRITE),
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {MONK}}}"),
            ],
            "any unit, the monk included"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == MONK
        ));
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(THEIR_BURIED), TargetRef::Card(fixtures::VI)]
        );
        assert!(
            ctx.in_banishment(THEIR_BURIED),
            "the banish is the cost, paid as the trigger is finalized"
        );
        assert!(ctx.events.contains(&Event::Banished {
            card: THEIR_BURIED,
            owner: 1,
            by: 0,
            token: false
        }));
        assert!(ctx.in_trash(MY_BURIED), "only the pick");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {THEIR_BURIED}}} is banished for the {{card {MONK}}} ability"
        )));
        assert!(
            !ctx.has_keyword(fixtures::VI, GRANTED),
            "not before it resolves"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.has_keyword(fixtures::VI, GRANTED));
        assert!(!ctx.has_keyword(MONK, GRANTED), "only the pick");
        assert_eq!(
            ctx.state_of(fixtures::VI).unwrap().granted,
            [(GRANTED, this_turn(&ctx))]
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} gets [Assault 2] this turn",
            fixtures::VI
        )));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert!(
            !ctx.has_keyword(fixtures::VI, GRANTED),
            "it ends with the turn"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_banished_card_is_no_target_a_response_can_save_and_the_unit_alone_can_be_lost() {
        let mut fixture = monastery(true);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {MY_BURIED}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(
            ctx.in_banishment(MY_BURIED),
            "355.10.c · a cost, paid already"
        );
        ctx.kill(fixtures::VI, crate::engine::ctx::Cause::Rule);
        assert!(!ctx.on_board(fixtures::VI), "the unit dies in response");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.in_banishment(MY_BURIED), "the cost is not refunded");
        assert!(!ctx.has_keyword(fixtures::VI, GRANTED));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn declining_the_energy_plays_him_for_one_and_asks_for_no_target() {
        let mut fixture = monastery(true);
        let mut ctx = fixture.ctx();
        let runes = ready_runes(&ctx);
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        assert!(additional_confirm(&ctx));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.location(MONK), Some(Location::Base(0)));
        assert_eq!(ready_runes(&ctx), runes - 1);
        assert!(ctx.blob.prompt.is_none(), "unpaid, no target prompt");
        assert!(ctx.blob.chain.is_empty(), "unpaid, the trigger never fires");
        assert!(ctx.in_trash(MY_BURIED));
        assert!(ctx.in_trash(THEIR_BURIED));
        assert!(!ctx.has_keyword(fixtures::VI, GRANTED));
    }

    #[test]
    fn with_every_trash_empty_the_paid_trigger_has_no_card_to_banish() {
        let mut fixture = monastery(false);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MONK).unwrap();
        assert!(additional_confirm(&ctx));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(MONK), Some(Location::Base(0)));
        assert!(ctx.blob.prompt.is_none(), "no candidate, no prompt");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.has_keyword(fixtures::VI, GRANTED));
        assert!(!ctx.has_keyword(MONK, GRANTED));
    }
}
