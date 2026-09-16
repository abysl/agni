use super::prelude::{
    a_friendly_unit, an_enemy_unit, asking, card_target, deal, detach_gear, done, equipment_of,
    play, spell, with_candidates,
};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

const STRIKER: usize = 0;
const STRUCK: usize = 1;
const DETACH: u8 = 1;

pub fn is_equipped(ctx: &Ctx, unit: u32) -> bool {
    !equipment_of(ctx, unit).is_empty()
}

fn equipment_of_the_striker(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    card_target(ctx, item, STRIKER)
        .map(|unit| equipment_of(ctx, unit))
        .unwrap_or_default()
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

fn strike(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let Some(striker) = card_target(ctx, item, STRIKER) else {
        return done();
    };
    if stage.0 == DETACH {
        let worn = equipment_of(ctx, striker);
        if let Some(gear) = ctx
            .picks()
            .first()
            .copied()
            .filter(|picked| worn.contains(picked))
        {
            detach_gear(ctx, gear);
        }
        return done();
    }
    if !is_equipped(ctx, striker) {
        ctx.narrate(format!("{{card {striker}}} wears no Equipment"));
        return done();
    }
    let might = u8::try_from(ctx.current_might(striker)).unwrap_or(0);
    if let Some(struck) = card_target(ctx, item, STRUCK) {
        if deal(ctx, item, struck, might) {
            ctx.narrate(format!(
                "{{card {striker}}} deals {might} to {{card {struck}}}"
            ));
        }
    }
    match equipment_of(ctx, striker).as_slice() {
        [] => done(),
        [only] => {
            detach_gear(ctx, *only);
            done()
        }
        _ => Flow::Ask(ctx.ask_resume(item, DETACH, 1, 1)),
    }
}

pub static CARD: Card = spell(
    "Strike Down",
    &[],
    &[asking(
        with_candidates(
            play(
                &[
                    a_friendly_unit("an equipped friendly unit"),
                    an_enemy_unit("an enemy unit"),
                ],
                strike,
            ),
            equipment_of_the_striker,
        ),
        "an Equipment to detach",
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{
        attach_gear, equip, gear, is_attached, while_attached, with_statics, ENEMY_UNIT,
        FRIENDLY_UNIT, ONE_ENERGY,
    };
    use crate::cards::{script_of, Grant, Keyword, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, prompts};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const STRIKE: u32 = 90;
    const THEIR_STRIKE: u32 = 91;
    const BLADE: u32 = 92;
    const SHIELD: u32 = 93;
    const TRINKET: u32 = 94;
    const BODY_RUNE: u32 = 100;

    static BLADE_CARD: Card = with_statics(
        gear("Blade", &[Keyword::Equip(ONE_ENERGY)], &[equip(ONE_ENERGY)]),
        &[while_attached(&[Grant::Might(2)])],
    );

    static SHIELD_CARD: Card = with_statics(
        gear(
            "Shield",
            &[Keyword::Equip(ONE_ENERGY)],
            &[equip(ONE_ENERGY)],
        ),
        &[while_attached(&[Grant::Might(1)])],
    );

    fn strike(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Strike Down", 3, 1);
        card.domain = vec!["Body".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(strike(STRIKE, 0));
        fixture.table.cards.push(strike(THEIR_STRIKE, 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(BLADE, fixtures::BASE, 0, "Blade", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(SHIELD, fixtures::BASE, 0, "Shield", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(TRINKET, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(6);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(BLADE, &BLADE_CARD)
            .with_script(SHIELD, &SHIELD_CARD);
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

    fn cast_at_jinx(ctx: &mut Ctx) {
        fixtures::play_from_hand(ctx, 0, STRIKE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(fixtures::labels(ctx), ["{card 60}", "{card 81}", "cancel"]);
        fixtures::choose(ctx, 0, "{card 81}").unwrap();
        fixtures::pass_until_open(ctx);
    }

    #[test]
    fn the_script_targets_a_friendly_unit_then_an_enemy_unit_and_asks_which_equipment_to_shed() {
        assert!(std::ptr::eq(script_of("Strike Down").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT);
        assert_eq!(ability.targets[1].filter, ENEMY_UNIT);
        assert_eq!(ability.question, Some("an Equipment to detach"));
        assert!(prompts::resume_questions().contains(&"an Equipment to detach"));
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert!(!is_equipped(&ctx, fixtures::VI));
        attach_gear(&mut ctx, TRINKET, fixtures::VI);
        assert!(
            !is_equipped(&ctx, fixtures::VI),
            "a plain gear attached by an effect is not Equipment"
        );
        attach_gear(&mut ctx, BLADE, fixtures::VI);
        assert!(is_equipped(&ctx, fixtures::VI));
        assert_eq!(equipment_of(&ctx, fixtures::VI), [BLADE]);
    }

    #[test]
    fn the_equipped_unit_deals_its_full_might_and_sheds_its_only_equipment() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, BLADE, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        cast_at_jinx(&mut ctx);
        assert!(ctx.blob.prompt.is_none(), "one Equipment needs no pick");
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.damage_on(fixtures::THEIR_UNIT),
            5,
            "3 printed plus 2 from the Blade, read before the detach"
        );
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::DamageDealt { card, n: 5, .. } if *card == fixtures::THEIR_UNIT
        )));
        assert!(ctx.on_board(fixtures::THEIR_UNIT), "6 Might survives 5");
        assert!(!is_attached(&ctx, BLADE));
        assert!(ctx.on_board(BLADE), "detached, not killed");
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} deals 5 to {card 81}".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} detaches from {card 50}".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn with_two_equipment_the_controller_picks_which_one_detaches_and_the_other_stays() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, BLADE, fixtures::VI);
        attach_gear(&mut ctx, SHIELD, fixtures::VI);
        attach_gear(&mut ctx, TRINKET, fixtures::VI);
        assert_eq!(ctx.current_might(fixtures::VI), 6);
        cast_at_jinx(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: DETACH
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 93}"],
            "the Trinket is attached but is no Equipment"
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "{card 90}: choose an Equipment to detach (0 of 1)"
        );
        assert_eq!(
            ctx.damage_on(fixtures::THEIR_UNIT),
            6,
            "the damage landed before the pick"
        );
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        assert!(!is_attached(&ctx, SHIELD));
        assert!(is_attached(&ctx, BLADE));
        assert!(is_attached(&ctx, TRINKET));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn lethal_damage_kills_the_enemy_and_a_striker_that_lost_its_gear_first_does_nothing() {
        let mut fixture = armed();
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().might = Some(4);
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(BLADE, &BLADE_CARD);
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, BLADE, fixtures::VI);
        cast_at_jinx(&mut ctx);
        assert_eq!(
            ctx.card(fixtures::THEIR_UNIT).unwrap().zone,
            Some(fixtures::TRASH),
            "5 damage on 4 Might · {:?}",
            ctx.blob.log
        );
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, BLADE, fixtures::VI);
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        detach_gear(&mut ctx, BLADE);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} wears no Equipment".to_string()));
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn enemy_units_cannot_strike_and_the_sorcery_is_the_turn_players() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STRIKE)),
            Err(Refusal::NotYourTurn)
        );
        attach_gear(&mut ctx, BLADE, fixtures::VI);
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        for wrong in [
            fixtures::THEIR_UNIT,
            fixtures::SPRITE,
            BLADE,
            fixtures::HAND_UNIT,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a friendly unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the struck unit must be an enemy"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(STRIKE).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    #[ignore = "engine gap · Filter has no Equipped arm: the first target is any friendly unit and an unequipped one is offered, then does nothing at resolution; is_equipped is the predicate a Filter::Equipped would read"]
    fn an_unequipped_friendly_unit_is_not_offered_as_the_striker() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::unit(95, fixtures::BASE, 0, "Bare", 2));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, BLADE, fixtures::VI);
        fixtures::play_from_hand(&mut ctx, 0, STRIKE).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "cancel"]);
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[95]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
    }
}
