use super::prelude::{asking, done, on_conquer_me, optional, unit, with_candidates};
use super::{Card, Filter, Flow, Item, Keyword, Paying, Stage, TargetKind, TargetSpec, KIND_SPELL};
use crate::engine::ctx::Ctx;
use crate::engine::{cost, pay, play as play_engine, targets};
use crate::state::{ChainItem, ItemKind, Leave, Origin, TargetRef};

const STAGE_PICKED: u8 = 1;
pub const QUESTION: &str = "a spell in your trash with Energy cost less than your points";

pub const FRIENDLY_SPELL_IN_TRASH: Filter =
    Filter::And(&[Filter::Kind(KIND_SPELL), Filter::InTrash, Filter::Friendly]);

pub const SPELL_IN_TRASH: TargetSpec = TargetSpec {
    filter: FRIENDLY_SPELL_IN_TRASH,
    min: 0,
    max: 1,
    kind: TargetKind::Card,
    label: QUESTION,
    min_at_level: None,
};

fn cheaper_than_points(ctx: &Ctx, seat: u8, spell: u32) -> bool {
    let energy = ctx
        .card(spell)
        .and_then(|face| face.energy)
        .map(i32::from)
        .unwrap_or(0);
    energy < ctx.points(seat)
}

fn replay(seat: u8, spell: u32) -> ChainItem {
    ChainItem::new(
        0,
        ItemKind::Spell { card: spell },
        seat,
        Origin::Trash {
            leave: Leave::Recycle,
        },
    )
}

pub fn evolutions(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    let seat = item.controller;
    targets::candidates(ctx, item, &SPELL_IN_TRASH)
        .into_iter()
        .filter(|spell| match spell {
            TargetRef::Card(card) => {
                let again = replay(seat, *card);
                cheaper_than_points(ctx, seat, *card)
                    && targets::first_spec_fillable(ctx, &again)
                    && pay::affordable_for(
                        ctx,
                        seat,
                        &cost::of_item(ctx, &again, None),
                        Paying::Item(&again),
                    )
            }
            _ => false,
        })
        .collect()
}

fn evolve(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        STAGE_PICKED => {
            let offered = evolutions(ctx, item, stage);
            let Some(spell) = ctx
                .picks()
                .first()
                .copied()
                .filter(|spell| offered.contains(&TargetRef::Card(*spell)))
            else {
                return done();
            };
            let seat = item.controller;
            ctx.narrate(format!(
                "{{seat {seat}}} plays {{card {spell}}} from the trash without its Energy cost · recycled after"
            ));
            let _ = play_engine::begin(
                ctx,
                seat,
                spell,
                Origin::Trash {
                    leave: Leave::Recycle,
                },
                None,
            );
            done()
        }
        _ => {
            if evolutions(ctx, item, Stage(STAGE_PICKED)).is_empty() {
                return done();
            }
            Flow::Ask(ctx.ask_resume(item, STAGE_PICKED, 0, 1))
        }
    }
}

