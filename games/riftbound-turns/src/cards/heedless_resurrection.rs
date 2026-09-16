use super::cruel_patron::pay_kill_cost;
use super::prelude::{
    a_friendly_unit, asking, card_target, done, play, remember_card, remembered_cards, spell,
    with_candidates, Location, Price,
};
use super::the_harrowing::play_from_trash;
use super::{Card, Flow, Item, Keyword, Stage, KIND_UNIT};
use crate::engine::ctx::{Ctx, Killed};
use crate::state::TargetRef;

pub const QUESTION: &str =
    "a unit in your trash costing no more than the killed unit, then where it is played";
pub const STAGE_PICK: u8 = 1;
pub const STAGE_LOCATE: u8 = 2;
pub const KILLS: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Cap {
    pub energy: u8,
    pub power: u8,
}

pub fn cap_of(ctx: &Ctx, card: u32) -> Cap {
    ctx.card(card)
        .map(|held| Cap {
            energy: held.energy.unwrap_or(0),
            power: held.power.unwrap_or(0),
        })
        .unwrap_or_default()
}

pub fn within(ctx: &Ctx, card: u32, cap: Cap) -> bool {
    let held = cap_of(ctx, card);
    held.energy <= cap.energy && held.power <= cap.power
}

pub fn resurrectable(ctx: &Ctx, seat: u8, cap: Cap) -> Vec<u32> {
    let mut units: Vec<u32> = ctx
        .trash_of(seat)
        .into_iter()
        .filter(|card| ctx.kind_of(*card) == Some(KIND_UNIT))
        .filter(|card| within(ctx, *card, cap))
        .collect();
    units.sort_unstable();
    units
}

pub fn killed_as_the_additional_cost_until_play_pays_it_at_the_pay_stage(
    ctx: &mut Ctx,
    item: &Item,
) -> Option<u32> {
    let unit = card_target(ctx, item, 0)?;
    if ctx.controller(unit) != item.controller {
        ctx.narrate(format!(
            "{{card {unit}}} is no longer a friendly unit · the cost can't be paid"
        ));
        return None;
    }
    (pay_kill_cost(ctx, unit) == Killed::Yes).then_some(unit)
}

fn killed(item: &Item) -> Option<u32> {
    remembered_cards(item).first().copied()
}

fn cap_for(ctx: &Ctx, item: &Item) -> Cap {
    killed(item)
        .map(|unit| cap_of(ctx, unit))
        .unwrap_or_default()
}

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    let seat = item.controller;
    match stage.0 {
        STAGE_PICK => resurrectable(ctx, seat, cap_for(ctx, item))
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        stage if stage >= STAGE_LOCATE => location_options(ctx, seat),
        _ => Vec::new(),
    }
}

fn resurrect(ctx: &mut Ctx, item: &Item, unit: u32, cap: Cap) -> Flow {
    let seat = item.controller;
    let locations = ctx.play_locations(seat);
    let Some(index) = resurrectable(ctx, seat, cap)
        .iter()
        .position(|held| *held == unit)
    else {
        return done();
    };
    let Ok(stage) = u8::try_from(index).map(|index| index.saturating_add(STAGE_LOCATE)) else {
        return done();
    };
    if locations.len() > 1 {
        return Flow::Ask(ctx.ask_resume(item, stage, 1, 1));
    }
    play_from_trash(ctx, item, unit, vec![locations[0]], Price::Free);
    done()
}

