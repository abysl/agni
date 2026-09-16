use super::prelude::{
    asking, done, friendly_gear, play, ready, remember_card, remembered_cards, spawn, spell,
    with_candidates, Location, Token,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::engine::march::describe;
use crate::state::TargetRef;

pub const READIED: u8 = 2;
pub const SOLDIER_ARRIVES_READY: bool = false;
pub const LOCATE: u8 = 1;
pub const READY: u8 = 2;
pub const QUESTION: &str = "where each Sand Soldier is played, then two Sand Soldiers to ready";

pub fn equipment_controlled_by(ctx: &Ctx, seat: u8) -> Vec<u32> {
    let mut equipment: Vec<u32> = friendly_gear(ctx, seat)
        .into_iter()
        .filter(|gear| {
            ctx.script(*gear)
                .is_some_and(|script| script.is_equipment())
        })
        .collect();
    equipment.sort_unstable();
    equipment
}

fn location_options(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
    ctx.play_locations(seat)
        .into_iter()
        .filter_map(|location| ctx.zone_of(location))
        .map(|(zone, _)| TargetRef::Zone(zone))
        .collect()
}

fn candidates(ctx: &Ctx, item: &Item, stage: Stage) -> Vec<TargetRef> {
    match stage.0 {
        LOCATE => location_options(ctx, item.controller),
        READY => remembered_cards(item)
            .into_iter()
            .map(TargetRef::Card)
            .collect(),
        _ => Vec::new(),
    }
}

fn play_a_soldier(ctx: &mut Ctx, seat: u8, at: Location, soldiers: &mut Vec<u32>) {
    if let Some(soldier) = spawn(ctx, seat, Token::SandSoldier, at, SOLDIER_ARRIVES_READY) {
        remember_card(ctx, soldier);
        soldiers.push(soldier);
        ctx.narrate(format!(
            "{{seat {seat}}} plays a Sand Soldier to {}",
            describe(at)
        ));
    }
}

fn muster(ctx: &mut Ctx, item: &Item, mut soldiers: Vec<u32>) -> Flow {
    let seat = item.controller;
    let count = equipment_controlled_by(ctx, seat).len();
    if soldiers.len() < count {
        let locations = ctx.play_locations(seat);
        if locations.len() > 1 {
            return Flow::Ask(ctx.ask_resume(item, LOCATE, 1, 1));
        }
        let at = locations.first().copied().unwrap_or(Location::Base(seat));
        while soldiers.len() < count {
            let before = soldiers.len();
            play_a_soldier(ctx, seat, at, &mut soldiers);
            if soldiers.len() == before {
                break;
            }
        }
    }
    if soldiers.len() <= usize::from(READIED) {
        for soldier in soldiers {
            ready(ctx, soldier);
        }
        return done();
    }
    Flow::Ask(ctx.ask_resume(item, READY, READIED, READIED))
}

fn arise(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let seat = item.controller;
    match stage.0 {
        READY => {
            let soldiers = remembered_cards(item);
            let picked: Vec<u32> = ctx
                .picks()
                .iter()
                .copied()
                .filter(|soldier| soldiers.contains(soldier))
                .take(usize::from(READIED))
                .collect();
            for soldier in picked {
                ready(ctx, soldier);
            }
            done()
        }
        LOCATE => {
            let at = ctx
                .picks()
                .first()
                .and_then(|zone| u16::try_from(*zone).ok())
                .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
                .filter(|at| ctx.play_locations(seat).contains(at))
                .unwrap_or(Location::Base(seat));
            let mut soldiers = remembered_cards(item);
            play_a_soldier(ctx, seat, at, &mut soldiers);
            muster(ctx, item, soldiers)
        }
        _ => {
            if equipment_controlled_by(ctx, seat).is_empty() {
                ctx.narrate(format!(
                    "{{seat {seat}}} controls no Equipment · no Sand Soldier is played"
                ));
                return done();
            }
            muster(ctx, item, Vec::new())
        }
    }
}

pub static CARD: Card = spell(
    "Arise!",
    &[],
    &[asking(
        with_candidates(play(&[], arise), candidates),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attach_gear, equip, gear, with_statics, ONE_ENERGY};
    use crate::cards::{script_of, Keyword, Trigger, TOKEN_SAND_SOLDIER};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const ARISE: u32 = 90;
    const THEIR_ARISE: u32 = 91;
    const BLADE: u32 = 92;
    const SHIELD: u32 = 93;
    const HELM: u32 = 94;
    const TRINKET: u32 = 95;
    const THEIR_BLADE: u32 = 96;
    const CALM_RUNE: u32 = 100;
    const ORDER_RUNE: u32 = 101;
    const FIRST_EXTRA_RUNE: u32 = 102;
    const FIRST_SOLDIER: u32 = 200;

    static BLADE_CARD: Card = with_statics(
        gear("Blade", &[Keyword::Equip(ONE_ENERGY)], &[equip(ONE_ENERGY)]),
        &[],
    );

    fn arise(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Arise!", 6, 1);
        card.domain = vec!["Calm".into(), "Order".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(arise(ARISE, 0));
        fixture.table.cards.push(arise(THEIR_ARISE, 1));
        for (id, seat) in [
            (BLADE, 0),
            (SHIELD, 0),
            (HELM, 0),
            (TRINKET, 0),
            (THEIR_BLADE, 1),
        ] {
            fixture
                .table
                .cards
                .push(fixtures::gear(id, fixtures::BASE, seat, "Blade", 1));
        }
        fixture
            .table
            .cards
            .push(fixtures::rune(CALM_RUNE, 0, "Calm", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        for rune in FIRST_EXTRA_RUNE..FIRST_EXTRA_RUNE + 3 {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BLADE, &BLADE_CARD)
            .with_script(SHIELD, &BLADE_CARD)
            .with_script(HELM, &BLADE_CARD)
            .with_script(THEIR_BLADE, &BLADE_CARD);
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

    fn soldiers_of(ctx: &Ctx, seat: u8) -> Vec<u32> {
        ctx.table
            .cards
            .iter()
            .filter(|card| card.name == TOKEN_SAND_SOLDIER && ctx.controller(card.id) == seat)
            .map(|card| card.id)
            .collect()
    }

    fn cast(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, ARISE).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "no location is chosen at play time"
        );
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn place(ctx: &mut Ctx, where_: &str) {
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: LOCATE
            })
        );
        assert_eq!(
            fixtures::labels(ctx),
            ["{zone 8}", "{zone 9}"],
            "the base and the held battlefield · one soldier at a time"
        );
        fixtures::choose(ctx, 0, where_).unwrap();
    }

    fn cast_to(ctx: &mut Ctx, where_: &str) {
        cast(ctx);
        while matches!(ctx.blob.why, Some(PromptWhy::Resume { stage: LOCATE, .. })) {
            place(ctx, where_);
        }
    }

    #[test]
    fn the_script_asks_a_play_location_per_soldier_then_two_soldiers_to_ready() {
        assert!(std::ptr::eq(script_of("Arise!").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(
            ability.targets.is_empty(),
            "185.2.a · each token play picks its own location"
        );
        assert!(ability.candidates.is_some());
        assert_eq!(ability.question, Some(QUESTION));
        assert!(prompts::resume_questions().contains(&QUESTION));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            equipment_controlled_by(&ctx, 0),
            [BLADE, SHIELD, HELM],
            "the Trinket is gear but not Equipment, the enemy Blade is theirs"
        );
        attach_gear(&mut ctx, BLADE, fixtures::VI);
        assert_eq!(
            equipment_controlled_by(&ctx, 0),
            [BLADE, SHIELD, HELM],
            "attached or not, it is controlled"
        );
        assert_eq!(equipment_controlled_by(&ctx, 1), [THEIR_BLADE]);
    }

    #[test]
    fn three_equipment_play_three_exhausted_soldiers_and_the_controller_readies_two() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_to(&mut ctx, "{zone 9}");
        let soldiers = soldiers_of(&ctx, 0);
        assert_eq!(
            soldiers,
            [FIRST_SOLDIER, FIRST_SOLDIER + 1, FIRST_SOLDIER + 2]
        );
        for soldier in &soldiers {
            let held = ctx.card(*soldier).unwrap();
            assert_eq!(held.might, Some(2));
            assert_eq!(held.owner, 0);
            assert!(held.exhausted, "played, so exhausted until readied");
            assert!(ctx.is_token(*soldier));
            assert_eq!(
                ctx.location(*soldier),
                Some(Location::Battlefield(fixtures::BF1))
            );
            assert!(ctx.events.iter().any(|event| matches!(
                event,
                Event::Played { card, controller: 0, .. } if card == soldier
            )));
        }
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: READY
            })
        );
        let prompt = ctx.blob.prompt.clone().unwrap();
        assert_eq!((prompt.seat, prompt.min, prompt.max), (0, 2, 2));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 200}", "{card 201}", "{card 202}"],
            "no skip: two must be readied"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            format!("{{card 90}}: choose {QUESTION} (0 of 2)")
        );
        fixtures::choose(&mut ctx, 0, "{card 200}").unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 201}", "{card 202}"]);
        fixtures::choose(&mut ctx, 0, "{card 202}").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.card(FIRST_SOLDIER).unwrap().exhausted);
        assert!(ctx.card(FIRST_SOLDIER + 1).unwrap().exhausted);
        assert!(!ctx.card(FIRST_SOLDIER + 2).unwrap().exhausted);
        assert_eq!(
            ctx.blob
                .log
                .iter()
                .filter(|line| *line == "{seat 0} plays a Sand Soldier to {zone 9}")
                .count(),
            3
        );
        assert_eq!(ctx.card(ARISE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn each_soldier_picks_its_own_location_and_one_location_asks_nothing() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        place(&mut ctx, "{zone 9}");
        place(&mut ctx, "{zone 8}");
        place(&mut ctx, "{zone 9}");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: READY
            })
        );
        assert_eq!(
            ctx.location(FIRST_SOLDIER),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert_eq!(ctx.location(FIRST_SOLDIER + 1), Some(Location::Base(0)));
        assert_eq!(
            ctx.location(FIRST_SOLDIER + 2),
            Some(Location::Battlefield(fixtures::BF1))
        );
        fixtures::choose(&mut ctx, 0, "{card 200}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 201}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
        drop(ctx);

        let mut fixture = armed();
        fixture.blob.set_holder(fixtures::BF1, None);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BLADE, &BLADE_CARD)
            .with_script(SHIELD, &BLADE_CARD)
            .with_script(HELM, &BLADE_CARD);
        let mut ctx = fixture.ctx();
        cast(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: READY
            }),
            "with only the base to play to, the three land there unasked"
        );
        for soldier in soldiers_of(&ctx, 0) {
            assert_eq!(ctx.location(soldier), Some(Location::Base(0)));
        }
    }

    #[test]
    fn two_or_fewer_soldiers_are_all_readied_without_a_prompt_and_none_plays_nothing() {
        let mut fixture = armed();
        fixture.table.cards.retain(|card| card.id != HELM);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BLADE, &BLADE_CARD)
            .with_script(SHIELD, &BLADE_CARD);
        let mut ctx = fixture.ctx();
        cast_to(&mut ctx, "{zone 8}");
        assert!(ctx.blob.prompt.is_none());
        let soldiers = soldiers_of(&ctx, 0);
        assert_eq!(soldiers.len(), 2);
        for soldier in soldiers {
            assert!(!ctx.card(soldier).unwrap().exhausted);
            assert_eq!(ctx.location(soldier), Some(Location::Base(0)));
        }
        drop(ctx);

        let mut fixture = armed();
        fixture
            .table
            .cards
            .retain(|card| ![BLADE, SHIELD, HELM].contains(&card.id));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        cast_to(&mut ctx, "{zone 8}");
        assert!(
            soldiers_of(&ctx, 0).is_empty(),
            "the Trinket is no Equipment"
        );
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx
            .blob
            .log
            .contains(&"{seat 0} controls no Equipment · no Sand Soldier is played".to_string()));
        assert_eq!(ctx.card(ARISE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn a_battlefield_the_caster_does_not_hold_is_refused_and_the_sorcery_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_ARISE)),
            Err(Refusal::NotYourTurn)
        );
        cast(&mut ctx);
        assert!(
            !fixtures::labels(&ctx).contains(&"{zone 10}".to_string()),
            "the opponent's battlefield is no play location"
        );
        drop(ctx);
        let mut poor = armed();
        poor.table
            .cards
            .retain(|card| !(FIRST_EXTRA_RUNE..FIRST_EXTRA_RUNE + 3).contains(&card.id));
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, ARISE)),
            Err(Refusal::NotEnoughRunes {
                needed: 6,
                ready: 5
            }),
            "five ready runes cannot pay six energy"
        );
    }
}