pub static CARD: Card = unit(
    "Kai'Sa - Evolutionary",
    &[Keyword::Ganking],
    &[asking(
        with_candidates(optional(on_conquer_me(&[], evolve)), evolutions),
        QUESTION,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, card_target, might_this_turn, play, spell};
    use crate::cards::script_of;
    use crate::cards::{Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{cleanup, priority, prompts, settle};
    use crate::state::PromptWhy;
    use agni_plugin_sdk::decide::{Effect, BOTTOM};
    use agni_plugin_sdk::table::CardInfo;

    const KAISA: u32 = 90;
    const CHEAP: u32 = 91;
    const DEAR: u32 = 92;
    const THEIRS: u32 = 93;

    static PUMP: Card = spell(
        "Pump",
        &[],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = card_target(ctx, item, 0) {
                might_this_turn(ctx, item, unit, 2, None);
            }
            Flow::Done
        })],
    );

    fn kaisa(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(6),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(KAISA, zone, seat, "Kai'Sa - Evolutionary", 6)
        }
    }

    fn void(points: i32) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(kaisa(fixtures::BF1, 0));
        fixture
            .table
            .cards
            .push(fixtures::spell(CHEAP, fixtures::TRASH, 0, "Pump", 1, 0));
        fixture
            .table
            .cards
            .push(fixtures::spell(DEAR, fixtures::TRASH, 0, "Pump", 2, 0));
        fixture
            .table
            .cards
            .push(fixtures::spell(THEIRS, fixtures::TRASH, 1, "Pump", 0, 0));
        fixture.blob.set_contested(fixtures::BF1, Some(0));
        fixture.set_points(0, points);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(CHEAP, &PUMP)
            .with_script(DEAR, &PUMP)
            .with_script(THEIRS, &PUMP);
        fixture
    }

    fn conquer(ctx: &mut Ctx) {
        assert_eq!(
            cleanup::establish(ctx, fixtures::BF1),
            cleanup::Established::Conquered(0)
        );
        settle(ctx).unwrap();
    }

    fn resolve_until_asked(ctx: &mut Ctx) {
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == KAISA
        ));
        assert!(ctx.blob.prompt.is_none(), "the pick comes at resolution");
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    #[test]
    fn the_script_prints_ganking_and_one_optional_conquer_trigger_that_offers_trash_spells() {
        assert!(std::ptr::eq(
            script_of("Kai'Sa - Evolutionary").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Kai'Sa - Evolutionary");
        assert_eq!(CARD.keywords, [Keyword::Ganking]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert_eq!(CARD.abilities.len(), 1);
        let conquer = &CARD.abilities[0];
        assert_eq!(conquer.trigger, Trigger::Conquer(Who::Me));
        assert!(conquer.optional, "you may play");
        assert!(conquer.targets.is_empty());
        assert!(conquer.candidates.is_some());
        assert_eq!(conquer.question, Some(QUESTION));
        assert!(conquer.cost.is_none());
        assert!(conquer.condition.is_none());
    }

    #[test]
    fn conquering_at_two_points_offers_only_the_spell_costing_less_plays_it_free_of_energy_and_recycles_it(
    ) {
        let mut fixture = void(1);
        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 2, "the conquer's point counts");
        resolve_until_asked(&mut ctx);
        let item = match ctx.blob.why {
            Some(PromptWhy::Resume { item, stage }) => {
                assert_eq!(stage, STAGE_PICKED);
                item
            }
            other => panic!("the trigger asks for a spell, not {other:?}"),
        };
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CHEAP}}}"), "skip".to_string()],
            "1 < 2 offers the cheap one; 2 < 2 does not; the opponent's is not yours"
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Resume {
                    item,
                    stage: STAGE_PICKED
                }
            ),
            format!("{{card {KAISA}}}: choose {QUESTION} (0 of 1)")
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHEAP}}}")).unwrap();
        assert_eq!(ctx.card(CHEAP).unwrap().zone, Some(fixtures::CHAIN));
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Target { .. })));
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        let top = ctx.blob.chain.last().unwrap();
        assert!(matches!(top.kind, ItemKind::Spell { card } if card == CHEAP));
        assert_eq!(
            top.origin,
            Origin::Trash {
                leave: Leave::Recycle
            }
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready,
            "no energy and a free Power cost: nothing was paid"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 5, "the spell resolved");
        assert_eq!(ctx.card(CHEAP).unwrap().zone, Some(fixtures::MAIN_DECK));
        assert!(ctx.effects.contains(&Effect::Move {
            card: CHEAP,
            zone: fixtures::MAIN_DECK,
            seat: 0,
            index: BOTTOM
        }));
        assert_eq!(ctx.trash_of(0), [DEAR], "recycled, not trashed again");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_power_cost_is_still_paid() {
        let mut fixture = void(1);
        fixture.table.card_mut(CHEAP).unwrap().power = Some(1);
        fixture.table.card_mut(CHEAP).unwrap().domain = vec!["Calm".into()];
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(CHEAP, &PUMP);
        let mut ctx = fixture.ctx();
        let ready = ctx.ready_runes_of(0).len();
        conquer(&mut ctx);
        resolve_until_asked(&mut ctx);
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {CHEAP}}}"), "skip".to_string()]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {CHEAP}}}")).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - 1,
            "one Calm rune for the Power; the Energy is waived"
        );
    }

    #[test]
    fn skipping_plays_nothing_and_leaves_the_trash_alone() {
        let mut fixture = void(1);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        resolve_until_asked(&mut ctx);
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.trash_of(0), [CHEAP, DEAR]);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }

    #[test]
    fn with_no_spell_cheaper_than_the_points_the_trigger_resolves_without_asking() {
        let mut fixture = void(0);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert_eq!(ctx.points(0), 1);
        resolve_until_asked(&mut ctx);
        assert!(
            ctx.blob.prompt.is_none(),
            "1 energy is not less than 1 point: {:?}",
            ctx.blob.why
        );
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.trash_of(0), [CHEAP, DEAR]);
    }

    #[test]
    fn a_conquer_without_her_offers_nothing() {
        let mut fixture = void(5);
        fixture.table.card_mut(KAISA).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(CHEAP, &PUMP)
            .with_script(DEAR, &PUMP);
        let mut ctx = fixture.ctx();
        conquer(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.trash_of(0), [CHEAP, DEAR]);
    }
}
