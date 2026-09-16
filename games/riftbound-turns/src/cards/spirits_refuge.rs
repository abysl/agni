use super::prelude::{a_friendly_unit, buff, card_target, done, gear, play, with_statics};
use super::{Card, Flow, Grant, Item, Keyword, Scope, Stage, Static};
use crate::engine::ctx::Ctx;
use crate::engine::{attach, statics};

fn grants_deflect(grants: &[Grant]) -> bool {
    grants
        .iter()
        .any(|grant| matches!(grant, Grant::Keyword(Keyword::Deflect(_))))
}

pub fn has_deflect_of_its_own(ctx: &Ctx, unit: u32) -> bool {
    let printed = ctx.script(unit).is_some_and(|script| {
        script.has_keyword(Keyword::Deflect(1))
            || script.statics.iter().any(|held| match held {
                Static::While(applies, grants) => applies(ctx, unit) && grants_deflect(grants),
                Static::Level(level, grants) => {
                    statics::level_active(ctx, unit, *level) && grants_deflect(grants)
                }
                _ => false,
            })
    });
    let granted = ctx.state_of(unit).is_some_and(|row| {
        row.granted
            .iter()
            .any(|(keyword, _)| keyword.same_kind(Keyword::Deflect(1)))
    });
    let worn = attach::attachments_of(ctx, unit)
        .into_iter()
        .any(|gear| grants_deflect(attach::grants_of(ctx, gear)));
    printed || granted || worn
}

