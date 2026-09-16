use super::prelude::{
    a_card, asking, card_target, done, play, spell, with_candidates, with_statics, Location, Price,
};
use super::starhound::is_companion;
use super::the_harrowing::play_from_trash;
use super::{Card, Cost, Filter, Flow, Item, Stage, Static, TargetSpec, KIND_UNIT};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const ENERGY_AT_MOST: u8 = 2;
pub const POWER_AT_MOST: u8 = 1;
pub const DISCOUNT: Cost = Cost {
    energy: 2,
    power: &[],
};
pub const QUESTION: &str = "where the unit from your trash is played";
pub const LOCATE: u8 = 1;

pub const CHEAP_UNIT_IN_YOUR_TRASH: TargetSpec = a_card(
    Filter::And(&[
        Filter::Kind(KIND_UNIT),
        Filter::InTrash,
        Filter::Friendly,
        Filter::EnergyAtMost(ENERGY_AT_MOST),
        Filter::PowerAtMost(POWER_AT_MOST),
    ]),
    "a unit in your trash costing 2 or less with one power or less",
);

pub fn chosen_for(ctx: &Ctx, card: u32) -> Option<u32> {
    let targets = ctx
        .blob
        .queue
        .iter()
        .map(|pending| &pending.item)
        .chain(ctx.blob.chain.iter())
        .find(|item| item.kind.card() == Some(card))
        .map(|item| item.targets.clone())?;
    match targets.first()? {
        TargetRef::Card(unit) => Some(*unit),
        _ => None,
    }
}

fn discount(ctx: &Ctx, card: u32, _: u8) -> Cost {
    match chosen_for(ctx, card) {
        Some(unit) if is_companion(ctx, unit) => DISCOUNT,
        _ => Cost::FREE,
    }
}

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    if stage.0 == LOCATE {
        return location_options(ctx, item.controller);
    }
    Vec::new()
}

fn loyalty(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if stage.0 == LOCATE {
        let Some(at) = ctx
            .picks()
            .first()
            .and_then(|zone| u16::try_from(*zone).ok())
            .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
            .filter(|at| ctx.play_locations(seat).contains(at))
        else {
            return done();
        };
        play_from_trash(ctx, item, unit, vec![at], Price::Free);
        return done();
    }
    let locations = ctx.play_locations(seat);
    match locations.as_slice() {
        [] => done(),
        [only] => {
            play_from_trash(ctx, item, unit, vec![*only], Price::Free);
            done()
        }
        _ => Flow::Ask(ctx.ask_resume(item, LOCATE, 1, 1)),
    }
}

