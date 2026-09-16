use crate::engine::cleanup::{self, Established};
use crate::engine::ctx::Ctx;
use crate::engine::{combat, expiry, priority};
use crate::state::{GameBlob, Phase, PromptWhy, Showdown, Staged};
use crate::Refusal;
use agni_plugin_sdk::turns::Window;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    NeutralOpen,
    NeutralClosed,
    ShowdownOpen,
    ShowdownClosed,
}

pub fn state(blob: &GameBlob) -> State {
    let closed = blob.priority.is_some() || !blob.chain.is_empty();
    match (blob.showdown.is_some(), closed) {
        (false, false) => State::NeutralOpen,
        (false, true) => State::NeutralClosed,
        (true, false) => State::ShowdownOpen,
        (true, true) => State::ShowdownClosed,
    }
}

pub fn stage(ctx: &mut Ctx) {
    let contested: Vec<(u16, u8)> = ctx.blob.contested_zones().collect();
    let mut staged = Vec::new();
    for (zone, contester) in contested {
        let seats = ctx.seats_with_units(zone);
        if !seats.contains(&contester) {
            ctx.blob.set_contested(zone, None);
            continue;
        }
        if ctx
            .blob
            .showdown
            .as_ref()
            .is_some_and(|showdown| showdown.zone == zone)
        {
            continue;
        }
        staged.push(Staged {
            zone,
            combat: seats.len() >= 2,
            contester,
        });
    }
    staged.sort_by_key(|staged| staged.zone);
    ctx.blob.staged = staged;
}

pub fn choices(blob: &GameBlob) -> Vec<usize> {
    let showdowns: Vec<usize> = blob
        .staged
        .iter()
        .enumerate()
        .filter(|(_, staged)| !staged.combat)
        .map(|(index, _)| index)
        .collect();
    if showdowns.is_empty() {
        (0..blob.staged.len()).collect()
    } else {
        showdowns
    }
}

pub fn owed(ctx: &Ctx) -> bool {
    !ctx.blob.queue.is_empty() || !ctx.deaths.is_empty() || ctx.collected < ctx.events.len()
}

pub fn open_next(ctx: &mut Ctx) {
    if state(ctx.blob) != State::NeutralOpen
        || ctx.blob.phase() != Some(Phase::Action)
        || ctx.blob.prompt.is_some()
        || owed(ctx)
    {
        return;
    }
    let choices = choices(ctx.blob);
    match choices.as_slice() {
        [] => {}
        [only] => open(ctx, *only),
        _ => {
            let turn_player = ctx.turn_player();
            ctx.ask(turn_player, 1, 1, false, PromptWhy::PickStaged);
        }
    }
}

pub fn pick(ctx: &mut Ctx, zone: u16) -> Result<(), Refusal> {
    let index = choices(ctx.blob)
        .into_iter()
        .find(|index| ctx.blob.staged[*index].zone == zone)
        .ok_or(Refusal::NoShowdown)?;
    open(ctx, index);
    Ok(())
}

pub fn open(ctx: &mut Ctx, index: usize) {
    if index >= ctx.blob.staged.len() {
        return;
    }
    let staged = ctx.blob.staged.remove(index);
    let defender = ctx
        .blob
        .holder(staged.zone)
        .filter(|holder| *holder != staged.contester)
        .or_else(|| {
            ctx.seats_with_units(staged.zone)
                .into_iter()
                .find(|seat| *seat != staged.contester)
        })
        .unwrap_or_else(|| ctx.blob.order().next_seat(staged.contester));
    let opened = Showdown {
        combat: staged.combat,
        ..Showdown::open(staged.zone, staged.contester, defender)
    };
    ctx.blob.showdown = Some(opened.clone());
    let what = if staged.combat { "combat" } else { "showdown" };
    ctx.narrate(format!(
        "{what} at {{zone {}}} · {{seat {}}} against {{seat {defender}}}",
        staged.zone, staged.contester
    ));
    if staged.combat && combat::designate(ctx, &opened) > 0 {
        if let Some(showdown) = ctx.blob.showdown.as_mut() {
            showdown.initial_chain = true;
        }
        ctx.narrate("combat triggers form the initial chain");
    }
    auto_pass(ctx);
}

