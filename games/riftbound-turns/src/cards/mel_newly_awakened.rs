use super::prelude::{done, draw, empower, is_empowered, play, unit, with_statics};
use super::{Card, Cost, Flow, Item, Keyword, Stage, Static};
use crate::engine::ctx::Ctx;
use crate::engine::statics;
use crate::state::{ChainItem, ItemKind, TargetRef};

pub const EMPOWER: Cost = Cost {
    energy: 3,
    power: &[],
};
pub const DRAWS: usize = 1;
pub const EXTRA_PENALTY: i16 = -1;
pub const EMPOWER_ABILITY: u8 = 0;
pub const AWAKEN: u8 = 1;

pub fn empowered_mels_of(ctx: &Ctx, seat: u8) -> u8 {
    let mut mels: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|held| ctx.controller(held.id) == seat)
        .filter(|held| statics::in_play(ctx, held.id) && is_empowered(ctx, held.id))
        .map(|held| held.id)
        .collect();
    mels.sort_unstable();
    u8::try_from(mels.len()).unwrap_or(u8::MAX)
}

fn a_spell_or_ability(item: &ChainItem) -> bool {
    matches!(
        item.kind,
        ItemKind::Spell { .. } | ItemKind::Ability { .. } | ItemKind::Trigger { .. }
    )
}

pub fn cannot_be_countered(ctx: &Ctx, item: &ChainItem) -> bool {
    a_spell_or_ability(item) && empowered_mels_of(ctx, item.controller) > 0
}

pub fn extra_penalty(ctx: &Ctx, item: &ChainItem, unit: u32) -> i16 {
    if !a_spell_or_ability(item) || !item.targets.contains(&TargetRef::Card(unit)) {
        return 0;
    }
    EXTRA_PENALTY.saturating_mul(i16::from(empowered_mels_of(ctx, item.controller)))
}

pub fn amplified(ctx: &Ctx, item: &ChainItem, unit: u32, delta: i16) -> i16 {
    if delta < 0 {
        delta.saturating_add(extra_penalty(ctx, item, unit))
    } else {
        delta
    }
}

fn awaken(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    draw(ctx, item.controller, DRAWS);
    done()
}

