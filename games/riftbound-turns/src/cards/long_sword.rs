use super::prelude::{
    asking, attach_gear, done, equip, friendly_units, gear, play, while_attached, with_candidates,
    with_statics,
};
use super::{Ability, Card, Cost, Domain, Flow, Grant, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;
use crate::state::TargetRef;

pub const EQUIP: Cost = Cost {
    energy: 0,
    power: &[Power::Domain(Domain::Fury)],
};

pub const MIGHT_BONUS: i16 = 2;

pub static EFFECT_TEXT: &[Grant] = &[Grant::Might(MIGHT_BONUS)];

pub const QUICK_DRAW_QUESTION: &str = "a unit you control to wear it";
pub const WEARER: u8 = 1;

pub fn quick_draw_reacts(ctx: &Ctx, card: u32) -> bool {
    ctx.has_keyword(card, Keyword::QuickDraw)
}

pub fn quick_draw_wearers(ctx: &Ctx, item: &Item, _: Stage) -> Vec<TargetRef> {
    friendly_units(ctx, item.controller)
        .into_iter()
        .map(TargetRef::Card)
        .collect()
}

pub fn quick_draw_attach(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let gear = item.kind.source();
    if !ctx.on_board(gear) {
        return done();
    }
    if stage.0 != WEARER {
        if quick_draw_wearers(ctx, item, stage).is_empty() {
            ctx.narrate(format!(
                "{{card {gear}}} finds no unit of its controller's to attach to"
            ));
            return done();
        }
        return Flow::Ask(ctx.ask_resume(item, WEARER, 1, 1));
    }
    if let Some(unit) = ctx.picks().first().copied() {
        attach_gear(ctx, gear, unit);
    }
    done()
}

pub const fn quick_draw() -> Ability {
    asking(
        with_candidates(play(&[], quick_draw_attach), quick_draw_wearers),
        QUICK_DRAW_QUESTION,
    )
}

pub static CARD: Card = with_statics(
    gear(
        "Long Sword",
        &[Keyword::QuickDraw, Keyword::Reaction, Keyword::Equip(EQUIP)],
        &[quick_draw(), equip(EQUIP)],
    ),
    &[while_attached(EFFECT_TEXT)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attached_to, is_attached, FRIENDLY_UNIT};
    use crate::cards::{script_of, SelfCost, Static, Timing, Trigger};
    use crate::engine::ctx::{Ctx, EntryMove, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{activate, hide, priority, prompts};
    use crate::state::{ChainItem, ItemKind, Origin, Priority, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    static WITHOUT_REACTION: Card = with_statics(
        gear(
            "Long Sword",
            &[Keyword::QuickDraw, Keyword::Equip(EQUIP)],
            &[quick_draw(), equip(EQUIP)],
        ),
        &[while_attached(EFFECT_TEXT)],
    );

    const SWORD: u32 = 90;
    const QUICK_DRAW: u8 = 0;
    const EQUIP_INDEX: u8 = 1;
    const ENERGY: u8 = 2;

    fn sword(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::gear(SWORD, zone, seat, "Long Sword", ENERGY)
        }
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(sword(zone, 0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(SWORD).unwrap(), &CARD));
        fixture
    }

    fn resolve_chain(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
    }

    fn entry(ctx: &Ctx) -> EntryMove {
        EntryMove {
            card: SWORD,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn close_the_chain(ctx: &mut Ctx) {
        ctx.blob.chain.push(ChainItem::new(
            7,
            ItemKind::Spell { card: 99 },
            1,
            Origin::Hand,
        ));
        ctx.blob.priority = Some(Priority {
            active: 0,
            passes: 1,
        });
    }

    #[test]
    fn the_script_is_a_quick_draw_equipment_with_a_reaction_a_play_attach_and_plus_two() {
        assert!(std::ptr::eq(script_of("Long Sword").unwrap(), &CARD));
        assert_eq!(CARD.name, "Long Sword");
        assert_eq!(
            CARD.keywords,
            [Keyword::QuickDraw, Keyword::Reaction, Keyword::Equip(EQUIP)],
            "819.1.b · Quick-Draw has Reaction inherently"
        );
        assert_eq!(CARD.equip_cost(), Some(EQUIP));
        assert_eq!(CARD.abilities.len(), 2);
        let draw = &CARD.abilities[usize::from(QUICK_DRAW)];
        assert_eq!(draw.trigger, Trigger::Play);
        assert!(
            draw.targets.is_empty(),
            "819.1.d · attaching chooses no target"
        );
        assert!(draw.candidates.is_some());
        assert_eq!(draw.question, Some(QUICK_DRAW_QUESTION));
        assert!(!draw.optional);
        assert!(prompts::resume_questions().contains(&QUICK_DRAW_QUESTION));
        let equip = &CARD.abilities[usize::from(EQUIP_INDEX)];
        assert_eq!(equip.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(equip.cost, Some(EQUIP));
        assert_eq!(equip.self_cost, SelfCost::Free);
        assert_eq!(equip.label, Some("equip"));
        assert_eq!(equip.targets[0].filter, FRIENDLY_UNIT);
        assert!(CARD.has_static(Static::WhileAttached(&[])));
        assert!(matches!(CARD.attached_grants(), [Grant::Might(2)]));
        assert_eq!(MIGHT_BONUS, 2);
    }

    #[test]
    fn playing_it_attaches_it_to_a_chosen_unit_you_control_as_the_play_trigger_resolves() {
        let mut fixture = armed(fixtures::HAND);
        fixture
            .table
            .cards
            .push(fixtures::unit(95, fixtures::BF1, 0, "Wailer", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SWORD).unwrap();
        assert_eq!(ctx.location(SWORD), Some(Location::Base(0)));
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the play trigger waits on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: QUICK_DRAW } if source == SWORD
        ));
        assert!(!is_attached(&ctx, SWORD));
        resolve_chain(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 2,
                stage: WEARER
            })
        );
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 95}"],
            "every unit you control, at any location; Jinx and the Sprite are not yours"
        );
        fixtures::choose(&mut ctx, 0, "{card 95}").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, SWORD), Some(95));
        assert_eq!(ctx.current_might(95), 4, "+2 while attached");
        assert_eq!(
            ctx.location(SWORD),
            Some(Location::Battlefield(fixtures::BF1)),
            "the sword stands where the wearer stands"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn it_plays_into_a_closed_chain_as_a_reaction_and_finds_no_wearer_without_a_unit() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        close_the_chain(&mut ctx);
        assert!(
            matches!(
                legal::classify(&ctx, 0, &entry(&ctx)),
                Ok(legal::Intent::Play { on_chain: true, .. })
            ),
            "819.1.c · Quick-Draw plays at Reaction timing"
        );
        assert!(quick_draw_reacts(&ctx, SWORD));
        assert!(
            hide::reacts(&ctx, SWORD),
            "the explicit Reaction stands in until hide::reacts reads QuickDraw"
        );
        drop(ctx);

        let mut alone = armed(fixtures::HAND);
        alone.table.cards.retain(|card| card.id != fixtures::VI);
        alone.resolve();
        let mut ctx = alone.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SWORD).unwrap();
        resolve_chain(&mut ctx);
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, SWORD));
        assert_eq!(ctx.location(SWORD), Some(Location::Base(0)));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} finds no unit of its controller's to attach to".to_string()));
    }

    #[test]
    fn a_loose_sword_equips_for_a_fury_rune_and_an_attached_one_cannot_be_re_equipped() {
        let mut fixture = armed(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            activate::activate(&mut ctx, 1, SWORD, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        activate::activate(&mut ctx, 0, SWORD, EQUIP_INDEX).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.runes_of(0).len(), 3, "one Fury rune recycled");
        resolve_chain(&mut ctx);
        assert_eq!(attached_to(&ctx, SWORD), Some(fixtures::VI));
        assert_eq!(ctx.current_might(fixtures::VI), 5);
        assert_eq!(
            activate::activate(&mut ctx, 0, SWORD, EQUIP_INDEX),
            Err(Refusal::Illegal(Reason::Attached))
        );
        ctx.detach(SWORD);
        assert_eq!(ctx.current_might(fixtures::VI), 3);
    }

    #[test]
    #[ignore = "engine gap · Quick-Draw: hide::reacts and legal::timing read Keyword::Reaction only, so the stub carries an explicit Reaction; with QuickDraw read as Reaction timing the explicit keyword goes and the on-play attach stays"]
    fn quick_draw_alone_gives_the_sword_reaction_timing() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        close_the_chain(&mut ctx);
        let scripts = ctx.scripts.clone().with_script(SWORD, &WITHOUT_REACTION);
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        let ctx = Ctx::fresh(&table, &mut blob, &scripts, 0);
        assert!(hide::reacts(&ctx, SWORD));
        assert!(matches!(
            legal::classify(&ctx, 0, &entry(&ctx)),
            Ok(legal::Intent::Play { on_chain: true, .. })
        ));
    }
}