pub fn pass(ctx: &mut Ctx, seat: u8) -> Result<(), Refusal> {
    if ctx.blob.prompt.is_some() {
        return Err(Refusal::PromptOpen);
    }
    let showdown = ctx.blob.showdown.clone().ok_or(Refusal::NoShowdown)?;
    if !showdown.window.has_focus(seat) {
        return Err(Refusal::NotYourFocus);
    }
    ctx.narrate(format!("{{seat {seat}}} passes"));
    if step(ctx, showdown) == Window::Open {
        auto_pass(ctx);
    }
    Ok(())
}

fn step(ctx: &mut Ctx, mut showdown: Showdown) -> Window {
    match showdown.window.pass(&ctx.blob.order()) {
        Window::Open => {
            ctx.blob.showdown = Some(showdown);
            Window::Open
        }
        Window::Closed => {
            ctx.blob.showdown = Some(showdown);
            close(ctx);
            Window::Closed
        }
    }
}

pub fn auto_pass(ctx: &mut Ctx) {
    for _ in 0..ctx.players() {
        let Some(showdown) = ctx.blob.showdown.clone() else {
            return;
        };
        if ctx.blob.prompt.is_some()
            || !ctx.blob.queue.is_empty()
            || state(ctx.blob) != State::ShowdownOpen
        {
            return;
        }
        let focus = showdown.focus();
        if priority::can_act(ctx, focus) {
            return;
        }
        ctx.narrate(format!("{{seat {focus}}} passes · nothing to play"));
        if step(ctx, showdown) == Window::Closed {
            return;
        }
    }
}

pub fn played(ctx: &mut Ctx, seat: u8) {
    if ctx.blob.note_play(seat) {
        auto_pass(ctx);
    }
}

pub fn close(ctx: &mut Ctx) {
    let Some(showdown) = ctx.blob.showdown.take() else {
        return;
    };
    let zone = showdown.zone;
    if showdown.combat {
        combat::close(ctx, showdown);
        return;
    }
    if cleanup::establish(ctx, zone) == Established::Combat {
        ctx.narrate(format!("{{zone {zone}}} stays contested · combat next"));
    }
    cleanup::run(ctx, None);
}

