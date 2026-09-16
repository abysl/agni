use super::prelude::{a_card, an_item, card_target, done, item_target, play, spell};
use super::{Card, Filter, Flow, Item, Keyword, Rel, Stage};
use crate::engine::ctx::{CounterDest, Ctx};
use crate::engine::{chain, targets};
use crate::state::TargetRef;

const UNIT: usize = 0;
const ITEM: usize = 1;

pub const FRIENDLY_UNIT_AT_A_BATTLEFIELD: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::AtBattlefield,
    Filter::ChosenByEnemyItem(&Filter::ItemOnChain),
]);

pub const ENEMY_ITEM_CHOOSING_IT_ALONE: Filter = Filter::And(&[
    Filter::ItemOnChain,
    Filter::ItemControlledBy(Rel::Enemy),
    Filter::ItemTargetsOnly(UNIT as u8),
]);

pub fn chooses_it_and_no_other_friendly_unit(
    ctx: &Ctx,
    item: &Item,
    chosen: u16,
    unit: u32,
) -> bool {
    ctx.chain_item(chosen)
        .is_some_and(|held| targets::chooses_only(ctx, item.controller, held, unit))
}

fn repulse(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, UNIT) else {
        return done();
    };
    let Some(TargetRef::Item(chosen)) = item.targets.get(ITEM).copied() else {
        return done();
    };
    if item_target(ctx, item, ITEM).is_none() {
        if let Some(held) = ctx.chain_item(chosen) {
            if !chooses_it_and_no_other_friendly_unit(ctx, item, chosen, unit) {
                let source = held.kind.source();
                ctx.narrate(format!(
                    "{{card {source}}} chooses another friendly unit beside {{card {unit}}} · it is not countered"
                ));
            }
        }
        return done();
    }
    chain::counter(ctx, chosen, CounterDest::Trash);
    done()
}

