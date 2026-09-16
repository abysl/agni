use super::prelude::{done, equip, gear, on_conquer_me, spawn_gold, while_attached, with_statics};
use super::{Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage, GRANTED};
use crate::engine::ctx::Ctx;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Chaos)],
};

pub const MIGHT_BONUS: i16 = 1;
pub const ON_CONQUER: u8 = GRANTED;

fn plunder(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    spawn_gold(ctx, item.controller, false);
    done()
}

pub static WEARER_TEXT: [Ability; 1] = [on_conquer_me(&[], plunder)];

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS), Grant::Ability(&WEARER_TEXT)];

pub static CARD: Card = with_statics(
    gear("Cull", &[Keyword::Equip(EQUIP)], &[equip(EQUIP)]),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::cards::prelude::{attach_gear, detach_gear, Attached, Location};
    use crate::cards::{script_of, Event, Static, Trigger, Who, TOKEN_GOLD};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::play::STAGE_TARGET;
    use crate::engine::{settle, triggers};
    use crate::state::{ChainItem, ItemKind, Needs, Origin, Pending, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    pub const GEAR: u32 = 90;

    pub fn equipment(id: u32, seat: u8, name: &str, energy: u8, domain: &str) -> CardInfo {
        let mut card = fixtures::gear(id, fixtures::BASE, seat, name, energy);
        card.domain = vec![domain.into()];
        card
    }

    pub fn conquered(units: &[u32]) -> Event {
        Event::Conquered {
            zone: fixtures::BF1,
            seat: 0,
            units: units.to_vec(),
        }
    }

    pub fn held(units: &[u32]) -> Event {
        Event::Held {
            zone: fixtures::BF1,
            seat: 0,
            units: units.to_vec(),
        }
    }

    pub fn queue_trigger(ctx: &mut Ctx, source: u32, index: u8, subject: TargetRef) {
        queue_kind(ctx, ItemKind::Trigger { source, index }, subject);
    }

    pub fn queue_granted(ctx: &mut Ctx, wearer: u32, gear: u32, index: u8, subject: TargetRef) {
        queue_kind(
            ctx,
            ItemKind::Granted {
                holder: wearer,
                lender: gear,
                index,
            },
            subject,
        );
    }

    pub fn granted_kinds(ctx: &Ctx) -> Vec<ItemKind> {
        ctx.blob
            .queue
            .iter()
            .map(|pending| pending.item.kind)
            .chain(ctx.blob.chain.iter().map(|item| item.kind))
            .filter(|kind| matches!(kind, ItemKind::Granted { .. }))
            .collect()
    }

    fn queue_kind(ctx: &mut Ctx, kind: ItemKind, subject: TargetRef) {
        let id = ctx.blob.next_item_id();
        let mut item = ChainItem::new(id, kind, 0, Origin::Board);
        item.stage = STAGE_TARGET;
        item.subject = Some(subject);
        ctx.blob.queue.push(Pending {
            item,
            needs: Needs::Choices,
        });
        settle(ctx).unwrap();
    }

    pub fn golds_of(ctx: &Ctx, seat: u8) -> usize {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_GOLD && card.owner == seat)
            .filter(|card| ctx.on_board(card.id))
            .count()
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture
            .table
            .cards
            .push(equipment(GEAR, 0, "Cull", 1, "Chaos"));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_chaos_equipment_with_a_might_bonus_of_one_and_a_wearer_conquer_listener() {
        assert!(std::ptr::eq(script_of("Cull").unwrap(), &CARD));
        assert_eq!(CARD.keywords, &[Keyword::Equip(EQUIP)]);
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].label, Some("equip"));
        let listener = &WEARER_TEXT[0];
        assert_eq!(listener.trigger, Trigger::Conquer(Who::Me));
        assert!(listener.condition.is_none(), "136.2.c · I am the wearer");
        assert!(listener.targets.is_empty());
        assert!(!listener.optional);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(
            CARD.attached_grants(),
            [Grant::Might(1), Grant::Ability(_)]
        ));
    }

    #[test]
    fn the_listener_matches_the_wearer_among_the_conquering_units_and_nothing_else() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(
            triggers::find(&ctx, &conquered(&[fixtures::VI])).is_empty(),
            "loose gear has no wearer"
        );
        assert_eq!(attach_gear(&mut ctx, GEAR, fixtures::VI), Attached::Yes);
        assert_eq!(ctx.current_might(fixtures::VI), 4, "136.3 · the bonus");
        let found = triggers::find(&ctx, &conquered(&[fixtures::VI]));
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].kind(),
            ItemKind::Granted {
                holder: fixtures::VI,
                lender: GEAR,
                index: ON_CONQUER
            },
            "136.2.c · the wearer holds the text, the gear lends it"
        );
        assert_eq!(found[0].controller, 0);
        assert!(triggers::find(&ctx, &held(&[fixtures::VI])).is_empty());
        assert!(triggers::find(&ctx, &conquered(&[fixtures::SPRITE])).is_empty());
        assert!(triggers::find(&ctx, &Event::Attacks { card: fixtures::VI }).is_empty());
        assert!(detach_gear(&mut ctx, GEAR));
        assert!(triggers::find(&ctx, &conquered(&[fixtures::VI])).is_empty());
    }

    #[test]
    fn the_wearers_conquer_plays_a_gold_exhausted_for_the_controller() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        assert_eq!(golds_of(&ctx, 0), 0);
        queue_granted(
            &mut ctx,
            fixtures::VI,
            GEAR,
            ON_CONQUER,
            TargetRef::Zone(fixtures::BF1),
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(golds_of(&ctx, 0), 1);
        assert_eq!(golds_of(&ctx, 1), 0);
        let gold = ctx
            .table
            .cards
            .iter()
            .find(|card| card.name == TOKEN_GOLD && card.owner == 0)
            .unwrap();
        assert!(gold.exhausted, "played exhausted");
        assert_eq!(ctx.location(gold.id), Some(Location::Base(0)));
        assert!(ctx.blob.log.contains(&"{seat 0} gains a Gold".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_conquer_by_the_wearer_queues_the_gold_trigger() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, GEAR, fixtures::VI);
        ctx.raise(conquered(&[fixtures::VI]));
        assert_eq!(triggers::collect(&mut ctx), 1);
        settle(&mut ctx).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(golds_of(&ctx, 0), 1);
        ctx.raise(conquered(&[fixtures::THEIR_UNIT]));
        assert_eq!(triggers::collect(&mut ctx), 0, "another unit's conquer");
    }
}
