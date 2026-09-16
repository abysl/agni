use crate::cards::{Filter, Keyword, TargetSpec, KIND_SPELL, KIND_UNIT};
use crate::engine::cost::{Cost, Need};
use crate::engine::ctx::{Ctx, Location};
use crate::engine::legal::Reason;
use crate::engine::{legal, pay, play, targets};
use crate::state::{ChainItem, ItemKind, Origin, Phase, FLAG_FROM_FACEDOWN};
use crate::Refusal;
use agni_plugin_sdk::decide::Effect;

pub const ANNOTATION_HIDDEN: &str = "hidden";

fn illegal<T>(reason: Reason) -> Result<T, Refusal> {
    Err(Refusal::Illegal(reason))
}

pub fn cost() -> Cost {
    Cost {
        energy: 0,
        power: vec![Need::Rainbow],
        ..Cost::default()
    }
}

pub fn zone_of(ctx: &Ctx, card: u32) -> Option<u16> {
    ctx.state_of(card).and_then(|row| row.hidden_at)
}

pub fn is_facedown(ctx: &Ctx, card: u32) -> bool {
    ctx.is_facedown(card)
}

pub fn facedown_at(ctx: &Ctx, zone: u16) -> Vec<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.hidden_at == Some(zone))
        .map(|row| row.id)
        .collect()
}

pub fn of_seat(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.blob
        .cards
        .iter()
        .filter(|row| row.hidden_at.is_some())
        .map(|row| row.id)
        .filter(|card| ctx.controller(*card) == seat)
        .collect()
}

pub fn playable(ctx: &Ctx, card: u32) -> bool {
    ctx.state_of(card)
        .is_some_and(|row| row.hidden_at.is_some() && row.hidden_since < ctx.turn())
}

pub fn reacts(ctx: &Ctx, card: u32) -> bool {
    ctx.has_keyword(card, Keyword::Reaction)
        || (playable(ctx, card) && ctx.has_keyword(card, Keyword::Hidden))
}

pub fn legal(ctx: &Ctx, seat: u8, card: u32, zone: u16) -> Result<(), Refusal> {
    if !ctx.blob.is_turn_player(seat) {
        return Err(Refusal::NotYourTurn);
    }
    if ctx.blob.phase() != Some(Phase::Action) {
        return illegal(Reason::NotActionPhase);
    }
    if ctx.blob.showdown.is_some() {
        return Err(Refusal::ShowdownOpen);
    }
    if ctx.blob.priority.is_some() || !ctx.blob.chain.is_empty() {
        return illegal(Reason::ChainClosed);
    }
    if !ctx.zones.is_battlefield(zone) {
        return illegal(Reason::UnknownDestination);
    }
    if !ctx.holds(seat, zone) {
        return illegal(Reason::HideNeedsHold);
    }
    if !facedown_at(ctx, zone).is_empty() {
        return illegal(Reason::OneFacedown);
    }
    if ctx.is_pending_play(card) {
        return illegal(Reason::Unplayable);
    }
    pay::plan(ctx, seat, &cost())?;
    Ok(())
}

pub fn hide(ctx: &mut Ctx, seat: u8, card: u32, zone: u16) -> Result<(), Refusal> {
    legal(ctx, seat, card, zone)?;
    let plan = pay::plan(ctx, seat, &cost())?;
    pay::pay(ctx, seat, &plan);
    let turn = ctx.turn();
    {
        let row = ctx.state_mut(card);
        row.hidden_at = Some(zone);
        row.hidden_since = turn;
    }
    ctx.emit(Effect::Annotate {
        card,
        key: ANNOTATION_HIDDEN.into(),
        value: Some(zone.to_le_bytes().to_vec()),
    });
    ctx.narrate(format!("{{seat {seat}}} hides a card at {{zone {zone}}}"));
    for viewer in lookers_at(ctx, seat) {
        ctx.peek(card, viewer);
    }
    Ok(())
}

pub fn lookers_at(ctx: &Ctx, seat: u8) -> Vec<u8> {
    (0..ctx.players())
        .filter(|viewer| *viewer != seat)
        .filter(|viewer| ctx.blob.seat(*viewer).looks_facedown_of & (1 << seat) != 0)
        .collect()
}

pub fn game_over(ctx: &mut Ctx) {
    let facedown: Vec<u32> = ctx
        .blob
        .cards
        .iter()
        .filter(|row| row.hidden_at.is_some())
        .map(|row| row.id)
        .collect();
    for card in facedown {
        let owner = ctx.controller(card);
        drop_facedown(ctx, card);
        ctx.reveal(card);
        ctx.narrate(format!(
            "{{seat {owner}}} reveals {{card {card}}} · the game is over"
        ));
    }
}

pub fn play_legal(ctx: &Ctx, seat: u8, card: u32) -> Result<u16, Refusal> {
    let Some(zone) = zone_of(ctx, card) else {
        return illegal(Reason::NotFacedown);
    };
    if ctx.controller(card) != seat {
        return illegal(Reason::NotYourCard);
    }
    if !playable(ctx, card) {
        return illegal(Reason::HiddenThisTurn);
    }
    if !ctx.has_keyword(card, Keyword::Hidden) {
        return illegal(Reason::NoHiddenKeyword);
    }
    legal::timing_at(ctx, seat, card, None)?;
    legal::unlocked(ctx, seat, ctx.kind_of(card).unwrap_or(KIND_UNIT))?;
    if ctx.is_unit(card) && !ctx.units_played_here(zone) {
        return illegal(Reason::NoUnitsPlayedHere);
    }
    if !targets_available(ctx, seat, card, zone) {
        return illegal(Reason::NoLegalTargets);
    }
    Ok(zone)
}

