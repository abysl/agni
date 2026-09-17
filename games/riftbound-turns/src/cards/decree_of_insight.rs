use super::prelude::{a_card, card_target, might_this_turn, play};
use super::{Card, Domain, Filter, Flow, Keyword, Static, KIND_SPELL};

pub const ENEMY_BODY_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::Domain(Domain::Body)]);

pub const WEAKEN: i16 = -5;

pub static CARD: Card = Card {
    name: "Decree of Insight",
    keywords: &[Keyword::Reaction],
    abilities: &[play(
        &[a_card(ENEMY_BODY_UNIT, "an enemy Body unit")],
        |ctx, item, _| {
            if let Some(unit) = card_target(ctx, item, 0) {
                might_this_turn(ctx, item, unit, WEAKEN, None);
            }
            Flow::Done
        },
    )],
    statics: &[Static::IgnoresDeflect],
    replacement: None,
    additional: None,
    names: None,
    kind: Some(KIND_SPELL),
    adds: None,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ctx::{Ctx, EntryMove, COUNTER_MIGHT};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{cost, legal, phases, play, prompts, settle, targets};
    use crate::state::{Expiry, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Answer, Pick};
    use agni_plugin_sdk::table::Target;

    const DECREE: u32 = 90;
    const THEIR_BODY_UNIT: u32 = 91;

    fn fixture() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut decree = fixtures::spell(DECREE, fixtures::HAND, 0, "Decree of Insight", 1, 0);
        decree.domain = vec!["Mind".into()];
        fixture.table.cards.push(decree);
        let mut brute = fixtures::unit(THEIR_BODY_UNIT, fixtures::BASE, 1, "Brute", 7);
        brute.domain = vec!["Body".into()];
        fixture.table.cards.push(brute);
        fixture.table.card_mut(fixtures::VI).unwrap().domain = vec!["Body".into()];
        fixture.resolve();
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

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        play::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        let answered = prompts::answer(ctx, seat, Pick { prompt, option })?;
        if let Some(answered) = answered {
            match (answered.why, answered.answer) {
                (PromptWhy::Target { item, .. }, Answer::Cancel) => play::cancel(ctx, item),
                (PromptWhy::Target { item, spec }, _) => {
                    play::choose_targets(ctx, item, spec, &answered.prompt.picked)?
                }
                _ => {}
            }
        }
        settle(ctx)?;
        fixtures::settle_rune_payments(ctx, seat)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), COUNTER_MIGHT)
            .unwrap_or(0)
    }

    #[test]
    fn the_script_is_registered_as_a_reaction_that_ignores_deflect() {
        let card = crate::cards::script_of("Decree of Insight").unwrap();
        assert!(std::ptr::eq(card, &CARD));
        assert!(card.has_keyword(Keyword::Reaction));
        assert!(!card.has_keyword(Keyword::Action));
        assert!(card.has_static(Static::IgnoresDeflect));
        assert_eq!(card.abilities.len(), 1);
        assert_eq!(card.abilities[0].targets.len(), 1);
        assert_eq!(card.abilities[0].targets[0].min, 1);
        assert_eq!(card.abilities[0].targets[0].max, 1);
    }

    #[test]
    fn decree_of_insight_offers_only_enemy_body_units_and_gives_the_pick_minus_five_this_turn() {
        let mut fixture = fixture();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(THEIR_BODY_UNIT), 7);
        play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            [format!("{{card {THEIR_BODY_UNIT}}}"), "cancel".to_string()],
            "the friendly Body unit and the Fury enemies are never offered"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            format!("{{card {DECREE}}}: choose an enemy Body unit (0 of 1)")
        );
        pick(&mut ctx, 0, 0).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(THEIR_BODY_UNIT)]
        );
        assert_eq!(
            ctx.current_might(THEIR_BODY_UNIT),
            7,
            "nothing happens until the spell resolves"
        );
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, THEIR_BODY_UNIT), -5);
        assert_eq!(ctx.current_might(THEIR_BODY_UNIT), 2);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        let row = ctx.state_of(THEIR_BODY_UNIT).unwrap();
        assert_eq!(row.might.len(), 1);
        assert_eq!(row.might[0].delta, -5);
        assert_eq!(row.might[0].until, Expiry::EndOfTurn(ctx.turn()));
        assert_eq!(
            ctx.card(DECREE).unwrap().zone,
            Some(fixtures::TRASH),
            "the spell is trashed once it resolves"
        );
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(THEIR_BODY_UNIT), 2);
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(ctx.current_might(THEIR_BODY_UNIT), 7);
        assert_eq!(might_counter(&ctx, THEIR_BODY_UNIT), 0);
        assert!(ctx
            .state_of(THEIR_BODY_UNIT)
            .is_none_or(|row| row.might.is_empty()));
    }

    #[test]
    fn the_whole_penalty_is_recorded_even_when_it_takes_the_unit_below_zero() {
        let mut fixture = fixture();
        fixture.table.card_mut(THEIR_BODY_UNIT).unwrap().might = Some(2);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, DECREE).unwrap();
        pick(&mut ctx, 0, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(THEIR_BODY_UNIT), 0);
        assert_eq!(might_counter(&ctx, THEIR_BODY_UNIT), -5);
        assert_eq!(ctx.state_of(THEIR_BODY_UNIT).unwrap().might[0].delta, -5);
        let ctx_turn = ctx.turn();
        ctx.expire(Expiry::EndOfTurn(ctx_turn));
        assert_eq!(ctx.current_might(THEIR_BODY_UNIT), 2);
        assert_eq!(might_counter(&ctx, THEIR_BODY_UNIT), 0);
    }

    #[test]
    fn deflect_on_the_enemy_costs_nothing_extra_and_never_hides_it() {
        let mut fixture = fixture();
        fixture
            .blob
            .card_state_mut(THEIR_BODY_UNIT)
            .granted
            .push((Keyword::Deflect(2), Expiry::Permanent));
        for rune in [41, 42] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(THEIR_BODY_UNIT), 2);
        assert_eq!(ctx.ready_runes_of(0).len(), 1);
        play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(
            labels(&ctx),
            [format!("{{card {THEIR_BODY_UNIT}}}"), "cancel".to_string()],
            "one ready rune pays the spell; the deflect is ignored"
        );
        let pending = ctx.blob.pending(1).unwrap().item.clone();
        assert!(cost::ignores_deflect(&ctx, &pending));
        let with = cost::of_item(&ctx, &pending, Some(TargetRef::Card(THEIR_BODY_UNIT)));
        assert_eq!(with.energy, 1);
        assert!(with.power.is_empty());
        pick(&mut ctx, 0, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(ctx.ready_runes_of(0).len(), 0);
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(THEIR_BODY_UNIT), 2);
    }

    #[test]
    fn it_reacts_on_the_other_seats_chain_but_not_in_their_neutral_open() {
        let mut fixture = fixture();
        fixture.table.card_mut(DECREE).unwrap().seat = 1;
        fixture.table.card_mut(DECREE).unwrap().owner = 1;
        let mut brute = fixtures::unit(92, fixtures::BASE, 0, "Bruiser", 4);
        brute.domain = vec!["Body".into()];
        fixture.table.cards.push(brute);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, DECREE)),
            Err(Refusal::NotYourTurn),
            "a reaction still needs an open chain or the seat's own turn"
        );
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, DECREE)),
            Err(Refusal::Illegal(Reason::ChainClosed)),
            "seat 0 holds priority first"
        );
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        play_from_hand(&mut ctx, 1, DECREE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                "{card 92}".to_string(),
                "cancel".to_string()
            ],
            "from seat 1 the enemy Body units are seat 0's"
        );
        pick(&mut ctx, 1, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        crate::engine::priority::pass(&mut ctx, 1).unwrap();
        crate::engine::priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the reaction resolved first");
        assert_eq!(ctx.current_might(92), 0);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }

    #[test]
    fn a_friendly_or_non_body_unit_is_refused_and_no_candidate_offers_no_card() {
        let mut fixture = fixture();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, DECREE).unwrap();
        let pending = ctx.blob.pending(1).unwrap().item.clone();
        let spec = &CARD.abilities[0].targets[0];
        assert_eq!(
            targets::candidates(&ctx, &pending, spec),
            [TargetRef::Card(THEIR_BODY_UNIT)]
        );
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, fixtures::SPRITE] {
            assert_eq!(
                play::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy Body unit"
            );
        }
        assert_eq!(
            play::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert!(ctx.blob.pending(1).is_some(), "the play is still pending");
        assert!(ctx.blob.chain.is_empty());
        pick(&mut ctx, 0, 1).unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.pending(1).is_none());
        assert_eq!(
            ctx.card(DECREE).unwrap().zone,
            Some(fixtures::HAND),
            "cancel returns the spell to hand"
        );
        assert_eq!(ctx.ready_runes_of(0).len(), 3, "nothing was paid");
        let table = ctx.table.clone();
        let mut blob = ctx.blob.clone();
        drop(ctx);
        blob.prompt = None;
        fixture.commit(table);
        fixture.blob = blob;
        fixture.table.card_mut(THEIR_BODY_UNIT).unwrap().domain = vec!["Fury".into()];
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        let offered = labels(&ctx);
        assert!(
            prompts::offered(&ctx).iter().all(|opt| opt.card.is_none()),
            "with no enemy Body unit no card is offered: {offered:?}"
        );
        let cancel = offered.iter().position(|label| label == "cancel").unwrap();
        pick(&mut ctx, 0, cancel as u16).unwrap();
        assert_eq!(ctx.card(DECREE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.ready_runes_of(0).len(), 3);
    }
}
