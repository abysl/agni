use super::prelude::{
    a_card, card_target, chosen_mode, deal, done, kill, modal, mode, spell, GEAR,
};
use super::{Card, Cost, Domain, Filter, Flow, Item, Keyword, ModeSpec, Power, Stage};
use crate::engine::ctx::{Ctx, Killed};

pub const DAMAGE: u8 = 4;
pub const REPEAT: Cost = Cost {
    energy: 4,
    power: &[Power::Domain(Domain::Mind)],
};
pub const UNIT_IN_A_BASE: Filter = Filter::And(&[Filter::Unit, Filter::InBase]);
pub const MODES: &[ModeSpec] = &[
    mode(
        "deal 4 to a unit in a base",
        &[a_card(UNIT_IN_A_BASE, "a unit in a base to deal 4 to")],
        deal_four,
    ),
    mode("kill a gear", &[a_card(GEAR, "a gear to kill")], kill_gear),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Deal,
    Kill,
}

pub fn mode_of(item: &Item) -> Option<Mode> {
    match chosen_mode(item)? {
        0 => Some(Mode::Deal),
        1 => Some(Mode::Kill),
        _ => None,
    }
}

fn deal_four(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

fn kill_gear(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(gear) = card_target(ctx, item, 0) {
        if kill(ctx, item, gear) == Killed::Yes {
            ctx.narrate(format!("{{card {gear}}} dies"));
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Rocket Barrage",
    &[Keyword::Repeat(REPEAT)],
    &[modal(MODES)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::engine::prompts;
    use crate::state::{
        ChainItem, Expiry, GameBlob, ItemKind, Origin, Promise, PromiseEffect, PromiseKind,
        PromptWhy, TargetRef,
    };
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const BARRAGE: u32 = 90;
    const THEIR_BARRAGE: u32 = 91;
    const MY_GEAR: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const MY_EXTRA: [u32; 7] = [46, 47, 48, 49, 100, 101, 102];

    fn barrage_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Rocket Barrage", 4, 1);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(barrage_card(BARRAGE, 0));
        fixture.table.cards.push(barrage_card(THEIR_BARRAGE, 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Trinket", 1));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
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

    #[test]
    fn the_script_is_a_repeatable_plain_spell_over_two_named_modes() {
        assert!(std::ptr::eq(script_of("Rocket Barrage").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert!(ability.targets.is_empty());
        assert_eq!(ability.modes.len(), 2);
        assert_eq!(ability.modes[0].label, "deal 4 to a unit in a base");
        assert_eq!(ability.modes[0].targets.len(), 1);
        assert_eq!(ability.modes[0].targets[0].filter, UNIT_IN_A_BASE);
        assert_eq!(ability.modes[1].label, "kill a gear");
        assert_eq!(ability.modes[1].targets.len(), 1);
        assert_eq!(ability.modes[1].targets[0].filter, GEAR);
        let mut item = ChainItem::new(1, ItemKind::Spell { card: BARRAGE }, 0, Origin::Hand);
        assert_eq!(mode_of(&item), None);
        item.set_mode(0, 0);
        assert_eq!(mode_of(&item), Some(Mode::Deal));
        item.set_mode(0, 1);
        assert_eq!(mode_of(&item), Some(Mode::Kill));
        item.set_mode(0, 2);
        assert_eq!(mode_of(&item), None);
    }

    #[test]
    fn a_unit_in_a_base_takes_four_and_a_gear_dies() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BARRAGE).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Mode {
                item: 1,
                execution: 0
            })
        );
        fixtures::choose(&mut ctx, 0, "deal 4 to a unit in a base").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 81}", "cancel"],
            "units in bases, either side; the Sprite at its battlefield and the gear are not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::THEIR_UNIT)]
        );
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            6,
            "four energy exhausted, one of them recycled for the Mind"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.on_board(fixtures::THEIR_UNIT),
            "four kills a 2-Might Jinx"
        );
        assert!(ctx.blob.log.contains(&"{card 81} takes 4".to_string()));
        assert_eq!(ctx.card(BARRAGE).unwrap().zone, Some(fixtures::TRASH));
        let mut wreck = armed();
        let mut ctx = wreck.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BARRAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, "kill a gear").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 93}", "cancel"],
            "gear alone in the kill mode"
        );
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(THEIR_GEAR));
        assert_eq!(ctx.card(THEIR_GEAR).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.log.contains(&"{card 93} dies".to_string()));
        assert!(ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::Died { card, .. } if *card == THEIR_GEAR)));
        assert_eq!(ctx.damage_on(fixtures::THEIR_UNIT), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn repeated_it_may_choose_differently_each_time() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BARRAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Mode {
                item: 1,
                execution: 0
            })
        );
        fixtures::choose(&mut ctx, 0, "deal 4 to a unit in a base").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Mode {
                item: 1,
                execution: 1
            }),
            "820.2.a · the repeat chooses its own mode before any target"
        );
        assert_eq!(
            prompts::status(
                &ctx,
                PromptWhy::Mode {
                    item: 1,
                    execution: 1
                }
            ),
            "{card 90}: choose one for the repeat"
        );
        fixtures::choose(&mut ctx, 0, "kill a gear").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 92}", "{card 93}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert!(item.repeated());
        assert_eq!(item.spec_counts, [1, 1]);
        assert_eq!(item.mode_at(0), Some(0));
        assert_eq!(item.mode_at(1), Some(1));
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "eight energy exhausted, two of them recycled for the two Mind"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(fixtures::VI), "four kills a 3-Might Vi");
        assert!(!ctx.on_board(THEIR_GEAR));
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert!(ctx.blob.log.contains(&"{card 50} takes 4".to_string()));
        assert!(ctx.blob.log.contains(&"{card 93} dies".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_at_a_battlefield_is_refused_and_an_unaffordable_repeat_is_not_offered() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_BARRAGE)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, BARRAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, "deal 4 to a unit in a base").unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::SPRITE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the Sprite is at a battlefield"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[THEIR_GEAR]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a gear is the other mode's target"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(BARRAGE).unwrap().zone, Some(fixtures::HAND));
        let mut poor = armed();
        poor.table
            .cards
            .retain(|card| !MY_EXTRA[3..].contains(&card.id));
        poor.resolve();
        let mut ctx = poor.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BARRAGE).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Mode {
                item: 1,
                execution: 0
            }),
            "six ready runes cannot pay ten: the repeat is skipped"
        );
    }

    #[test]
    fn the_mode_is_asked_by_name_before_its_target() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BARRAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["deal 4 to a unit in a base", "kill a gear", "cancel"]
        );
    }

    fn reconstruct(fixture: &mut Fixture, table: agni_plugin_sdk::table::Snapshot) {
        fixture.table = table;
        fixture.blob = GameBlob::decode(&fixture.blob.encode()).unwrap();
        fixture.resolve();
    }

    #[test]
    fn printed_and_promised_repeats_keep_three_mode_groups_across_reconstruction() {
        let mut fixture = armed();
        for id in 103..107 {
            fixture
                .table
                .cards
                .push(fixtures::rune(id, 0, "Mind", false));
        }
        fixture.blob.seat_mut(0).promises.push(Promise {
            kind: PromiseKind::Spell,
            effect: PromiseEffect::RepeatForCost,
            until: Expiry::Permanent,
        });
        fixture.resolve();

        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, BARRAGE).unwrap();
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: crate::engine::play::SLOT_PROMISED_REPEAT as u8,
            })
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Mode {
                item: 1,
                execution: 0
            })
        );
        fixtures::choose(&mut ctx, 0, "deal 4 to a unit in a base").unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        reconstruct(&mut fixture, table);

        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "kill a gear").unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Mode {
                item: 1,
                execution: 2
            })
        );
        let table = ctx.table.clone();
        drop(ctx);
        reconstruct(&mut fixture, table);

        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "deal 4 to a unit in a base").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        reconstruct(&mut fixture, table);

        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        let table = ctx.table.clone();
        drop(ctx);
        reconstruct(&mut fixture, table);

        let mut ctx = fixture.ctx();
        fixtures::choose(&mut ctx, 0, "{card 81}").unwrap();
        let item = ctx.blob.chain[0].clone();
        assert_eq!(item.repeats(), 2);
        assert_eq!(item.mode_at(0), Some(0));
        assert_eq!(item.mode_at(1), Some(1));
        assert_eq!(item.mode_at(2), Some(0));
        assert_eq!(
            item.targets,
            [
                TargetRef::Card(50),
                TargetRef::Card(93),
                TargetRef::Card(81)
            ]
        );
        assert!(ctx.blob.seat(0).promises.is_empty());
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(50));
        assert!(!ctx.on_board(81));
        assert!(!ctx.on_board(93));
        assert!(ctx.blob.log.contains(&"{card 50} takes 4".to_string()));
        assert!(ctx.blob.log.contains(&"{card 93} dies".to_string()));
        assert!(ctx.blob.log.contains(&"{card 81} takes 4".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }
}