pub static CARD: Card = spell(
    "Repulse",
    &[Keyword::Reaction],
    &[play(
        &[
            a_card(
                FRIENDLY_UNIT_AT_A_BATTLEFIELD,
                "a friendly unit at a battlefield",
            ),
            an_item(
                ENEMY_ITEM_CHOOSING_IT_ALONE,
                "an enemy spell or ability that chooses it and no other friendly unit",
            ),
        ],
        repulse,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, deal, unit as unit_card, units_up_to};
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority, prompts, targets};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const REPULSE: u32 = 90;
    const THEIR_REPULSE: u32 = 91;
    const ZAP: u32 = 92;
    const SWEEP: u32 = 93;
    const SCOUT: u32 = 94;
    const BODY_RUNE: u32 = 46;
    const THEIR_BODY: u32 = 47;
    const TRIGGER: u16 = 40;

    static ZAP_CARD: Card = spell(
        "Zap",
        &[],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = card_target(ctx, item, 0) {
                deal(ctx, item, unit, 1);
            }
            Flow::Done
        })],
    );

    static SWEEP_CARD: Card = spell(
        "Sweep",
        &[],
        &[play(
            &[units_up_to(2, "up to two units")],
            |ctx, item, _| {
                for unit in crate::cards::prelude::card_targets(ctx, item) {
                    deal(ctx, item, unit, 1);
                }
                Flow::Done
            },
        )],
    );

    static SCOUT_CARD: Card = unit_card("Scout", &[], &[]);

    fn repulse_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Repulse", 1, 1);
        card.domain = vec!["Body".into()];
        card
    }

    fn contested() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(repulse_card(REPULSE, 0));
        fixture.table.cards.push(repulse_card(THEIR_REPULSE, 1));
        let mut zap = fixtures::spell(ZAP, fixtures::HAND, 1, "Zap", 1, 0);
        zap.domain = vec!["Mind".into()];
        fixture.table.cards.push(zap);
        let mut sweep = fixtures::spell(SWEEP, fixtures::HAND, 1, "Sweep", 1, 0);
        sweep.domain = vec!["Mind".into()];
        fixture.table.cards.push(sweep);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(SCOUT, fixtures::BF1, 0, "Scout", 2));
        fixture
            .table
            .cards
            .push(fixtures::rune(BODY_RUNE, 0, "Body", false));
        fixture
            .table
            .cards
            .push(fixtures::rune(THEIR_BODY, 1, "Body", false));
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(ZAP, &ZAP_CARD)
            .with_script(SWEEP, &SWEEP_CARD)
            .with_script(SCOUT, &SCOUT_CARD);
        fixture
    }

    fn their_spell_at(ctx: &mut Ctx, spell: u32, picks: &[&str]) {
        {
            let core = ctx.blob.core_mut().unwrap();
            core.player = 1;
        }
        fixtures::play_from_hand(ctx, 1, spell).unwrap();
        for pick in picks {
            fixtures::choose(ctx, 1, pick).unwrap();
        }
        if ctx.blob.prompt.is_some() {
            fixtures::choose(ctx, 1, "done").unwrap();
        }
        priority::pass(ctx, 1).unwrap();
        assert_eq!(priority::holder(ctx), Some(0));
    }

    #[test]
    fn the_script_is_a_reaction_over_a_chosen_friendly_unit_at_a_battlefield_and_the_enemy_item() {
        assert!(std::ptr::eq(script_of("Repulse").unwrap(), &CARD));
        assert_eq!(CARD.name, "Repulse");
        assert_eq!(CARD.keywords, [Keyword::Reaction]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].kind, TargetKind::Card);
        assert_eq!(ability.targets[0].filter, FRIENDLY_UNIT_AT_A_BATTLEFIELD);
        assert_eq!(ability.targets[1].kind, TargetKind::Item);
        assert_eq!(ability.targets[1].filter, ENEMY_ITEM_CHOOSING_IT_ALONE);
        assert_eq!((ability.targets[1].min, ability.targets[1].max), (1, 1));
    }

    #[test]
    fn the_predicate_wants_the_unit_among_the_items_targets_and_no_other_friendly_unit() {
        let mut fixture = contested();
        let ctx = fixture.ctx();
        let mut zap = ChainItem::new(1, ItemKind::Spell { card: ZAP }, 1, Origin::Hand);
        zap.targets = vec![TargetRef::Card(fixtures::VI)];
        ctx.blob.chain.push(zap);
        let repulse = ChainItem::new(2, ItemKind::Spell { card: REPULSE }, 0, Origin::Hand);
        assert!(chooses_it_and_no_other_friendly_unit(
            &ctx,
            &repulse,
            1,
            fixtures::VI
        ));
        assert!(
            !chooses_it_and_no_other_friendly_unit(&ctx, &repulse, 1, SCOUT),
            "the Scout is not chosen"
        );
        ctx.blob.chain[0].targets = vec![TargetRef::Card(fixtures::VI), TargetRef::Card(SCOUT)];
        assert!(
            !chooses_it_and_no_other_friendly_unit(&ctx, &repulse, 1, fixtures::VI),
            "the Scout is another friendly unit"
        );
        ctx.blob.chain[0].targets = vec![
            TargetRef::Card(fixtures::VI),
            TargetRef::Card(fixtures::THEIR_UNIT),
        ];
        assert!(
            chooses_it_and_no_other_friendly_unit(&ctx, &repulse, 1, fixtures::VI),
            "an enemy unit beside it is fine"
        );
        assert!(
            !chooses_it_and_no_other_friendly_unit(&ctx, &repulse, 9, fixtures::VI),
            "no such item"
        );
    }

    #[test]
    fn repulse_counters_the_zap_aimed_at_vi_and_vi_takes_nothing() {
        let mut fixture = contested();
        let mut ctx = fixture.ctx();
        their_spell_at(&mut ctx, ZAP, &["{card 50}"]);
        assert_eq!(ctx.blob.chain.len(), 1);
        fixtures::play_from_hand(&mut ctx, 0, REPULSE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "cancel"],
            "the Scout is at the battlefield but nothing chooses it"
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 1 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {ZAP}}} on the chain"), "cancel".to_string()]
        );
        assert_eq!(
            prompts::status(&ctx, ctx.blob.why.unwrap()),
            "{card 90}: choose an enemy spell or ability that chooses it and no other friendly unit (0 of 1)"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {ZAP}}} on the chain")).unwrap();
        assert_eq!(
            ctx.blob.chain[1].targets,
            [TargetRef::Card(fixtures::VI), TargetRef::Item(1)]
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            3,
            "one energy off the Body rune, which then recycles for the power"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty(), "{:?}", ctx.blob.chain);
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {ZAP}}} is countered")));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line == &format!("{{card {ZAP}}} resolves")));
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert_eq!(ctx.card(ZAP).unwrap().zone, Some(fixtures::TRASH));
        assert_eq!(ctx.card(REPULSE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn an_enemy_trigger_that_chooses_the_unit_is_countered_too() {
        let mut fixture = contested();
        let mut ctx = fixture.ctx();
        let mut trigger = ChainItem::new(
            TRIGGER,
            ItemKind::Trigger {
                source: fixtures::THEIR_UNIT,
                index: 0,
            },
            1,
            Origin::Board,
        );
        trigger.targets = vec![TargetRef::Card(fixtures::VI)];
        trigger.status = crate::state::ItemStatus::Finalized;
        ctx.blob.chain.push(trigger);
        ctx.blob.priority = Some(crate::state::Priority {
            active: 0,
            passes: 0,
        });
        fixtures::play_from_hand(&mut ctx, 0, REPULSE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        eprintln!(
            "DBG chain {:?}\nDBG log {:?}\nDBG why {:?}",
            ctx.blob.chain, ctx.blob.log, ctx.blob.why
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 81} on the chain", "cancel"],
            "an ability is an item to counter as much as a spell"
        );
        fixtures::choose(&mut ctx, 0, "{card 81} on the chain").unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 81} ability is countered".to_string()));
    }

    #[test]
    fn a_sweep_whose_other_pick_becomes_friendly_in_response_is_left_alone_as_repulse_resolves() {
        let mut fixture = contested();
        let mut ctx = fixture.ctx();
        their_spell_at(&mut ctx, SWEEP, &["{card 50}", "{card 81}"]);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [
                TargetRef::Card(fixtures::VI),
                TargetRef::Card(fixtures::THEIR_UNIT)
            ]
        );
        fixtures::play_from_hand(&mut ctx, 0, REPULSE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [
                format!("{{card {SWEEP}}} on the chain"),
                "cancel".to_string()
            ],
            "the Sweep chooses Vi and an enemy unit, so the spec admits it"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {SWEEP}}} on the chain")).unwrap();
        assert!(ctx.set_controller(fixtures::THEIR_UNIT, 0, fixtures::VI));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "the Sweep is still waiting");
        assert!(ctx.blob.log.contains(&format!(
            "{{card {SWEEP}}} chooses another friendly unit beside {{card 50}} · it is not countered"
        )));
        priority::pass(&mut ctx, 1).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(ctx.damage_on(fixtures::VI), 1);
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 1);
    }

    #[test]
    fn a_unit_in_the_base_my_own_spell_and_a_unit_nothing_chooses_are_refused() {
        let mut fixture = contested();
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BASE);
        let mut ctx = fixture.ctx();
        {
            let core = ctx.blob.core_mut().unwrap();
            core.player = 1;
        }
        fixtures::play_from_hand(&mut ctx, 1, ZAP).unwrap();
        eprintln!(
            "DBG2 log {:?}\nDBG2 why {:?} prompt {:?}",
            ctx.blob.log, ctx.blob.why, ctx.blob.prompt
        );
        fixtures::choose(&mut ctx, 1, "{card 50}").unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, REPULSE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "Vi is chosen but sits in the base; the Scout is at a battlefield but unchosen"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 2, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "chosen, but in the base"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 2, 0, &[SCOUT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(REPULSE).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.blob.chain.len(), 1);
        drop(ctx);

        let mut fixture = contested();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        let mine = ChainItem::new(9, ItemKind::Spell { card: REPULSE }, 0, Origin::Hand);
        let spec = &CARD.abilities[0].targets[1];
        assert_eq!(
            targets::candidates(&ctx, &mine, spec),
            Vec::<TargetRef>::new(),
            "my own spell is not an enemy item"
        );
        drop(ctx);

        let mut quiet = contested();
        let ctx = quiet.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: THEIR_REPULSE,
            from: ctx.zones.hand,
            from_seat: 1,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 1, &entry),
            Err(Refusal::NotYourTurn),
            "no chain on the other seat's turn: nothing to react to"
        );
    }

    #[test]
    fn only_items_that_choose_the_picked_unit_alone_are_offered() {
        let mut fixture = contested();
        let mut ctx = fixture.ctx();
        their_spell_at(
            &mut ctx,
            SWEEP,
            &["{card 50}", &format!("{{card {SCOUT}}}")],
        );
        fixtures::play_from_hand(&mut ctx, 0, REPULSE).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "the Sweep chooses the Scout too, so it is not offered"
        );
    }
}
