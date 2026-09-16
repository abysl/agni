use super::prelude::{ask_discard, done, draw, play, unit, with_statics, ONE_ENERGY};
use super::{Card, Cost, Flow, Item, Keyword, Power, Stage, Static};
use crate::engine::ctx::Ctx;
use crate::state::ChainItem;

pub const DRAWS: usize = 2;
const STAGE_DISCARDED: u8 = 1;

fn paid_optional_additional_costs(ctx: &Ctx, item: &ChainItem) -> Vec<Cost> {
    let card = item.kind.source();
    let script = ctx.script(card);
    let mut paid = Vec::new();
    if item.paid_additional() {
        if let Some(additional) = script.and_then(|script| script.additional) {
            paid.push(additional);
        }
    }
    if item.repeated() {
        if let Some(repeat) = script.and_then(|script| {
            script.keywords.iter().find_map(|held| match held {
                Keyword::Repeat(cost) => Some(*cost),
                _ => None,
            })
        }) {
            paid.push(repeat);
        }
    }
    if item.accelerated() && ctx.card(card).is_some() {
        paid.push(ONE_ENERGY);
    }
    paid
}

pub fn an_optional_additional_cost_you_pay(ctx: &Ctx, item: &ChainItem, me: u32) -> bool {
    item.controller == ctx.controller(me) && !paid_optional_additional_costs(ctx, item).is_empty()
}

static RAINBOWS: [Power; 3] = [Power::Rainbow; 3];

pub fn discount_for(ctx: &Ctx, item: &ChainItem, me: u32) -> Cost {
    if item.controller != ctx.controller(me) {
        return Cost::FREE;
    }
    let paid = paid_optional_additional_costs(ctx, item);
    let energy = paid.iter().filter(|cost| cost.energy > 0).count();
    let rainbows = paid.len() - energy;
    Cost {
        energy: energy as u8,
        power: &RAINBOWS[..rainbows.min(RAINBOWS.len())],
    }
}

fn arcane_shift(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 != STAGE_DISCARDED {
        if let Some(ask) = ask_discard(ctx, item, STAGE_DISCARDED) {
            return Flow::Ask(ask);
        }
    }
    let drawn = draw(ctx, seat, DRAWS);
    ctx.narrate(format!(
        "{{card {}}} · {{seat {seat}}} draws {drawn}",
        item.kind.source()
    ));
    done()
}