pub static CARD: Card = with_statics(
    spell(
        "Undying Loyalty",
        &[],
        &[asking(
            with_candidates(play(&[CHEAP_UNIT_IN_YOUR_TRASH], loyalty), candidates),
            QUESTION,
        )],
    ),
    &[Static::SelfDiscount(discount)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::the_harrowing::tests::DRAWS;
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, prompts};
    use crate::state::{Leave, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const LOYALTY: u32 = 90;
    const THEIR_LOYALTY: u32 = 91;
    const PORO: u32 = 92;
    const CRAB: u32 = 93;
    const DEAR: u32 = 94;
    const RAINBOW_HEAVY: u32 = 95;
    const THEIR_PORO: u32 = 96;
    const ORDER_RUNE: u32 = 100;

    fn loyalty_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Undying Loyalty", 2, 1);
        card.domain = vec!["Order".into()];
        card
    }

    fn trashed(id: u32, seat: u8, name: &str, energy: u8, power: u8) -> CardInfo {
        CardInfo {
            energy: Some(energy),
            power: Some(power),
            domain: vec!["Order".into()],
            ..fixtures::unit(id, fixtures::TRASH, seat, name, 2)
        }
    }

    fn kennel() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(loyalty_card(LOYALTY, 0));
        fixture.table.cards.push(loyalty_card(THEIR_LOYALTY, 1));
        fixture
            .table
            .cards
            .push(trashed(PORO, 0, "Daring Poro", 2, 1));
        fixture.table.cards.push(trashed(CRAB, 0, "Crab", 1, 0));
        fixture.table.cards.push(trashed(DEAR, 0, "Titan", 3, 1));
        fixture
            .table
            .cards
            .push(trashed(RAINBOW_HEAVY, 0, "Twin", 2, 2));
        fixture
            .table
            .cards
            .push(trashed(THEIR_PORO, 1, "Daring Poro", 2, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(CRAB, &DRAWS);
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> EntryMove {
        EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_targets_a_cheap_unit_in_your_trash_and_discounts_itself_for_a_companion() {
        assert!(std::ptr::eq(script_of("Undying Loyalty").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets, [CHEAP_UNIT_IN_YOUR_TRASH]);
        assert_eq!(ability.question, Some(QUESTION));
        assert!(ability.candidates.is_some());
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert!(CARD.has_static(Static::SelfDiscount(discount)));
        assert_eq!((ENERGY_AT_MOST, POWER_AT_MOST), (2, 1));
        assert_eq!(DISCOUNT.energy, 2);
    }

    #[test]
    fn a_plain_unit_is_paid_in_full_and_played_from_the_trash_ignoring_its_cost() {
        let mut fixture = kennel();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, LOYALTY).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 93}", "cancel"],
            "the Titan costs three, the Twin needs two power, the opponent's Poro is theirs"
        );
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "two energy and one Order power from four ready runes · no discount for a Crab"
        );
        assert_eq!(ctx.runes_of(0).len(), 4, "one rune recycled for the power");
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Chosen { card, by: 0, .. } if *card == CRAB
        )));
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "one play location · no question · {:?}",
            ctx.blob.log
        );
        assert_eq!(ctx.location(CRAB), Some(Location::Base(0)));
        assert!(ctx.card(CRAB).unwrap().exhausted);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "ignoring its cost · nothing more is paid"
        );
        assert_eq!(ctx.runes_of(0).len(), 4);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Trash { leave: Leave::Recycle }, .. } if *card == CRAB
        )));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.hand_of(0).len(),
            hand - 1 + 1,
            "the Crab's own play trigger fires again and draws"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} plays {card 93} from the trash for nothing".to_string()));
        assert_eq!(ctx.card(LOYALTY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn choosing_a_poro_takes_two_energy_off_and_a_held_battlefield_is_asked_for() {
        let mut fixture = kennel();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert!(is_companion(&ctx, PORO));
        assert!(!is_companion(&ctx, CRAB));
        assert_eq!(chosen_for(&ctx, LOYALTY), None);
        fixtures::play_from_hand(&mut ctx, 0, LOYALTY).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(chosen_for(&ctx, LOYALTY), Some(PORO));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "the Order rune recycled for the power and nothing exhausted · the Poro takes the two energy off"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: LOCATE
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "{zone 9}"],
            "the base and the held battlefield"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {LOYALTY}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert_eq!(
            ctx.location(PORO),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "ignoring its cost");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_unit_that_left_the_trash_is_left_alone_and_the_spell_is_still_spent() {
        let mut fixture = kennel();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LOYALTY).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        let hand = ctx.zones.hand.unwrap();
        ctx.table
            .apply_entry(&fixtures::move_action(CRAB, hand, 0), 0)
            .unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(CRAB).unwrap().zone, Some(fixtures::HAND));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("from the trash")));
        assert_eq!(ctx.card(LOYALTY).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_dear_a_power_heavy_or_an_enemy_unit_is_refused_and_the_opponents_copy_waits() {
        let mut fixture = kennel();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_LOYALTY)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, LOYALTY).unwrap();
        for wrong in [
            DEAR,
            RAINBOW_HEAVY,
            THEIR_PORO,
            fixtures::VI,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a cheap unit in your trash"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(LOYALTY).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · tags on the face: CardInfo carries no tags, so starhound::is_companion matches the printed names in starhound::COMPANIONS and a Bird, Cat, Dog or Poro under a name the list does not know pays the full two energy (the Poro Herder / Herald of Scales row)"]
    fn a_companion_the_list_does_not_know_still_takes_two_energy_off() {
        let mut fixture = kennel();
        fixture.table.card_mut(PORO).unwrap().name = "Unlisted Poro".into();
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LOYALTY).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "one Order power only · the tag takes the two energy off"
        );
    }

    #[test]
    fn a_seat_that_can_only_afford_the_discounted_price_may_start_the_play() {
        let mut fixture = kennel();
        for rune in [41, 42, 43] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        fixtures::play_from_hand(&mut ctx, 0, LOYALTY).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(PORO), Some(Location::Base(0)));
    }
}