fn first_refuge_of(ctx: &Ctx, seat: u8) -> Option<u32> {
    ctx.faces_on_board()
        .map(|held| held.id)
        .filter(|refuge| {
            ctx.script(*refuge)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .filter(|refuge| ctx.controller(*refuge) == seat && statics::in_play(ctx, *refuge))
        .min()
}

pub fn buffed_friendly_without_deflect(ctx: &Ctx, source: u32, unit: u32) -> bool {
    let seat = ctx.controller(source);
    ctx.controller(unit) == seat
        && ctx.is_buffed(unit)
        && !has_deflect_of_its_own(ctx, unit)
        && first_refuge_of(ctx, seat) == Some(source)
}

fn shelter(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if buff(ctx, unit) {
            ctx.narrate(format!("{{card {unit}}} is buffed"));
        }
    }
    done()
}

pub static CARD: Card = with_statics(
    gear(
        "Spirit's Refuge",
        &[],
        &[play(&[a_friendly_unit("a friendly unit to buff")], shelter)],
    ),
    &[Static::Aura {
        scope: Scope::FriendlyUnits,
        when: buffed_friendly_without_deflect,
        grants: &[Grant::Keyword(Keyword::Deflect(1))],
    }],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{grant_this_turn, FRIENDLY_UNIT};
    use crate::cards::{script_of, Trigger};
    use crate::engine::cost::{self, Need};
    use crate::engine::ctx::COUNTER_BUFFED;
    use crate::engine::fixtures::{self, Fixture};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::{CardInfo, CounterInfo, Target};

    const REFUGE: u32 = 90;
    const SECOND_REFUGE: u32 = 91;
    const VEX: u32 = 92;

    fn refuge(id: u32, zone: u16) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Calm".into()],
            ..fixtures::gear(id, zone, 0, "Spirit's Refuge", 2)
        }
    }

    fn buffed(fixture: &mut Fixture, unit: u32) {
        fixture.table.counters.push(CounterInfo {
            target: Target::Card(unit),
            counter: COUNTER_BUFFED,
            value: 1,
        });
        fixture.table.counters.sort();
    }

    fn sanctuary(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(refuge(REFUGE, zone));
        fixture
            .table
            .cards
            .push(fixtures::unit(VEX, fixtures::BF1, 0, "Vex - Apathetic", 3));
        buffed(&mut fixture, fixtures::THEIR_UNIT);
        for id in [fixtures::RUNE_A, 41, 43] {
            fixture.table.card_mut(id).unwrap().exhausted = false;
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn deflect_tax(ctx: &Ctx, unit: u32) -> usize {
        let mut item = ChainItem::new(
            9,
            ItemKind::Spell {
                card: fixtures::THEIR_HAND_CARD,
            },
            1,
            Origin::Hand,
        );
        item.targets.push(TargetRef::Card(unit));
        cost::of_item(ctx, &item, None)
            .power
            .iter()
            .filter(|need| **need == Need::Rainbow)
            .count()
    }

    #[test]
    fn the_script_buffs_on_play_and_projects_deflect_onto_buffed_friendly_units() {
        assert!(std::ptr::eq(script_of("Spirit's Refuge").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let shelter = &CARD.abilities[0];
        assert_eq!(shelter.trigger, Trigger::Play);
        assert_eq!(shelter.targets[0].filter, FRIENDLY_UNIT);
        assert!(matches!(
            CARD.statics,
            [Static::Aura {
                scope: Scope::FriendlyUnits,
                grants: [Grant::Keyword(Keyword::Deflect(1))],
                ..
            }]
        ));
        assert!(CARD.has_aura());
    }

    #[test]
    fn played_from_hand_it_buffs_the_chosen_friendly_unit_which_then_reads_deflect() {
        let mut fixture = sanctuary(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, REFUGE).unwrap();
        assert!(ctx.on_board(REFUGE));
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target { item: 2, spec: 0 }),
            "the gear is item 1, its play trigger item 2"
        );
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {}}}", fixtures::VI),
                format!("{{card {VEX}}}")
            ],
            "a paid trigger offers no cancel"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert!(!ctx.is_buffed(fixtures::VI), "the buff waits for the chain");
        assert!(!ctx.has_keyword(fixtures::VI, Keyword::Deflect(1)));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_buffed(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(ctx.has_keyword(fixtures::VI, Keyword::Deflect(1)));
        assert_eq!(ctx.deflect_of(fixtures::VI), 1);
        assert_eq!(
            deflect_tax(&ctx, fixtures::VI),
            1,
            "an enemy spell choosing the buffed unit pays one more"
        );
        assert_eq!(
            ctx.deflect_of(VEX),
            1,
            "unbuffed, Vex reads her printed Deflect and nothing from the Refuge"
        );
        assert_eq!(deflect_tax(&ctx, VEX), 1);
        assert!(
            !ctx.has_keyword(fixtures::THEIR_UNIT, Keyword::Deflect(1)),
            "a buffed enemy unit is not friendly to the Refuge"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_that_already_has_deflect_gains_none_from_the_refuge() {
        let mut fixture = sanctuary(fixtures::BASE);
        buffed(&mut fixture, VEX);
        buffed(&mut fixture, fixtures::VI);
        let mut ctx = fixture.ctx();
        assert!(has_deflect_of_its_own(&ctx, VEX), "Vex prints Deflect 1");
        assert_eq!(ctx.deflect_of(VEX), 1, "printed once, projected never");
        assert_eq!(deflect_tax(&ctx, VEX), 1);
        assert!(!has_deflect_of_its_own(&ctx, fixtures::VI));
        assert_eq!(
            ctx.deflect_of(fixtures::VI),
            1,
            "Vi's comes from the Refuge"
        );
        assert!(grant_this_turn(&mut ctx, fixtures::VI, Keyword::Deflect(1)));
        assert!(has_deflect_of_its_own(&ctx, fixtures::VI));
        assert_eq!(
            ctx.deflect_of(fixtures::VI),
            1,
            "granted for the turn, the Refuge steps back"
        );
    }

    #[test]
    fn the_deflect_follows_the_buff_and_leaves_with_it() {
        let mut fixture = sanctuary(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.deflect_of(fixtures::VI), 0);
        assert!(ctx.buff(fixtures::VI));
        assert_eq!(ctx.deflect_of(fixtures::VI), 1);
        assert!(crate::cards::sett_brawler::spend_buff(
            &mut ctx,
            fixtures::VI
        ));
        assert_eq!(ctx.deflect_of(fixtures::VI), 0);
        assert!(ctx.buff(fixtures::VI));
        ctx.table
            .apply_entry(&fixtures::move_action(REFUGE, fixtures::TRASH, 0), 0)
            .unwrap();
        assert_eq!(
            ctx.deflect_of(fixtures::VI),
            0,
            "the Refuge gone, the aura is gone"
        );
    }

    #[test]
    fn two_refuges_project_one_deflect_between_them() {
        let mut fixture = sanctuary(fixtures::BASE);
        fixture
            .table
            .cards
            .push(refuge(SECOND_REFUGE, fixtures::BASE));
        buffed(&mut fixture, fixtures::VI);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert_eq!(
            ctx.deflect_of(fixtures::VI),
            1,
            "'if they didn't already' · the second Refuge adds nothing"
        );
        assert_eq!(deflect_tax(&ctx, fixtures::VI), 1);
        ctx.table
            .apply_entry(&fixtures::move_action(REFUGE, fixtures::TRASH, 0), 0)
            .unwrap();
        assert_eq!(
            ctx.deflect_of(fixtures::VI),
            1,
            "the second Refuge takes over when the first leaves"
        );
    }

    #[test]
    fn an_enemy_unit_is_refused_as_the_play_target_and_the_paid_trigger_cannot_be_declined() {
        let mut fixture = sanctuary(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, REFUGE).unwrap();
        assert!(
            ctx.on_board(REFUGE),
            "paid and on the board before its trigger asks"
        );
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 2, 0, &[fixtures::THEIR_UNIT]),
            Err(crate::Refusal::Illegal(
                crate::engine::legal::Reason::NotALegalTarget
            ))
        );
        assert!(
            !fixtures::labels(&ctx).iter().any(|label| label == "cancel"),
            "a trigger with a friendly unit to choose is not declined"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {VEX}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.is_buffed(VEX));
        assert_eq!(ctx.deflect_of(VEX), 1, "Vex keeps her printed Deflect only");
        drop(ctx);
        let mut alone = sanctuary(fixtures::HAND);
        alone
            .table
            .cards
            .retain(|card| !card.is_kind("Unit") || card.owner == 1);
        alone.resolve();
        let mut ctx = alone.ctx();
        fixtures::play_from_hand(&mut ctx, 0, REFUGE).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.on_board(REFUGE));
        assert!(
            ctx.blob.chain.is_empty(),
            "no friendly unit: the trigger fizzles"
        );
        assert!(!ctx.blob.log.iter().any(|line| line.ends_with(" is buffed")));
    }
}