pub static CARD: Card = with_statics(
    unit("Ezreal - Prodigy", &[], &[play(&[], arcane_shift)]),
    &[Static::PlayDiscount(discount_for)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{spell, RAINBOW};
    use crate::cards::{script_of, Domain, Power, Trigger};
    use crate::engine::cost::{self, Need};
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{
        ItemKind, Origin, PromptWhy, SLOT_ACCELERATE, SLOT_ADDITIONAL, SLOT_REPEAT,
    };
    use agni_plugin_sdk::table::CardInfo;

    const EZREAL: u32 = 90;
    const RAMPAGE: u32 = 91;
    const TRIBUTE: u32 = 92;
    const PLAIN: u32 = 93;

    const BODY: Cost = Cost {
        energy: 0,
        power: &[Power::Domain(Domain::Body)],
    };
    const TWO_ENERGY: Cost = Cost {
        energy: 2,
        power: &[],
    };

    static RAMPAGE_CARD: Card =
        crate::cards::prelude::with_additional(spell("Rampage", &[], &[]), BODY);
    static TRIBUTE_CARD: Card =
        crate::cards::prelude::with_additional(spell("Tribute", &[], &[]), TWO_ENERGY);
    static PLAIN_CARD: Card = spell("Spark", &[], &[]);
    static KEEPER_CARD: Card =
        crate::cards::prelude::with_additional(unit("Tribute", &[], &[]), TWO_ENERGY);

    fn ezreal(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(EZREAL, zone, 0, "Ezreal - Prodigy", 3)
        }
    }

    fn a_spell(id: u32, seat: u8, name: &str, energy: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, name, energy, 0);
        card.domain = vec!["Body".into()];
        card
    }

    fn workshop(ezreal_zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(ezreal(ezreal_zone));
        fixture.table.cards.push(a_spell(RAMPAGE, 0, "Rampage", 1));
        fixture.table.cards.push(a_spell(TRIBUTE, 0, "Tribute", 1));
        fixture.table.cards.push(a_spell(PLAIN, 0, "Spark", 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(47, 0, "Chaos", false));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(RAMPAGE, &RAMPAGE_CARD)
            .with_script(TRIBUTE, &TRIBUTE_CARD)
            .with_script(PLAIN, &PLAIN_CARD);
        fixture
    }

    fn spell_item(card: u32, seat: u8, paid: bool) -> ChainItem {
        let mut item = ChainItem::new(1, ItemKind::Spell { card }, seat, Origin::Hand);
        if paid {
            item.set_slot(SLOT_ADDITIONAL, 1);
        }
        item
    }

    #[test]
    fn the_script_is_a_play_trigger_and_a_spell_discount_static() {
        assert!(std::ptr::eq(script_of("Ezreal - Prodigy").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        assert!(!CARD.abilities[0].optional);
        assert_eq!(CARD.statics.len(), 1);
        assert!(CARD.has_static(Static::PlayDiscount(discount_for)));
        assert_eq!(DRAWS, 2);
    }

    #[test]
    fn the_discount_is_one_energy_or_one_rainbow_off_an_additional_cost_you_pay() {
        let mut fixture = workshop(fixtures::BASE);
        let ctx = fixture.ctx();
        assert_eq!(
            discount_for(&ctx, &spell_item(RAMPAGE, 0, true), EZREAL),
            RAINBOW,
            "a rune-only additional cost loses a rainbow"
        );
        assert_eq!(
            discount_for(&ctx, &spell_item(TRIBUTE, 0, true), EZREAL),
            ONE_ENERGY,
            "an energy additional cost loses one energy"
        );
        assert_eq!(
            discount_for(&ctx, &spell_item(RAMPAGE, 0, false), EZREAL),
            Cost::FREE,
            "not paid, not discounted"
        );
        assert_eq!(
            discount_for(&ctx, &spell_item(PLAIN, 0, true), EZREAL),
            Cost::FREE,
            "no additional cost to pay"
        );
        assert_eq!(
            discount_for(&ctx, &spell_item(RAMPAGE, 1, true), EZREAL),
            Cost::FREE,
            "the opponent's spell is not one you pay for"
        );
        let paid = cost::of_item(&ctx, &spell_item(RAMPAGE, 0, true), None);
        assert_eq!(paid.energy, 1);
        assert!(
            paid.power.is_empty(),
            "one Body less on the additional: {:?}",
            paid.power
        );
        let unpaid = cost::of_item(&ctx, &spell_item(RAMPAGE, 0, false), None);
        assert_eq!(unpaid.energy, 1);
        assert!(unpaid.power.is_empty());
        let tribute = cost::of_item(&ctx, &spell_item(TRIBUTE, 0, true), None);
        assert_eq!(tribute.energy, 2, "one plus two, less one");
        assert_eq!(tribute.power, Vec::<Need>::new());
    }

    #[test]
    fn playing_him_asks_for_one_discard_then_draws_two() {
        let mut fixture = workshop(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EZREAL).unwrap();
        assert!(ctx.on_board(EZREAL));
        let hand = ctx.hand_of(0).len();
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                stage: STAGE_DISCARDED,
                ..
            })
        ));
        assert_eq!(fixtures::labels(&ctx).len(), hand, "every card in hand");
        fixtures::choose(&mut ctx, 0, &format!("{{card {PLAIN}}}")).unwrap();
        assert!(ctx.in_trash(PLAIN));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), hand - 1 + DRAWS);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {EZREAL}}} · {{seat 0}} draws 2")));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_an_empty_hand_he_draws_two_without_asking() {
        let mut fixture = workshop(fixtures::HAND);
        fixture.table.cards.retain(|card| {
            card.zone != Some(fixtures::HAND) || card.id == EZREAL || card.seat == 1
        });
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, EZREAL).unwrap();
        assert!(ctx.hand_of(0).is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.hand_of(0).len(), DRAWS);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn paying_rampages_additional_cost_beside_him_strikes_the_body_rune() {
        let mut fixture = workshop(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let runes = ctx.runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, RAMPAGE).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(ctx.blob.chain[0].paid_additional());
        assert_eq!(
            ctx.runes_of(0).len(),
            runes,
            "no rune recycled · the Body power is struck"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        drop(ctx);

        let mut fixture = workshop(fixtures::HAND);
        let mut ctx = fixture.ctx();
        let runes = ctx.runes_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, RAMPAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "without him on the board the Body rune is recycled"
        );
    }

    #[test]
    fn a_units_additional_cost_is_discounted_too() {
        let mut fixture = workshop(fixtures::BASE);
        let keeper = fixture.table.card_mut(TRIBUTE).unwrap();
        keeper.kind = Some("Unit".into());
        keeper.might = Some(2);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(TRIBUTE, &KEEPER_CARD);
        let ctx = fixture.ctx();
        let mut item = ChainItem::new(1, ItemKind::Permanent { card: TRIBUTE }, 0, Origin::Hand);
        item.set_slot(SLOT_ADDITIONAL, 1);
        assert_eq!(discount_for(&ctx, &item, EZREAL), ONE_ENERGY);
        assert_eq!(
            cost::of_item(&ctx, &item, None).energy,
            2,
            "one plus two, less one"
        );
    }

    #[test]
    fn a_repeat_cost_is_an_optional_additional_cost_the_rules_example_discounts() {
        let mut fixture = workshop(fixtures::BASE);
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(PLAIN, &crate::cards::frigid_touch::CARD);
        let ctx = fixture.ctx();
        let mut item = ChainItem::new(1, ItemKind::Spell { card: PLAIN }, 0, Origin::Hand);
        item.set_slot(SLOT_REPEAT, 1);
        assert!(item.repeated());
        assert_eq!(
            discount_for(&ctx, &item, EZREAL),
            ONE_ENERGY,
            "356.4.c's own example: a repeated Frigid Touch is one energy cheaper beside him"
        );
        assert_eq!(cost::of_item(&ctx, &item, None).energy, 1 + 2 - 1);
        let mut accelerated =
            ChainItem::new(2, ItemKind::Permanent { card: TRIBUTE }, 0, Origin::Hand);
        accelerated.set_slot(SLOT_ACCELERATE, 1);
        assert_eq!(
            discount_for(&ctx, &accelerated, EZREAL),
            ONE_ENERGY,
            "805.2 · an Accelerate payment is an optional additional cost too"
        );
        accelerated.set_slot(SLOT_ADDITIONAL, 1);
        let both = discount_for(&ctx, &accelerated, EZREAL);
        assert_eq!(
            both.energy, 2,
            "each paid component is discounted on its own"
        );
        assert!(both.power.is_empty());
        let mut rampage = spell_item(RAMPAGE, 0, true);
        rampage.set_slot(SLOT_REPEAT, 1);
        let plain = discount_for(&ctx, &rampage, EZREAL);
        assert_eq!(plain, RAINBOW, "Rampage has no Repeat cost to discount");
    }
}