pub static CARD: Card = with_statics(
    unit(
        "Mel, Newly Awakened",
        &[Keyword::Empower(EMPOWER)],
        &[empower(EMPOWER), play(&[], awaken)],
    ),
    &[Static::ProtectsFromCounter(cannot_be_countered)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, SelfCost, Timing, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MEL: u32 = 90;
    const THEIR_MEL: u32 = 91;
    const THEIR_DEFY: u32 = 92;
    const STUPEFY: u32 = 93;
    const BRUTE: u32 = 94;

    fn mel(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(id, zone, seat, "Mel, Newly Awakened", 4)
        }
    }

    fn salon(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mel(MEL, zone, 0));
        let mut defy = fixtures::spell(THEIR_DEFY, fixtures::HAND, 1, "Defy", 1, 0);
        defy.domain = vec!["Mind".into()];
        fixture.table.cards.push(defy);
        let mut stupefy = fixtures::spell(STUPEFY, fixtures::HAND, 0, "Stupefy", 1, 0);
        stupefy.domain = vec!["Calm".into()];
        fixture.table.cards.push(stupefy);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BASE, 1, "Brute", 4));
        for id in [41, 42, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(MEL).unwrap(), &CARD));
        fixture
    }

    fn her_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == MEL)
            .collect()
    }

    fn empower_her(ctx: &mut Ctx) {
        activate::activate(ctx, 0, MEL, EMPOWER_ABILITY).unwrap();
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.is_empowered(MEL));
    }

    fn spell_of(seat: u8, card: u32, targets: &[TargetRef]) -> ChainItem {
        let mut item = ChainItem::new(7, ItemKind::Spell { card }, seat, Origin::Hand);
        item.targets = targets.to_vec();
        item
    }

    #[test]
    fn the_champion_prints_empower_and_carries_a_play_draw_and_the_empower_activation() {
        assert!(std::ptr::eq(
            script_of("Mel, Newly Awakened").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert!(matches!(
            CARD.statics,
            [Static::ProtectsFromCounter(protects)] if std::ptr::fn_addr_eq(*protects, cannot_be_countered as fn(&Ctx, &ChainItem) -> bool)
        ));
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[usize::from(EMPOWER_ABILITY)];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        assert_eq!(empower.label, Some("empower"));
        let awaken = &CARD.abilities[usize::from(AWAKEN)];
        assert_eq!(awaken.trigger, Trigger::Play);
        assert!(awaken.targets.is_empty());
        assert!(!awaken.optional);
        assert_eq!((DRAWS, EXTRA_PENALTY), (1, -1));
    }

    #[test]
    fn playing_her_draws_one_when_the_trigger_resolves() {
        let mut fixture = salon(fixtures::HAND);
        fixture.table.card_mut(40).unwrap().exhausted = false;
        fixture.table.card_mut(40).unwrap().domain = vec!["Mind".into()];
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Mind", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, MEL).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: AWAKEN }) if source == MEL
        ));
        assert_eq!(ctx.hand_of(0).len(), hand - 1, "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.hand_of(0).len(),
            hand,
            "she left the hand, one card came in"
        );
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Drew { seat: 0, .. })));
        assert!(ctx.on_board(MEL));
        assert!(!ctx.is_empowered(MEL));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn empower_pays_three_energy_once_and_a_second_empower_is_refused() {
        let mut fixture = salon(fixtures::BASE);
        let mut ctx = fixture.ctx();
        let offers = her_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {MEL}}}: empower (3 energy)")
        );
        assert!(offers[0].enabled);
        assert_eq!(empowered_mels_of(&ctx, 0), 0);
        activate::activate(&mut ctx, 0, MEL, EMPOWER_ABILITY).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            0,
            "three energy from three runes"
        );
        assert!(
            !ctx.card(MEL).unwrap().exhausted,
            "827.1 · Empower never exhausts her"
        );
        assert!(!is_empowered(&ctx, MEL), "nothing until it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(is_empowered(&ctx, MEL));
        assert_eq!(empowered_mels_of(&ctx, 0), 1);
        assert_eq!(empowered_mels_of(&ctx, 1), 0);
        assert!(her_offers(&ctx).is_empty(), "377.2.b · the offer is gone");
        assert_eq!(
            activate::activate(&mut ctx, 0, MEL, EMPOWER_ABILITY),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_ready_runes_grey_the_offer_and_refuse_the_activation() {
        let mut fixture = salon(fixtures::BASE);
        fixture.table.card_mut(43).unwrap().exhausted = true;
        let mut ctx = fixture.ctx();
        let offers = her_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert!(!offers[0].enabled);
        assert_eq!(
            activate::activate(&mut ctx, 0, MEL, EMPOWER_ABILITY),
            Err(Refusal::NotEnoughRunes {
                needed: 3,
                ready: 2
            })
        );
        assert!(!ctx.is_empowered(MEL));
    }

    #[test]
    fn the_seams_read_her_controllers_items_only_while_she_is_empowered_and_in_play() {
        let mut fixture = salon(fixtures::BASE);
        fixture.table.cards.push(mel(THEIR_MEL, fixtures::BASE, 1));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        let mine = spell_of(0, STUPEFY, &[TargetRef::Card(BRUTE)]);
        let theirs = spell_of(1, THEIR_DEFY, &[TargetRef::Card(BRUTE)]);
        assert!(!cannot_be_countered(&ctx, &mine));
        assert_eq!(extra_penalty(&ctx, &mine, BRUTE), 0);
        assert_eq!(amplified(&ctx, &mine, BRUTE, -1), -1);
        empower_her(&mut ctx);
        assert!(cannot_be_countered(&ctx, &mine));
        assert!(
            !cannot_be_countered(&ctx, &theirs),
            "their Mel is not empowered"
        );
        assert_eq!(extra_penalty(&ctx, &mine, BRUTE), -1);
        assert_eq!(
            extra_penalty(&ctx, &mine, fixtures::VI),
            0,
            "a unit the spell did not choose"
        );
        assert_eq!(extra_penalty(&ctx, &theirs, BRUTE), 0);
        assert_eq!(amplified(&ctx, &mine, BRUTE, -1), -2);
        assert_eq!(amplified(&ctx, &mine, BRUTE, 2), 2, "a plus is not a minus");
        assert_eq!(amplified(&ctx, &mine, BRUTE, 0), 0);
        let ability = ChainItem::new(
            8,
            ItemKind::Ability {
                source: MEL,
                index: EMPOWER_ABILITY,
            },
            0,
            Origin::Board,
        );
        assert!(cannot_be_countered(&ctx, &ability), "abilities too");
        assert!(ctx.empower(THEIR_MEL));
        assert!(cannot_be_countered(&ctx, &theirs));
        assert_eq!(extra_penalty(&ctx, &theirs, BRUTE), -1);
        ctx.recall(MEL, true);
        assert!(
            cannot_be_countered(&ctx, &mine),
            "her base is still in play"
        );
        assert!(ctx.bounce(MEL));
        assert!(!cannot_be_countered(&ctx, &mine));
        assert_eq!(extra_penalty(&ctx, &mine, BRUTE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn while_she_is_empowered_the_opponents_defy_cannot_counter_her_controllers_spark() {
        let mut fixture = salon(fixtures::BASE);
        for rune in [46, 47, 48] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        fixture.resolve();
        let mut ctx = fixture.ctx();
        empower_her(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let spark = ctx.blob.chain[0].id;
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_DEFY).unwrap();
        fixtures::choose(
            &mut ctx,
            1,
            &format!("{{card {}}} on the chain", fixtures::HAND_SPELL),
        )
        .unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the Spark is still waiting");
        assert_eq!(ctx.blob.chain[0].id, spark);
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("can't be countered")));
    }

    #[test]
    #[ignore = "engine gap · game-rule statics: Ctx::might applies the delta it is handed; the engine owes prelude::might_this_turn (or Ctx::might on a negative delta) passing the amount through mel_newly_awakened::amplified for the item's controller, so Stupefy on a chosen unit reads -2 while she is empowered"]
    fn while_she_is_empowered_a_chosen_minus_might_from_her_controller_is_one_more() {
        let mut fixture = salon(fixtures::BASE);
        fixture
            .table
            .cards
            .push(fixtures::rune(46, 0, "Calm", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        empower_her(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, STUPEFY).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.current_might(BRUTE),
            2,
            "-1 from Stupefy and -1 more from Mel"
        );
    }
}