pub fn targets_available(ctx: &Ctx, seat: u8, card: u32, zone: u16) -> bool {
    if ctx.kind_of(card) != Some(KIND_SPELL) {
        return true;
    }
    let item = ChainItem::new(0, ItemKind::Spell { card }, seat, Origin::Facedown { zone });
    targets::specs_of(ctx, &item)
        .iter()
        .all(|spec| targets::fillable(ctx, &item, spec))
}

pub fn play_from_facedown(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
    let zone = play_legal(ctx, seat, card)?;
    let location = ctx.is_unit(card).then_some(Location::Battlefield(zone));
    ctx.narrate(format!(
        "{{seat {seat}}} plays {{card {card}}} from hidden at {{zone {zone}}}"
    ));
    play::begin(ctx, seat, card, Origin::Facedown { zone }, location)
}

pub fn facedown_zone(item: &ChainItem) -> Option<u16> {
    match item.origin {
        Origin::Facedown { zone } => Some(zone),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtBattlefield {
    Holds,
    Fails,
    Open,
}

impl AtBattlefield {
    fn not(self) -> Self {
        match self {
            AtBattlefield::Holds => AtBattlefield::Fails,
            AtBattlefield::Fails => AtBattlefield::Holds,
            AtBattlefield::Open => AtBattlefield::Open,
        }
    }
}

fn at_hiding_battlefield(specs: &[TargetSpec], position: usize, filter: &Filter) -> AtBattlefield {
    let earlier_restricted =
        |index: u8| usize::from(index) >= position || !lifts_at(specs, usize::from(index));
    match filter {
        Filter::Here | Filter::AtBattlefield | Filter::HiddenBattlefield => AtBattlefield::Holds,
        Filter::InBase => AtBattlefield::Fails,
        Filter::SameLocationAs(index) => {
            if earlier_restricted(*index) {
                AtBattlefield::Holds
            } else {
                AtBattlefield::Open
            }
        }
        Filter::DifferentLocationFrom(index) => {
            if earlier_restricted(*index) {
                AtBattlefield::Fails
            } else {
                AtBattlefield::Open
            }
        }
        Filter::Not(inner) => at_hiding_battlefield(specs, position, inner).not(),
        Filter::And([]) => AtBattlefield::Open,
        Filter::And(all) => {
            let parts: Vec<AtBattlefield> = all
                .iter()
                .map(|held| at_hiding_battlefield(specs, position, held))
                .collect();
            if parts.contains(&AtBattlefield::Fails) {
                AtBattlefield::Fails
            } else if parts.iter().all(|part| *part == AtBattlefield::Holds) {
                AtBattlefield::Holds
            } else {
                AtBattlefield::Open
            }
        }
        Filter::Or([]) => AtBattlefield::Open,
        Filter::Or(all) => {
            let parts: Vec<AtBattlefield> = all
                .iter()
                .map(|held| at_hiding_battlefield(specs, position, held))
                .collect();
            if parts.contains(&AtBattlefield::Holds) {
                AtBattlefield::Holds
            } else if parts.iter().all(|part| *part == AtBattlefield::Fails) {
                AtBattlefield::Fails
            } else {
                AtBattlefield::Open
            }
        }
        _ => AtBattlefield::Open,
    }
}

pub fn lifts_at(specs: &[TargetSpec], position: usize) -> bool {
    specs.get(position).is_some_and(|spec| {
        at_hiding_battlefield(specs, position, &spec.filter) == AtBattlefield::Fails
    })
}

pub fn lifts(specs: &[TargetSpec], spec: &TargetSpec) -> bool {
    match specs.iter().position(|held| std::ptr::eq(held, spec)) {
        Some(position) => lifts_at(specs, position),
        None => lifted_by(&spec.filter),
    }
}

pub fn lifted_by(filter: &Filter) -> bool {
    at_hiding_battlefield(&[], 0, filter) == AtBattlefield::Fails
}

pub fn drop_facedown(ctx: &mut Ctx, card: u32) -> Option<u16> {
    let zone = zone_of(ctx, card)?;
    {
        let row = ctx.state_mut(card);
        row.hidden_at = None;
        row.hidden_since = 0;
    }
    if ctx.card(card).is_some() {
        ctx.emit(Effect::Annotate {
            card,
            key: ANNOTATION_HIDDEN.into(),
            value: None,
        });
    }
    Some(zone)
}

pub fn finalized(ctx: &mut Ctx, item: &ChainItem) {
    if facedown_zone(item).is_none() {
        return;
    }
    let Some(card) = item.kind.card() else {
        return;
    };
    drop_facedown(ctx, card);
    ctx.set_flag(card, FLAG_FROM_FACEDOWN, true);
}

pub fn shown(ctx: &mut Ctx, seat: u8, card: u32) -> bool {
    if !is_facedown(ctx, card) || ctx.controller(card) != seat {
        return false;
    }
    ctx.narrate(format!("{{seat {seat}}} reveals {{card {card}}}"));
    true
}

pub fn is_private(ctx: &Ctx, zone: u16) -> bool {
    ctx.zones.hand == Some(zone) || ctx.zones.is_deck(zone)
}

pub fn reveal_before(ctx: &mut Ctx, card: u32, to: u16) {
    if !is_facedown(ctx, card) || !is_private(ctx, to) {
        return;
    }
    let owner = ctx.owner(card);
    ctx.narrate(format!(
        "{{seat {owner}}} reveals {{card {card}}} before it leaves the board"
    ));
    drop_facedown(ctx, card);
}

pub fn lost_control(ctx: &mut Ctx) {
    let facedown: Vec<u32> = ctx
        .blob
        .cards
        .iter()
        .filter(|row| row.hidden_at.is_some())
        .map(|row| row.id)
        .collect();
    for card in facedown {
        let Some(zone) = zone_of(ctx, card) else {
            continue;
        };
        if ctx.card(card).is_none() || ctx.is_pending_play(card) {
            continue;
        }
        let owner = ctx.controller(card);
        if ctx.blob.holder(zone) == Some(owner) || ctx.blob.contester(zone).is_some() {
            continue;
        }
        drop_facedown(ctx, card);
        ctx.trash(card);
        ctx.narrate(format!(
            "{{card {card}}} is revealed in the trash · {{seat {owner}}} no longer holds {{zone {zone}}}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude;
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, settle, targets};
    use crate::state::{ChainItem, ItemKind, Priority, PromptWhy, Showdown, TargetRef};
    use agni_plugin_sdk::decide::{Action, Effect, BOTTOM, TOP};

    const HIDDEN_ANNOTATION: [u8; 2] = [fixtures::BF1 as u8, 0];

    fn held() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture
    }

    fn hidden_move(ctx: &Ctx, card: u32, to: u16) -> EntryMove {
        let before = ctx.card(card);
        EntryMove {
            card,
            from: before.and_then(|held| held.zone),
            from_seat: before.map(|held| held.seat).unwrap_or(0),
            to: Some(to),
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn turn_face_up(fixture: &mut Fixture, card: u32, name: &str, kind: &str, energy: u8) {
        {
            let face = fixture.table.card_mut(card).unwrap();
            face.name = name.into();
            face.kind = Some(kind.into());
            face.energy = Some(energy);
            face.domain = vec!["Mind".into()];
        }
        fixture.resolve();
    }

    fn hide_now(fixture: &mut Fixture, seat: u8, card: u32, zone: u16) -> Result<(), Refusal> {
        let action = fixtures::move_action(card, zone, 0);
        let mut ctx = fixture.ctx_for(seat, &action);
        let entry = ctx.entry.unwrap();
        let intent = legal::classify(&ctx, seat, &entry)?;
        assert_eq!(intent, legal::Intent::Hide { card, zone });
        let done = crate::engine::act(&mut ctx, seat, intent);
        let table = ctx.table.clone();
        drop(ctx);
        fixture.commit(table);
        done
    }

    #[test]
    fn a_hide_needs_a_held_battlefield_with_no_facedown_card_there_yet() {
        let mut fixture = Fixture::enforced();
        assert_eq!(
            hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "106.4.c · the facedown zone belongs to the battlefield's holder"
        );
        let mut theirs = Fixture::enforced();
        assert_eq!(
            hide_now(&mut theirs, 0, fixtures::HAND_HIDDEN, fixtures::BF2),
            Err(Refusal::Illegal(Reason::HideNeedsHold)),
            "the other seat holds that battlefield"
        );
        let mut taken = held();
        taken
            .blob
            .card_state_mut(fixtures::THEIR_HAND_CARD)
            .hidden_at = Some(fixtures::BF1);
        assert_eq!(
            hide_now(&mut taken, 0, fixtures::HAND_HIDDEN, fixtures::BF1),
            Err(Refusal::Illegal(Reason::OneFacedown)),
            "106.4.b · one card per facedown zone"
        );
    }

    #[test]
    fn a_hand_card_whose_face_is_public_is_hidden_by_a_hidden_move_not_played() {
        let mut fixture = held();
        fixture.table.revealed.push(fixtures::HAND_SPELL);
        fixture.table.revealed.sort_unstable();
        let action = Action::Move {
            card: fixtures::HAND_SPELL,
            to: Some(fixtures::BF1),
            seat: 0,
            index: TOP,
            hidden: true,
        };
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.unwrap();
        assert!(entry.hidden);
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Ok(legal::Intent::Hide {
                card: fixtures::HAND_SPELL,
                zone: fixtures::BF1
            }),
            "a public face does not turn the hide into a play"
        );
        assert!(
            ctx.card(fixtures::HAND_SPELL).unwrap().is_hidden(),
            "the snapshot lays it face down"
        );
        assert!(!ctx.table.is_revealed(fixtures::HAND_SPELL));
        crate::engine::act(
            &mut ctx,
            0,
            legal::Intent::Hide {
                card: fixtures::HAND_SPELL,
                zone: fixtures::BF1,
            },
        )
        .unwrap();
        let recycled = ctx
            .effects
            .iter()
            .filter(|effect| {
                matches!(effect, Effect::Move { zone, index, .. }
                    if *zone == fixtures::RUNE_DECK && *index == BOTTOM)
            })
            .count();
        assert_eq!(recycled, 1, "charged as any hide");
        assert!(ctx.is_facedown(fixtures::HAND_SPELL));
        assert!(
            !ctx.effects.iter().any(|effect| matches!(
                effect,
                Effect::Move { zone, .. } if Some(*zone) == ctx.zones.chain
            )),
            "nothing goes to the chain: {:?}",
            ctx.effects
        );
        let plain = fixtures::move_action(fixtures::HAND_SPELL, fixtures::BF1, 0);
        let mut shown = held();
        shown.table.revealed.push(fixtures::HAND_SPELL);
        shown.table.revealed.sort_unstable();
        let ctx = shown.ctx_for(0, &plain);
        let entry = ctx.entry.unwrap();
        assert!(!entry.hidden);
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::Illegal(Reason::SpellsToChain)),
            "a plain move of the same card is still read as a play"
        );
    }

    #[test]
    fn a_hide_is_own_turn_neutral_open_and_costs_one_rune() {
        let mut theirs = held();
        theirs.blob.core_mut().unwrap().player = 1;
        assert_eq!(
            hide_now(&mut theirs, 0, fixtures::HAND_HIDDEN, fixtures::BF1),
            Err(Refusal::NotYourTurn)
        );
        let mut closed = held();
        closed.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        assert_eq!(
            hide_now(&mut closed, 0, fixtures::HAND_HIDDEN, fixtures::BF1),
            Err(Refusal::Illegal(Reason::ChainClosed))
        );
        let mut showdown = held();
        showdown.blob.showdown = Some(Showdown::open(fixtures::BF1, 0, 1));
        assert_eq!(
            hide_now(&mut showdown, 0, fixtures::HAND_HIDDEN, fixtures::BF1),
            Err(Refusal::ShowdownOpen)
        );
        let mut broke = held();
        broke
            .table
            .cards
            .retain(|card| card.owner != 0 || !card.name.ends_with("Rune"));
        broke.resolve();
        assert!(matches!(
            hide_now(&mut broke, 0, fixtures::HAND_HIDDEN, fixtures::BF1),
            Err(Refusal::NotEnoughRunes { .. })
        ));
    }

    #[test]
    fn hiding_pays_one_rune_marks_the_card_and_answers_the_one_facedown_rule() {
        let mut fixture = held();
        let action = fixtures::move_action(fixtures::HAND_HIDDEN, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let entry = ctx.entry.unwrap();
        let intent = legal::classify(&ctx, 0, &entry).unwrap();
        crate::engine::act(&mut ctx, 0, intent).unwrap();
        let recycled = ctx
            .effects
            .iter()
            .filter(|effect| {
                matches!(effect, Effect::Move { zone, index, .. }
                    if *zone == fixtures::RUNE_DECK && *index == BOTTOM)
            })
            .count();
        assert_eq!(recycled, 1, "[A] recycles exactly one rune");
        assert!(ctx.effects.contains(&Effect::Annotate {
            card: fixtures::HAND_HIDDEN,
            key: ANNOTATION_HIDDEN.into(),
            value: Some(HIDDEN_ANNOTATION.to_vec()),
        }));
        assert_eq!(zone_of(&ctx, fixtures::HAND_HIDDEN), Some(fixtures::BF1));
        assert_eq!(
            ctx.state_of(fixtures::HAND_HIDDEN).unwrap().hidden_since,
            ctx.turn()
        );
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{seat 0} hides a card at {zone 9}"
        );
        assert!(legal(&ctx, 0, fixtures::HAND_UNIT, fixtures::BF1).is_err());
        assert_eq!(facedown_at(&ctx, fixtures::BF1), [fixtures::HAND_HIDDEN]);
        assert_eq!(of_seat(&ctx, 0), [fixtures::HAND_HIDDEN]);
        assert!(of_seat(&ctx, 1).is_empty());
    }

    #[test]
    fn a_facedown_card_is_playable_from_the_next_turn_only_and_for_nothing() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        {
            let mut ctx = fixture.ctx();
            assert_eq!(
                play_legal(&ctx, 0, fixtures::HAND_HIDDEN),
                Err(Refusal::Illegal(Reason::HiddenThisTurn)),
                "737.1.b · the grant starts on the next turn"
            );
            assert!(!playable(&ctx, fixtures::HAND_HIDDEN));
            assert!(!reacts(&ctx, fixtures::HAND_HIDDEN));
            let entry = hidden_move(&ctx, fixtures::HAND_HIDDEN, fixtures::CHAIN);
            assert_eq!(
                legal::classify(&ctx, 0, &entry),
                Err(Refusal::Illegal(Reason::HiddenThisTurn))
            );
            assert_eq!(
                crate::engine::act(
                    &mut ctx,
                    0,
                    legal::Intent::PlayFromFacedown {
                        card: fixtures::HAND_HIDDEN
                    }
                ),
                Err(Refusal::Illegal(Reason::HiddenThisTurn))
            );
        }
        fixture.blob.core_mut().unwrap().turn += 1;
        {
            let ctx = fixture.ctx();
            assert!(playable(&ctx, fixtures::HAND_HIDDEN));
            assert!(
                !reacts(&ctx, fixtures::HAND_HIDDEN),
                "737.1 · a card with no Hidden was never granted the Reaction"
            );
            assert_eq!(
                play_legal(&ctx, 0, fixtures::HAND_HIDDEN),
                Err(Refusal::Illegal(Reason::NoHiddenKeyword)),
                "737.1 · a card hidden without Hidden is stuck there"
            );
        }
        turn_face_up(
            &mut fixture,
            fixtures::HAND_HIDDEN,
            "Consult the Past",
            "Spell",
            4,
        );
        let ctx = fixture.ctx();
        assert!(playable(&ctx, fixtures::HAND_HIDDEN));
        assert!(reacts(&ctx, fixtures::HAND_HIDDEN));
        assert_eq!(
            play_legal(&ctx, 0, fixtures::HAND_HIDDEN),
            Ok(fixtures::BF1)
        );
        assert_eq!(
            play_legal(&ctx, 1, fixtures::HAND_HIDDEN),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert_eq!(
            play_legal(&ctx, 0, fixtures::HAND_UNIT),
            Err(Refusal::Illegal(Reason::NotFacedown))
        );
    }

    #[test]
    fn losing_the_battlefield_trashes_the_facedown_card_where_the_trash_shows_it() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert_eq!(
            zone_of(&ctx, fixtures::HAND_HIDDEN),
            Some(fixtures::BF1),
            "the holder keeps it"
        );
        ctx.recall(fixtures::VI, false);
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.blob.holder(fixtures::BF1), None, "322.6 · left empty");
        assert_eq!(zone_of(&ctx, fixtures::HAND_HIDDEN), None);
        assert!(ctx.effects.contains(&Effect::Move {
            card: fixtures::HAND_HIDDEN,
            zone: fixtures::TRASH,
            seat: 0,
            index: TOP,
        }));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("is revealed in the trash")));
    }

    #[test]
    fn a_facedown_card_is_revealed_before_it_reaches_a_private_zone() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        let mut ctx = fixture.ctx();
        assert!(is_private(&ctx, fixtures::HAND));
        assert!(is_private(&ctx, fixtures::MAIN_DECK));
        assert!(!is_private(&ctx, fixtures::TRASH));
        assert!(!is_private(&ctx, fixtures::BF1));
        ctx.bounce(fixtures::HAND_HIDDEN);
        assert_eq!(zone_of(&ctx, fixtures::HAND_HIDDEN), None);
        assert!(ctx.effects.contains(&Effect::Annotate {
            card: fixtures::HAND_HIDDEN,
            key: ANNOTATION_HIDDEN.into(),
            value: None,
        }));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("before it leaves the board")));
    }

    #[test]
    fn a_voluntary_reveal_names_the_card_and_leaves_it_in_its_facedown_zone() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        let mut ctx = fixture.ctx();
        assert!(!shown(&mut ctx, 1, fixtures::HAND_HIDDEN));
        assert!(!shown(&mut ctx, 0, fixtures::HAND_UNIT));
        assert!(shown(&mut ctx, 0, fixtures::HAND_HIDDEN));
        assert_eq!(
            ctx.blob.log.last().unwrap(),
            "{seat 0} reveals {card 73}",
            "411.2 · the show is a narration, the card stays where it is"
        );
        assert_eq!(zone_of(&ctx, fixtures::HAND_HIDDEN), Some(fixtures::BF1));
    }

    #[test]
    fn a_facedown_card_reacts_into_the_other_seats_chain_for_nothing() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        {
            let core = fixture.blob.core_mut().unwrap();
            core.turn += 1;
            core.player = 1;
        }
        {
            let card = fixture.table.card_mut(fixtures::HAND_HIDDEN).unwrap();
            card.zone = Some(fixtures::BF1);
            card.seat = 0;
            card.name = "Consult the Past".into();
            card.kind = Some("Spell".into());
            card.energy = Some(4);
            card.domain = vec!["Mind".into()];
        }
        fixture.resolve();
        fixture.blob.chain.push(ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            1,
            Origin::Hand,
        ));
        fixture.blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        let action = fixtures::move_action(fixtures::HAND_HIDDEN, fixtures::CHAIN, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        let intent = legal::classify(&ctx, 0, &ctx.entry.unwrap()).unwrap();
        assert_eq!(
            intent,
            legal::Intent::PlayFromFacedown {
                card: fixtures::HAND_HIDDEN
            },
            "737.6 · a facedown card has Reaction while the chain is closed"
        );
        crate::engine::act(&mut ctx, 0, intent).unwrap();
        settle(&mut ctx).unwrap();
        let item = ctx
            .blob
            .chain
            .iter()
            .find(|held| held.kind.card() == Some(fixtures::HAND_HIDDEN))
            .expect("it joined the chain");
        assert_eq!(
            item.origin,
            Origin::Facedown {
                zone: fixtures::BF1
            }
        );
        let paid: Vec<&Effect> = ctx
            .effects
            .iter()
            .filter(|effect| !matches!(effect, Effect::Annotate { .. }))
            .collect();
        assert!(paid.is_empty(), "737.1.b · played ignoring its base cost");
        assert_eq!(zone_of(&ctx, fixtures::HAND_HIDDEN), None);
        assert!(ctx.has_flag(fixtures::HAND_HIDDEN, FLAG_FROM_FACEDOWN));
    }

    #[test]
    fn a_play_from_facedown_only_chooses_among_what_is_at_that_battlefield() {
        let mut fixture = held();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        let ctx = fixture.ctx();
        let spec = prelude::a_unit("a unit");
        let from_hand = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        let open = targets::candidates(&ctx, &from_hand, &spec);
        assert!(open.contains(&TargetRef::Card(fixtures::SPRITE)));
        let hidden = ChainItem::new(
            2,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Facedown {
                zone: fixtures::BF1,
            },
        );
        let restricted = targets::candidates(&ctx, &hidden, &spec);
        assert!(
            restricted.contains(&TargetRef::Card(fixtures::THEIR_UNIT))
                && restricted.contains(&TargetRef::Card(fixtures::VI)),
            "737.1.d.2 · everything at the hiding battlefield stays on offer"
        );
        assert!(
            !restricted.contains(&TargetRef::Card(fixtures::SPRITE)),
            "737.1.d.2 · a unit at another battlefield is not offered"
        );
        let zone_spec = prelude::a_battlefield("where");
        let zones = targets::candidates(&ctx, &hidden, &zone_spec);
        assert_eq!(zones, [TargetRef::Zone(fixtures::BF1)]);
    }

    #[test]
    fn a_revealed_facedown_card_is_still_no_unit_no_target_and_no_source() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        {
            let card = fixture.table.card_mut(fixtures::HAND_HIDDEN).unwrap();
            card.name = "Vi".into();
            card.kind = Some("Unit".into());
            card.might = Some(3);
            card.domain = vec!["Fury".into()];
        }
        fixture.resolve();
        let ctx = fixture.ctx();
        assert_eq!(
            ctx.units_at(Location::Battlefield(fixtures::BF1)),
            [fixtures::VI],
            "a card in the facedown zone is not a unit at the battlefield"
        );
        let spec = prelude::a_unit("a unit");
        let item = ChainItem::new(
            1,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        assert!(!targets::candidates(&ctx, &item, &spec)
            .contains(&TargetRef::Card(fixtures::HAND_HIDDEN)));
    }

    #[test]
    fn the_restriction_lift_reads_the_filter_tree_and_fails_closed() {
        assert!(lifted_by(&Filter::DifferentLocationFrom(0)));
        assert!(lifted_by(&Filter::And(&[
            Filter::Unit,
            Filter::DifferentLocationFrom(0)
        ])));
        assert!(lifted_by(&Filter::Not(&Filter::Here)));
        assert!(
            !lifted_by(&Filter::Not(&Filter::DifferentLocationFrom(0))),
            "737.1.d.2 · a negated lift is not a lift"
        );
        assert!(
            !lifted_by(&Filter::Not(&Filter::Not(&Filter::Here))),
            "Not(Not(Here)) means Here, which the battlefield satisfies"
        );
        assert!(
            !lifted_by(&Filter::Or(&[Filter::Not(&Filter::Here), Filter::Unit])),
            "an Or is impossible at the battlefield only when every branch is"
        );
        assert!(lifted_by(&Filter::Or(&[
            Filter::Not(&Filter::Here),
            Filter::DifferentLocationFrom(0)
        ])));
        assert!(!lifted_by(&Filter::Or(&[])));
        assert!(!lifted_by(&Filter::Here));
        assert!(!lifted_by(&prelude::FRIENDLY_UNIT));
        assert!(!lifted_by(&Filter::SameLocationAs(0)));
    }

    #[test]
    fn the_lift_is_the_rules_own_test_and_reads_earlier_specs() {
        assert!(
            lifted_by(&Filter::And(&[Filter::Unit, Filter::InBase])),
            "737.1.d.2 · a unit in a base can never stand at the battlefield"
        );
        assert!(!lifted_by(&Filter::Not(&Filter::InBase)));
        assert!(
            lifted_by(&Filter::Not(&Filter::AtBattlefield)),
            "explicitly off every battlefield"
        );
        assert!(
            lifted_by(&Filter::Not(&Filter::HiddenBattlefield)),
            "the hiding battlefield holds this very card, so it is excluded by name"
        );
        assert!(!lifted_by(&Filter::And(&[])));
        assert!(
            lifted_by(&Filter::Or(&[Filter::InBase, Filter::Not(&Filter::Here)])),
            "every branch is impossible there"
        );
        let elsewhere = prelude::a_card(
            Filter::And(&[Filter::Unit, Filter::Not(&Filter::Here)]),
            "a unit elsewhere",
        );
        let apart = prelude::a_card(
            Filter::And(&[Filter::Unit, Filter::DifferentLocationFrom(0)]),
            "another unit apart from it",
        );
        let with_it = prelude::a_card(
            Filter::And(&[Filter::Unit, Filter::SameLocationAs(0)]),
            "a unit with it",
        );
        let here = prelude::a_unit("a unit");
        let after_lifted = [elsewhere, apart];
        assert!(lifts(&after_lifted, &after_lifted[0]));
        assert!(
            !lifts(&after_lifted, &after_lifted[1]),
            "the first target may stand elsewhere, so a unit apart from it may stand here"
        );
        let after_restricted = [here, apart];
        assert!(!lifts(&after_restricted, &after_restricted[0]));
        assert!(
            lifts(&after_restricted, &after_restricted[1]),
            "the first target is held to the battlefield, so 'apart from it' never is"
        );
        let same = [elsewhere, with_it];
        assert!(!lifts(&same, &same[1]));
        let forward = [apart, here];
        assert!(
            lifts(&forward, &forward[0]),
            "a forward reference reads as restricted"
        );
        assert!(
            lifts(&[], &elsewhere),
            "a spec outside its list falls back to the single-spec reading"
        );
        assert!(!lifts_at(&forward, 2));
    }

    fn a_unit_hidden_at(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(zone);
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.blob.set_holder(fixtures::BF2, None);
        fixture.blob.set_holder(zone, Some(0));
        fixture.resolve();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, zone).unwrap();
        fixture.blob.core_mut().unwrap().turn += 1;
        {
            let face = fixture.table.card_mut(fixtures::HAND_HIDDEN).unwrap();
            face.name = "Pyke - Returned".into();
            face.kind = Some("Unit".into());
            face.energy = Some(3);
            face.might = Some(3);
            face.domain = vec!["Chaos".into()];
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn a_hidden_unit_is_refused_where_units_may_not_be_played_and_lands_there_otherwise() {
        let mut blocked = a_unit_hidden_at(fixtures::BF2);
        {
            let ctx = blocked.ctx();
            assert!(
                !ctx.units_played_here(fixtures::BF2),
                "Rockfall Path sits at battlefield 2"
            );
            assert_eq!(
                play_legal(&ctx, 0, fixtures::HAND_HIDDEN),
                Err(Refusal::Illegal(Reason::NoUnitsPlayedHere)),
                "737.1.d.1 · the forced placement is not a free ride into the base"
            );
        }
        let mut ctx = blocked.ctx();
        assert_eq!(
            play_from_facedown(&mut ctx, 0, fixtures::HAND_HIDDEN),
            Err(Refusal::Illegal(Reason::NoUnitsPlayedHere))
        );
        drop(ctx);

        let mut fine = a_unit_hidden_at(fixtures::BF1);
        let mut ctx = fine.ctx();
        assert!(ctx.units_played_here(fixtures::BF1));
        assert_eq!(
            play_legal(&ctx, 0, fixtures::HAND_HIDDEN),
            Ok(fixtures::BF1)
        );
        play_from_facedown(&mut ctx, 0, fixtures::HAND_HIDDEN).unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            ctx.location(fixtures::HAND_HIDDEN),
            Some(Location::Battlefield(fixtures::BF1)),
            "737.1.d.1 · it lands at the battlefield it was hidden at"
        );
    }

    #[test]
    fn only_the_controller_may_show_a_facedown_card() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        let request = agni_plugin_sdk::decide::Request {
            plugin_state: fixture.blob.encode(),
            players: fixture.table.players,
            seat: 1,
            action: agni_plugin_sdk::decide::Action::Reveal {
                card: fixtures::HAND_HIDDEN,
                face: agni_plugin_sdk::table::Face::named("Consult the Past"),
            },
            table: fixture.table.clone(),
        };
        assert_eq!(
            crate::decide(&request),
            Err(Refusal::Illegal(Reason::NotYourCard)),
            "408.3 · the other seat cannot strip the face off a facedown card"
        );
        let mine = agni_plugin_sdk::decide::Request { seat: 0, ..request };
        let verdict = crate::decide(&mine).expect("its controller may show it");
        assert!(verdict.accept);
        let after = crate::state::GameBlob::decode(&verdict.plugin_state.unwrap()).unwrap();
        assert!(
            after
                .log
                .iter()
                .any(|line| *line
                    == format!("{{seat 0}} reveals {{card {}}}", fixtures::HAND_HIDDEN)),
            "411.2 · the controller's own show is narrated: {:?}",
            after.log
        );
    }

    #[test]
    fn a_facedown_card_is_no_source_for_an_activated_ability() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        {
            let face = fixture.table.card_mut(fixtures::HAND_HIDDEN).unwrap();
            face.name = "Lillia - Bashful Bloom".into();
            face.kind = Some("Legend".into());
        }
        fixture.resolve();
        let ctx = fixture.ctx();
        assert!(ctx.script(fixtures::HAND_HIDDEN).is_some());
        assert!(
            crate::engine::activate::offers(&ctx, 0)
                .iter()
                .all(|offer| offer.source != fixtures::HAND_HIDDEN),
            "408.3 · a card in a facedown zone has no printed abilities"
        );
    }

    #[test]
    fn an_open_prompt_and_a_card_already_being_played_both_block_a_hide() {
        let mut fixture = held();
        let mut ctx = fixture.ctx();
        ctx.ask(0, 1, 1, false, PromptWhy::Assign);
        let entry = hidden_move(&ctx, fixtures::HAND_HIDDEN, fixtures::BF1);
        assert_eq!(legal::classify(&ctx, 0, &entry), Err(Refusal::PromptOpen));
        ctx.blob.prompt = None;
        ctx.blob.why = None;
        assert_eq!(legal(&ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1), Ok(()));
        play::begin(&mut ctx, 0, fixtures::HAND_UNIT, Origin::Hand, None).unwrap();
        assert!(ctx.is_pending_play(fixtures::HAND_UNIT));
        assert_eq!(
            legal(&ctx, 0, fixtures::HAND_UNIT, fixtures::BF1),
            Err(Refusal::Illegal(Reason::Unplayable))
        );
        assert_eq!(
            legal(&ctx, 0, fixtures::HAND_HIDDEN, fixtures::BASE),
            Err(Refusal::Illegal(Reason::UnknownDestination))
        );
    }

    #[test]
    fn the_game_ending_reveals_every_facedown_card_to_the_table() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        fixture.set_points(0, crate::rules::DEFAULT_VICTORY_SCORE - 1);
        fixture.blob.core_mut().unwrap().player = 1;
        let request = agni_plugin_sdk::decide::Request {
            plugin_state: fixture.blob.encode(),
            players: 2,
            seat: 1,
            action: agni_plugin_sdk::decide::Action::Game(crate::TurnEvent::EndTurn.encode()),
            table: fixture.table.clone(),
        };
        let verdict = crate::decide(&request).unwrap();
        assert!(verdict.effects.contains(&Effect::Reveal {
            card: fixtures::HAND_HIDDEN
        }));
        assert!(verdict.effects.contains(&Effect::Annotate {
            card: fixtures::HAND_HIDDEN,
            key: ANNOTATION_HIDDEN.into(),
            value: None,
        }));
        let blob = crate::GameBlob::decode(&verdict.plugin_state.unwrap()).unwrap();
        assert!(blob
            .cards
            .iter()
            .all(|row| row.id != fixtures::HAND_HIDDEN || row.hidden_at.is_none()));
        assert!(blob.log.contains(&format!(
            "{{seat 0}} reveals {{card {}}} · the game is over",
            fixtures::HAND_HIDDEN
        )));
        let table = fixture.table.clone();
        let mut ctx = Ctx::fresh(&table, &mut fixture.blob, &fixture.scripts, 0);
        let before = ctx.effects.len();
        game_over(&mut ctx);
        assert_eq!(ctx.effects.len() - before, 2);
        game_over(&mut ctx);
        assert_eq!(
            ctx.effects.len() - before,
            2,
            "a second pass finds nothing facedown"
        );
    }

    #[test]
    fn a_seat_allowed_to_look_is_handed_every_facedown_face_now_and_later() {
        let mut fixture = held();
        hide_now(&mut fixture, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        let mut ctx = fixture.ctx();
        ctx.look_at_facedown(1, 0);
        assert_eq!(ctx.blob.seat(1).looks_facedown_of, 1);
        assert_eq!(
            ctx.effects,
            [Effect::Peek {
                card: fixtures::HAND_HIDDEN,
                seat: 1
            }]
        );
        assert_eq!(lookers_at(&ctx, 0), [1]);
        assert!(lookers_at(&ctx, 1).is_empty());
        ctx.look_at_facedown(2, 0);
        ctx.look_at_facedown(1, 2);
        assert_eq!(ctx.effects.len(), 1, "seats beyond the table are ignored");
        ctx.blob.set_holder(fixtures::BF2, Some(0));
        ctx.table
            .card_mut(fixtures::HAND_UNIT)
            .unwrap()
            .name
            .clear();
        hide(&mut ctx, 0, fixtures::HAND_UNIT, fixtures::BF2).unwrap();
        assert_eq!(
            ctx.effects.last(),
            Some(&Effect::Peek {
                card: fixtures::HAND_UNIT,
                seat: 1
            }),
            "a card hidden later this turn is shown to the looker as well"
        );
        let mut quiet = held();
        let mut ctx = quiet.ctx();
        hide(&mut ctx, 0, fixtures::HAND_HIDDEN, fixtures::BF1).unwrap();
        assert!(
            !ctx.effects
                .iter()
                .any(|effect| matches!(effect, Effect::Peek { .. })),
            "nobody may look, nobody is shown"
        );
    }

    #[test]
    fn a_hand_reveal_and_a_deck_peek_are_information_effects() {
        let mut fixture = held();
        let mut ctx = fixture.ctx();
        let hand = ctx.reveal_hand(0);
        assert_eq!(
            hand,
            [
                fixtures::HAND_UNIT,
                fixtures::HAND_SPELL,
                fixtures::HAND_GEAR,
                fixtures::HAND_HIDDEN
            ]
        );
        assert_eq!(
            ctx.effects,
            hand.iter()
                .map(|card| Effect::Reveal { card: *card })
                .collect::<Vec<Effect>>()
        );
        assert_eq!(ctx.blob.log.last().unwrap(), "{seat 0} reveals their hand");
        ctx.table.revealed = vec![fixtures::HAND_UNIT];
        ctx.reveal(fixtures::HAND_UNIT);
        ctx.reveal(999);
        assert_eq!(
            ctx.effects.len(),
            4,
            "a public or unknown card owes no reveal"
        );
        assert_eq!(ctx.peek_top(1), Some(25));
        assert_eq!(
            ctx.effects.last(),
            Some(&Effect::Peek { card: 25, seat: 1 })
        );
        ctx.peek(25, 7);
        ctx.peek(fixtures::HAND_UNIT, 1);
        assert_eq!(ctx.effects.len(), 5);
        assert!(ctx.fault.is_none());
    }
}