pub fn abandon(ctx: &mut Ctx) {
    let Some(showdown) = ctx.blob.showdown.clone() else {
        return;
    };
    if ctx.blob.why == Some(PromptWhy::Assign) {
        ctx.blob.close_prompt();
    }
    if !showdown.combat {
        close(ctx);
        return;
    }
    ctx.blob.showdown = None;
    combat::clear_designations(ctx);
    expiry::at_combat_end(ctx);
    ctx.blob.set_contested(showdown.zone, None);
    cleanup::establish(ctx, showdown.zone);
    cleanup::run(ctx, None);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, might_this_turn};
    use crate::cards::Card;
    use crate::engine::ctx::{Event, Location, MoveCause, Moved};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{prompts, settle, triggers};
    use crate::rules::COUNTER_POINTS;
    use crate::state::{GameBlob, Mode, Priority};
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::prompt::Answer;
    use agni_plugin_sdk::table::Target;

    fn fresh() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.blob = GameBlob::start(2, 0, Mode::Enforced);
        fixture
    }

    static AMBUSHER: Card = prelude::unit(
        "Ambusher",
        &[crate::cards::Keyword::Ambush],
        &[prelude::on_defend(&[], |ctx, item, _| {
            might_this_turn(ctx, item, item.kind.source(), 1, None);
            prelude::done()
        })],
    );

    #[test]
    fn a_unit_arriving_inside_an_open_showdown_waits_for_it_to_close_and_is_designated_by_the_restaged_combat(
    ) {
        let mut fixture = fresh();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::THEIR_UNIT, &AMBUSHER);
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        let open = ctx.blob.showdown.clone().unwrap();
        assert!(!open.combat);
        assert_eq!(
            ctx.move_unit(
                fixtures::THEIR_UNIT,
                Location::Battlefield(fixtures::BF1),
                MoveCause::Effect
            ),
            Moved::Moved
        );
        cleanup::run(&mut ctx, None);
        assert!(
            ctx.blob.staged.is_empty(),
            "322.13 · the showdown's own zone is not staged again while it is open"
        );
        assert_eq!(
            ctx.blob.showdown.as_ref().map(|held| held.combat),
            Some(false),
            "the open showdown stays a showdown"
        );
        assert!(
            !ctx.in_combat(fixtures::THEIR_UNIT) && !ctx.in_combat(fixtures::VI),
            "no designation before the combat opens"
        );
        pass(&mut ctx, 0).unwrap();
        pass(&mut ctx, 1).unwrap();
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line == "{zone 9} stays contested · combat next"));
        assert_eq!(ctx.blob.staged.len(), 1, "the combat is staged");
        settle(&mut ctx).unwrap();
        let combat = ctx
            .blob
            .showdown
            .clone()
            .expect("the restaged combat opens after the showdown closes");
        assert!(combat.combat);
        assert_eq!((combat.attacker, combat.defender), (0, 1));
        assert!(ctx.is_attacker(fixtures::VI));
        assert!(
            ctx.is_defender(fixtures::THEIR_UNIT),
            "464.2.c.3 · designated when the combat opens"
        );
        assert!(
            combat.initial_chain,
            "its defend trigger fires then, not at the arrival"
        );
        assert_eq!(
            ctx.events
                .iter()
                .filter(|event| matches!(event, Event::Defends { card } if *card == fixtures::THEIR_UNIT))
                .count(),
            1
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 3);
        assert_eq!(ctx.blob.holder(fixtures::BF1), None, "still contested");
        assert!(!ctx.blob.scored(fixtures::BF1, 0), "no conquer yet");
    }

    #[test]
    fn the_four_turn_states_follow_the_showdown_and_the_chain() {
        let mut blob = GameBlob::start(2, 0, Mode::Enforced);
        assert_eq!(state(&blob), State::NeutralOpen);
        blob.priority = Some(Priority {
            active: 0,
            passes: 0,
        });
        assert_eq!(state(&blob), State::NeutralClosed);
        blob.showdown = Some(Showdown::open(9, 0, 1));
        assert_eq!(state(&blob), State::ShowdownClosed);
        blob.priority = None;
        assert_eq!(state(&blob), State::ShowdownOpen);
    }

    #[test]
    fn passes_in_sequence_close_the_showdown_and_a_seat_with_no_hand_is_passed_for() {
        let mut fixture = fresh();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = fixture.ctx();
        assert_eq!(pass(&mut ctx, 0), Err(Refusal::NoShowdown));
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().focus(), 0);
        assert_eq!(pass(&mut ctx, 1), Err(Refusal::NotYourFocus));
        pass(&mut ctx, 0).unwrap();
        let showdown = ctx.blob.showdown.clone().unwrap();
        assert_eq!((showdown.focus(), showdown.passes()), (1, 1));
        assert_eq!(ctx.blob.log.last().unwrap(), "{seat 0} passes");
        ctx.blob.open_prompt(crate::state::Ask {
            prompt: agni_plugin_sdk::prompt::Prompt::new(1, 1, 0, 1),
            why: PromptWhy::PickStaged,
        });
        assert_eq!(pass(&mut ctx, 1), Err(Refusal::PromptOpen));
        ctx.blob.close_prompt();
        pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(ctx.effects, [Effect::score(0, COUNTER_POINTS, 1)]);
        assert_eq!(ctx.blob.log.last().unwrap(), "{seat 0} conquers {zone 9}");
        let mut quiet = fresh();
        quiet
            .table
            .cards
            .retain(|card| card.id != fixtures::THEIR_HAND_CARD);
        quiet.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        quiet.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = quiet.ctx();
        cleanup::run(&mut ctx, None);
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().focus(), 0);
        pass(&mut ctx, 0).unwrap();
        assert!(
            ctx.blob.showdown.is_none(),
            "seat 1 has no hand and is passed for"
        );
        assert_eq!(
            ctx.blob.log,
            [
                "showdown at {zone 9} · {seat 0} against {seat 1}",
                "{seat 0} passes",
                "{seat 1} passes · nothing to play",
                "{seat 0} conquers {zone 9}"
            ]
        );
        let mut hidden = fresh();
        hidden
            .table
            .cards
            .retain(|card| card.id != fixtures::THEIR_HAND_CARD);
        hidden.table.cards.push(fixtures::hidden(
            fixtures::THEIR_HAND_CARD,
            fixtures::BF1,
            1,
        ));
        hidden
            .blob
            .card_state_mut(fixtures::THEIR_HAND_CARD)
            .hidden_at = Some(fixtures::BF1);
        hidden.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        hidden.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = hidden.ctx();
        cleanup::run(&mut ctx, None);
        pass(&mut ctx, 0).unwrap();
        let showdown = ctx
            .blob
            .showdown
            .clone()
            .expect("a facedown card older than this turn keeps the window open");
        assert_eq!(showdown.focus(), 1);
        let mut silent = fresh();
        silent
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND));
        silent.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        silent.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = silent.ctx();
        cleanup::run(&mut ctx, None);
        assert!(
            ctx.blob.showdown.is_none(),
            "nobody can act: the showdown closes as it opens"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert!(ctx.blob.scored(fixtures::BF1, 0));
    }

    #[test]
    fn a_play_resets_the_passes_and_hands_focus_on() {
        let mut fixture = fresh();
        fixture.blob.showdown = Some(Showdown::open(fixtures::BF1, 0, 1));
        fixture.blob.showdown.as_mut().unwrap().window.passes = 1;
        let mut ctx = fixture.ctx();
        played(&mut ctx, 1);
        assert_eq!(
            ctx.blob.showdown.as_ref().unwrap().passes(),
            1,
            "not the focus holder"
        );
        played(&mut ctx, 0);
        let showdown = ctx.blob.showdown.clone().unwrap();
        assert_eq!((showdown.focus(), showdown.passes()), (1, 0));
    }

    #[test]
    fn two_staged_showdowns_ask_the_turn_player_and_the_pick_opens_that_one() {
        let mut fixture = fresh();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(90, fixtures::BF2, 0, "Jinx", 2));
        fixture
            .table
            .cards
            .retain(|card| card.id != fixtures::SPRITE);
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(ctx.blob.staged.len(), 2);
        assert_eq!(ctx.blob.why, Some(PromptWhy::PickStaged));
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!(
            (prompt.seat, prompt.min, prompt.max, prompt.cancel),
            (0, 1, 1, false)
        );
        let offered = prompts::offered(&ctx);
        assert_eq!(
            offered
                .iter()
                .map(|opt| opt.label.as_str())
                .collect::<Vec<_>>(),
            ["{zone 9}", "{zone 10}"]
        );
        assert_eq!(offered[1].answer, Answer::Zone(fixtures::BF2));
        assert_eq!(
            prompts::status(&ctx, PromptWhy::PickStaged),
            "which showdown opens first?"
        );
        assert_eq!(pick(&mut ctx, fixtures::BF3), Err(Refusal::NoShowdown));
        ctx.blob.close_prompt();
        pick(&mut ctx, fixtures::BF2).unwrap();
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().zone, fixtures::BF2);
        assert_eq!(ctx.blob.staged.len(), 1);
        pass(&mut ctx, 0).unwrap();
        pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.holder(fixtures::BF2), Some(0));
        assert!(
            ctx.blob.showdown.is_none(),
            "322.12 · the Conquered event is finalized before the next showdown opens"
        );
        triggers::collect(&mut ctx);
        open_next(&mut ctx);
        let next = ctx
            .blob
            .showdown
            .clone()
            .expect("the other staged showdown opens next");
        assert_eq!(next.zone, fixtures::BF1);
        assert!(ctx.blob.staged.is_empty());
        assert!(ctx.blob.prompt.is_none());
    }

    #[test]
    fn a_combat_deals_damage_and_control_follows_the_survivors() {
        let mut fixture = fresh();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF2);
        fixture.blob.set_holder(fixtures::BF2, Some(1));
        fixture.blob.set_contested(fixtures::BF2, Some(0));
        let mut ctx = fixture.ctx();
        cleanup::run(&mut ctx, None);
        let showdown = ctx.blob.showdown.clone().unwrap();
        assert!(showdown.combat);
        assert_eq!((showdown.attacker, showdown.defender), (0, 1));
        assert!(ctx.is_attacker(fixtures::VI) && ctx.is_defender(fixtures::SPRITE));
        pass(&mut ctx, 0).unwrap();
        pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.showdown.is_none());
        assert_eq!(
            ctx.effects,
            [
                Effect::Annotate {
                    card: fixtures::VI,
                    key: "attacker".into(),
                    value: Some(vec![1])
                },
                Effect::Annotate {
                    card: fixtures::SPRITE,
                    key: "defender".into(),
                    value: Some(vec![1])
                },
                Effect::Counter {
                    target: Target::Card(fixtures::SPRITE),
                    counter: crate::engine::ctx::COUNTER_DAMAGE,
                    delta: 3
                },
                Effect::Counter {
                    target: Target::Card(fixtures::VI),
                    counter: crate::engine::ctx::COUNTER_DAMAGE,
                    delta: 3
                },
                Effect::Move {
                    card: fixtures::VI,
                    zone: fixtures::TRASH,
                    seat: 0,
                    index: TOP
                },
                Effect::Despawn {
                    card: fixtures::SPRITE
                },
            ],
            "three might each · both sides take lethal and leave the field empty"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF2), None);
        assert_eq!(ctx.blob.contester(fixtures::BF2), None);
        assert!(!ctx.blob.scored(fixtures::BF2, 1));
        assert_eq!(
            ctx.blob.log,
            [
                "combat at {zone 10} · {seat 0} against {seat 1}",
                "{seat 0} passes",
                "{seat 1} passes",
                "attackers 3 might vs defenders 3 might",
                "{card 60} takes 3 · {card 50} takes 3",
                "{card 50} dies · {card 60} dies",
                "{zone 10} is left empty"
            ]
        );
        let mut four = fresh();
        four.table.players = 4;
        four.blob = GameBlob::start(4, 0, Mode::Enforced);
        four.table
            .cards
            .push(fixtures::unit(90, fixtures::BF1, 2, "Jinx", 2));
        four.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        four.blob.set_contested(fixtures::BF1, Some(0));
        let mut ctx = four.ctx();
        cleanup::run(&mut ctx, None);
        let showdown = ctx.blob.showdown.clone().unwrap();
        assert_eq!((showdown.defender, showdown.focus()), (2, 0));
        assert!(ctx.is_attacker(fixtures::VI) && ctx.is_defender(90));
        pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.blob.showdown.as_ref().unwrap().focus(), 1);
        pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.blob.showdown.is_none(),
            "seats 2 and 3 have no hand and are passed for"
        );
        assert_eq!(
            ctx.blob.holder(fixtures::BF1),
            Some(0),
            "three might kills the two-might defender and the attacker conquers"
        );
        assert!(ctx.blob.scored(fixtures::BF1, 0));
        assert_eq!(ctx.points(0), 1);
        assert_eq!(
            ctx.table.card(90).and_then(|card| card.zone),
            Some(fixtures::TRASH)
        );
        assert_eq!(
            ctx.table.counter(
                Target::Card(fixtures::VI),
                crate::engine::ctx::COUNTER_DAMAGE
            ),
            Some(0),
            "the survivor's two damage is healed at 2c"
        );
    }
}
