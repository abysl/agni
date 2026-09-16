use super::prelude::{done, might_this_turn, triggered, unit};
use super::{Card, Flow, Item, Keyword, Stage, Trigger};
use crate::engine::ctx::Ctx;

pub const BONUS: i16 = 2;

fn moonfall(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    if !ctx.on_board(me) {
        return done();
    }
    might_this_turn(ctx, item, me, BONUS, None);
    ctx.narrate(format!("{{card {me}}} gets +{BONUS} Might this turn"));
    done()
}

pub static CARD: Card = unit(
    "Diana - No Longer Human",
    &[Keyword::Ambush],
    &[triggered(Trigger::YouPlaySpell, &[], moonfall)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spell, this_turn};
    use crate::cards::script_of;
    use crate::engine::ctx::{Event, Location, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::ItemKind;
    use agni_plugin_sdk::table::{CardInfo, Target};

    static REPLY: Card = spell("Reply", &[Keyword::Reaction], &[]);

    const DIANA: u32 = 90;
    const THEIR_REPLY: u32 = 91;

    fn diana(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(DIANA, zone, 0, "Diana - No Longer Human", 3)
        }
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn moonrise(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(diana(zone));
        let mut reply = fixtures::spell(THEIR_REPLY, fixtures::HAND, 1, "Reply", 1, 0);
        reply.domain = vec!["Mind".into()];
        fixture.table.cards.push(reply);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(THEIR_REPLY, &REPLY);
        assert!(std::ptr::eq(fixture.scripts.of_card(DIANA).unwrap(), &CARD));
        fixture
    }

    #[test]
    fn the_script_prints_ambush_and_one_trigger_on_her_controllers_spells() {
        assert!(std::ptr::eq(
            script_of("Diana - No Longer Human").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, &[Keyword::Ambush]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::YouPlaySpell);
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none());
        assert_eq!(BONUS, 2);
        let mut fixture = moonrise(fixtures::HAND);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.ambush_locations(0, DIANA),
            [Location::Battlefield(fixtures::BF1)],
            "Ambush reaches the battlefield where her side already stands"
        );
    }

    #[test]
    fn her_controllers_spell_gives_her_two_might_for_the_turn_once_the_spell_resolves() {
        let mut fixture = moonrise(fixtures::BF1);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(DIANA), 3);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(
            ctx.blob.queue.is_empty(),
            "nothing triggers before the spell resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::PlayedSpell { controller: 0, .. })));
        assert_eq!(ctx.blob.chain.len(), 1, "the trigger is on the chain");
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == DIANA
        ));
        assert!(ctx.blob.prompt.is_none(), "she asks nothing");
        assert_eq!(might_counter(&ctx, DIANA), 0, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(DIANA), 5);
        assert_eq!(might_counter(&ctx, DIANA), 2);
        assert_eq!(might_counter(&ctx, fixtures::VI), 0, "only she grows");
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {DIANA}}} gets +2 Might this turn")));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(DIANA), 3, "the bonus ends with the turn");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_opponents_spell_leaves_her_alone_and_so_does_a_spell_while_she_is_in_hand() {
        let mut fixture = moonrise(fixtures::BF1);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_REPLY).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2, "the reply sits on the spell");
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the opponent's reply resolved and queued nothing of hers"
        );
        assert!(matches!(ctx.blob.chain[0].kind, ItemKind::Spell { .. }));
        assert_eq!(might_counter(&ctx, DIANA), 0);
        drop(ctx);

        let mut held = moonrise(fixtures::HAND);
        let mut ctx = held.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty(), "a Diana in hand never triggers");
        assert_eq!(might_counter(&ctx, DIANA), 0);
    }
}