fn heedless(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    if stage.0 >= STAGE_LOCATE {
        let units = resurrectable(ctx, seat, cap_for(ctx, item));
        let Some(unit) = units.get(usize::from(stage.0 - STAGE_LOCATE)).copied() else {
            return done();
        };
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
    if stage.0 == STAGE_PICK {
        let Some(unit) = ctx.picks().first().copied() else {
            return done();
        };
        return resurrect(ctx, item, unit, cap_for(ctx, item));
    }
    let Some(dead) = killed_as_the_additional_cost_until_play_pays_it_at_the_pay_stage(ctx, item)
    else {
        ctx.narrate(format!(
            "{{card {}}} does nothing · its additional cost was never paid",
            item.kind.source()
        ));
        return done();
    };
    remember_card(ctx, dead);
    let cap = cap_of(ctx, dead);
    let units = resurrectable(ctx, seat, cap);
    match units.as_slice() {
        [] => {
            ctx.narrate(format!(
                "{{seat {seat}}} has no unit in the trash costing {} or less energy and {} or less power",
                cap.energy, cap.power
            ));
            done()
        }
        [only] => resurrect(ctx, item, *only, cap),
        _ => Flow::Ask(ctx.ask_resume(item, STAGE_PICK, 1, 1)),
    }
}

pub static CARD: Card = spell(
    "Heedless Resurrection",
    &[Keyword::Reaction],
    &[asking(
        with_candidates(
            play(
                &[a_friendly_unit(
                    "a friendly unit to kill as an additional cost",
                )],
                heedless,
            ),
            candidates,
        ),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::FRIENDLY_UNIT;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::engine::{priority, prompts};
    use crate::state::{Leave, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const RESURRECTION: u32 = 90;
    const THEIR_RESURRECTION: u32 = 91;
    const BRUTE: u32 = 92;
    const CHEAP: u32 = 93;
    const DEAR: u32 = 94;
    const POWERED: u32 = 95;
    const THEIRS: u32 = 96;
    const CHAOS_RUNES: [u32; 2] = [46, 47];
    const THEIR_CHAOS: [u32; 2] = [48, 49];

    fn resurrection(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Heedless Resurrection", 2, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn trashed(id: u32, seat: u8, name: &str, energy: u8, power: u8) -> CardInfo {
        CardInfo {
            energy: Some(energy),
            power: Some(power),
            ..fixtures::unit(id, fixtures::TRASH, seat, name, 2)
        }
    }

    fn graveyard() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(resurrection(RESURRECTION, 0));
        fixture
            .table
            .cards
            .push(resurrection(THEIR_RESURRECTION, 1));
        fixture.table.cards.push(CardInfo {
            energy: Some(4),
            power: Some(1),
            ..fixtures::unit(BRUTE, fixtures::BF1, 0, "Brute", 4)
        });
        fixture.table.cards.push(trashed(CHEAP, 0, "Scout", 2, 0));
        fixture.table.cards.push(trashed(DEAR, 0, "Titan", 5, 1));
        fixture.table.cards.push(trashed(POWERED, 0, "Mage", 3, 2));
        fixture.table.cards.push(trashed(THEIRS, 1, "Jinx", 1, 0));
        for rune in CHAOS_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        for rune in THEIR_CHAOS {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Chaos", false));
        }
        fixture.resolve();
        fixture
    }

    #[test]
    fn the_script_is_a_reaction_whose_pick_is_the_friendly_unit_that_pays_and_asks_at_resolution() {
        assert!(std::ptr::eq(
            script_of("Heedless Resurrection").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Heedless Resurrection");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        assert_eq!(KILLS, 1);
    }

    #[test]
    fn the_cap_reads_the_killed_units_printed_cost_and_the_trash_is_filtered_by_it() {
        let mut fixture = graveyard();
        let ctx = fixture.ctx();
        assert_eq!(
            cap_of(&ctx, BRUTE),
            Cap {
                energy: 4,
                power: 1
            }
        );
        assert_eq!(
            cap_of(&ctx, fixtures::SPRITE),
            Cap::default(),
            "a token prints no cost"
        );
        let cap = cap_of(&ctx, BRUTE);
        assert!(within(&ctx, CHEAP, cap));
        assert!(!within(&ctx, DEAR, cap), "five energy is one too many");
        assert!(!within(&ctx, POWERED, cap), "two power is one too many");
        assert_eq!(resurrectable(&ctx, 0, cap), [CHEAP]);
        assert_eq!(
            resurrectable(&ctx, 1, cap),
            [THEIRS],
            "each seat reads its own trash"
        );
        assert_eq!(
            resurrectable(
                &ctx,
                0,
                Cap {
                    energy: 5,
                    power: 2
                }
            ),
            [CHEAP, DEAR, POWERED]
        );
    }

    #[test]
    fn the_brute_dies_as_the_cost_and_the_one_unit_it_covers_is_played_free_to_the_base() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESURRECTION).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [
                "{card 50}".to_string(),
                format!("{{card {BRUTE}}}"),
                "cancel".to_string()
            ],
            "my units only"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "two energy off five ready runes, the Chaos one then recycling for the power"
        );
        assert!(ctx.on_board(BRUTE), "nothing until it resolves");
        let ready = ctx.ready_runes_of(0).len();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.in_trash(BRUTE));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {BRUTE}}}"), format!("{{card {CHEAP}}}")],
            "the corpse fits its own cost; the Scout fits under it"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHEAP}}}")).unwrap();
        assert!(ctx.blob.prompt.is_none(), "one location: no question");
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Died { card, controller: 0, unit: true, .. } if *card == BRUTE
        )));
        assert_eq!(ctx.location(CHEAP), Some(Location::Base(0)));
        assert!(ctx.card(CHEAP).unwrap().exhausted);
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Trash { leave: Leave::Recycle }, .. } if *card == CHEAP
        )));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready,
            "ignoring its cost: nothing more is paid"
        );
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {BRUTE}}} is killed as an additional cost")));
        assert!(ctx.blob.log.contains(&format!(
            "{{seat 0}} plays {{card {CHEAP}}} from the trash for nothing"
        )));
        assert!(ctx.in_trash(DEAR) && ctx.in_trash(POWERED));
        assert_eq!(ctx.card(RESURRECTION).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_dearer_kill_widens_the_choice_and_the_killed_unit_itself_can_come_back() {
        let mut fixture = graveyard();
        {
            let brute = fixture.table.card_mut(BRUTE).unwrap();
            brute.energy = Some(5);
            brute.power = Some(2);
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESURRECTION).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_PICK
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {BRUTE}}}"),
                format!("{{card {CHEAP}}}"),
                format!("{{card {DEAR}}}"),
                format!("{{card {POWERED}}}")
            ],
            "the Brute is in the trash now and covers its own cost"
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 1, 1));
        assert!(!prompt.cancel);
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card {RESURRECTION}}}: choose {QUESTION} (0 of 1)")
        );
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(BRUTE), TargetRef::Card(BRUTE)],
            "the kill target and the remembered corpse"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_LOCATE
            }),
            "two play locations: where does it enter?"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{zone {}}}", fixtures::BASE),
                format!("{{zone {}}}", fixtures::BF1)
            ]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{zone {}}}", fixtures::BF1)).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.card(BRUTE).unwrap().exhausted);
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "the resurrection paid two energy and one power, the Brute nothing"
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn the_other_seat_reacts_to_my_spell_and_resurrects_from_its_own_trash() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, THEIR_RESURRECTION).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "cancel"],
            "seat 1's units only"
        );
        fixtures::choose(&mut ctx, 1, "{card 81}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert!(
            ctx.in_trash(fixtures::THEIR_UNIT),
            "the reaction resolved first"
        );
        assert_eq!(
            ctx.blob.prompt.as_ref().map(|prompt| prompt.seat),
            Some(1),
            "seat 1 picks from its own trash"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81}".to_string(), format!("{{card {THEIRS}}}")],
            "the 2-energy corpse and the 1-energy Jinx beneath it"
        );
        fixtures::choose(&mut ctx, 1, &format!("{{card {THEIRS}}}")).unwrap();
        assert_eq!(
            ctx.location(THEIRS),
            Some(Location::Base(1)),
            "Jinx at 1 energy fits under the 2-energy corpse"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 1, origin: Origin::Trash { .. }, .. } if *card == THEIRS
        )));
        assert_eq!(ctx.blob.chain.len(), 1, "my spell still waits beneath");
        assert!(!ctx.blob.log.iter().any(|line| line == "{card 71} resolves"));
        assert!(ctx.in_trash(CHEAP), "my trash is not theirs");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.log.contains(&"{card 71} resolves".to_string()));
    }

    #[test]
    fn a_sprite_pays_with_nothing_so_only_a_free_unit_returns_and_a_stolen_unit_cannot_pay() {
        let mut fixture = graveyard();
        fixture.table.card_mut(fixtures::SPRITE).unwrap().owner = 0;
        fixture.table.card_mut(fixtures::SPRITE).unwrap().seat = 0;
        fixture.table.card_mut(CHEAP).unwrap().energy = Some(0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESURRECTION).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "the token is gone once killed"
        );
        assert_eq!(
            ctx.location(CHEAP),
            Some(Location::Base(0)),
            "a 0/0 unit fits"
        );
        assert!(ctx.in_trash(DEAR));
        drop(ctx);

        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESURRECTION).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.set_controller(BRUTE, 1, fixtures::THEIR_UNIT);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(BRUTE), "the other seat's unit now: no kill");
        assert!(ctx.in_trash(CHEAP), "no cost, no resurrection");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} does nothing · its additional cost was never paid".to_string()));
    }

    #[test]
    fn with_nothing_cheap_enough_the_kill_still_stands_and_wrong_picks_are_refused() {
        let mut fixture = graveyard();
        fixture.table.card_mut(fixtures::SPRITE).unwrap().owner = 0;
        fixture.table.cards.retain(|card| card.id != CHEAP);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESURRECTION).unwrap();
        for wrong in [fixtures::THEIR_UNIT, fixtures::HAND_GEAR, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(
            ctx.card(fixtures::SPRITE).is_none(),
            "357 · a paid cost is not refunded"
        );
        assert!(ctx.in_trash(DEAR) && ctx.in_trash(POWERED));
        assert!(ctx.blob.log.contains(
            &"{seat 0} has no unit in the trash costing 0 or less energy and 0 or less power"
                .to_string()
        ));
        assert!(ctx.blob.chain.is_empty());
        drop(ctx);

        let mut fixture = graveyard();
        fixture.table.cards.retain(|card| card.id != CHEAP);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESURRECTION).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.location(BRUTE),
            Some(Location::Base(0)),
            "the corpse always fits its own cost: the Brute comes straight back"
        );
        assert!(ctx.in_trash(DEAR) && ctx.in_trash(POWERED));
        drop(ctx);

        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESURRECTION).unwrap();
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(RESURRECTION).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.on_board(BRUTE));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    #[ignore = "engine gap · a mandatory non-resource additional cost (kill a friendly unit) at the pay stage; play::advance knows optional rune costs only, so the kill is a play-time pick paid as the spell resolves, where a response can steal the unit and void the cost"]
    fn the_kill_is_paid_before_the_spell_is_on_the_chain_and_no_response_can_undo_it() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RESURRECTION).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(
            ctx.in_trash(BRUTE),
            "357.2 · the kill is paid at the pay stage, before priority"
        );
        assert!(ctx.blob.chain[0].paid_additional());
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.location(CHEAP), Some(Location::Base(0)));
    }
}
