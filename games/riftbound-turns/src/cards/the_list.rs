use super::prelude::{
    a_unit_with_the_named_tag, activated, card_target, done, exhausting_self, gear,
    might_this_turn, named, naming, usable_if,
};
use super::{Card, Cost, Flow, Item, NameKind, Source, Stage, Timing};
use crate::engine::ctx::Ctx;

pub const PENALTY: i16 = -2;
pub const WEAKEN: u8 = 0;

pub fn named_tag(ctx: &Ctx, list: u32) -> Option<String> {
    ctx.named(list).map(str::to_string)
}

pub fn a_tag_is_named(ctx: &Ctx, source: Source) -> bool {
    named_tag(ctx, source.card).is_some()
}

pub fn bears_tag_until_tags_land(ctx: &Ctx, card: u32, tag: &str) -> bool {
    ctx.bears_tag(card, tag)
}

pub fn weaken_if_tagged(ctx: &mut Ctx, item: &Item, unit: u32, tag: Option<&str>) -> bool {
    let list = item.kind.source();
    let Some(tag) = tag else {
        ctx.narrate(format!("{{card {list}}} names no tag · nothing happens"));
        return false;
    };
    if !bears_tag_until_tags_land(ctx, unit, tag) {
        ctx.narrate(format!("{{card {unit}}} is not a {tag} · nothing happens"));
        return false;
    }
    might_this_turn(ctx, item, unit, PENALTY, None);
    ctx.narrate(format!("{{card {unit}}} gets {PENALTY} Might this turn"));
    true
}

fn weaken(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let tag = named_tag(ctx, item.kind.source());
    weaken_if_tagged(ctx, item, unit, tag.as_deref());
    done()
}

pub static CARD: Card = naming(
    gear(
        "The List",
        &[],
        &[named(
            usable_if(
                exhausting_self(activated(
                    Timing::Sorcery,
                    Cost::FREE,
                    &[a_unit_with_the_named_tag(
                        "a unit with the named tag to weaken",
                    )],
                    weaken,
                )),
                a_tag_is_named,
            ),
            "give a unit with the named tag -2 Might this turn",
        )],
    ),
    NameKind::Tag,
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, SelfCost, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const LIST: u32 = 90;
    const PORO: u32 = 91;

    fn list(exhausted: bool) -> CardInfo {
        CardInfo {
            domain: vec!["Chaos".into()],
            exhausted,
            ..fixtures::gear(LIST, fixtures::BASE, 0, CARD.name, 1)
        }
    }

    fn ledger(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(list(exhausted));
        fixture
            .table
            .cards
            .push(fixtures::unit(PORO, fixtures::BASE, 1, "Lonely Poro", 3));
        fixture.resolve();
        fixture
    }

    fn activation() -> Item {
        Item::new(
            7,
            ItemKind::Ability {
                source: LIST,
                index: WEAKEN,
            },
            0,
            Origin::Board,
        )
    }

    #[test]
    fn the_script_is_one_exhaust_activation_gated_on_a_named_tag_and_the_naming_is_a_seam() {
        assert!(std::ptr::eq(script_of("The List").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(
            CARD.abilities.len(),
            1,
            "the naming happens as it is played, not on the chain"
        );
        let weaken = &CARD.abilities[usize::from(WEAKEN)];
        assert_eq!(weaken.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(weaken.self_cost, SelfCost::Exhaust);
        assert_eq!(weaken.cost, Some(Cost::FREE));
        assert!(weaken.usable.is_some(), "no named tag, no unit to choose");
        assert_eq!(
            weaken.targets[0].filter,
            crate::cards::prelude::UNIT_WITH_THE_NAMED_TAG
        );
        assert_eq!(CARD.names, Some(NameKind::Tag));
        assert_eq!(PENALTY, -2);
        let mut fixture = ledger(false);
        let ctx = fixture.ctx();
        assert_eq!(named_tag(&ctx, LIST), None);
        assert!(!a_tag_is_named(
            &ctx,
            Source {
                card: LIST,
                ability: WEAKEN
            }
        ));
    }

    #[test]
    fn until_tags_land_a_tag_is_read_off_the_printed_name_as_a_word_or_the_champion() {
        let mut fixture = ledger(false);
        let ctx = fixture.ctx();
        assert!(bears_tag_until_tags_land(&ctx, PORO, "Poro"));
        assert!(!bears_tag_until_tags_land(&ctx, PORO, "Lonely Poro"));
        assert!(bears_tag_until_tags_land(
            &ctx,
            fixtures::THEIR_UNIT,
            "Jinx"
        ));
        assert!(
            bears_tag_until_tags_land(&ctx, fixtures::CHAMPION_CARD, "Lillia"),
            "the champion before the dash"
        );
        assert!(!bears_tag_until_tags_land(&ctx, fixtures::VI, "Poro"));
        assert!(!bears_tag_until_tags_land(&ctx, fixtures::VI, ""));
        assert!(!bears_tag_until_tags_land(&ctx, 999, "Vi"));
    }

    #[test]
    fn the_weaken_takes_two_might_for_the_turn_from_a_tagged_unit_and_nothing_from_the_rest() {
        let mut fixture = ledger(false);
        let mut ctx = fixture.ctx();
        let item = activation();
        assert!(weaken_if_tagged(&mut ctx, &item, PORO, Some("Poro")));
        assert_eq!(ctx.current_might(PORO), 3 + i32::from(PENALTY));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {PORO}}} gets -2 Might this turn")));
        assert!(!weaken_if_tagged(
            &mut ctx,
            &item,
            fixtures::VI,
            Some("Poro")
        ));
        assert_eq!(ctx.current_might(fixtures::VI), 3);
        assert!(ctx.blob.log.contains(&format!(
            "{{card {}}} is not a Poro · nothing happens",
            fixtures::VI
        )));
        assert!(!weaken_if_tagged(&mut ctx, &item, PORO, None));
        assert_eq!(
            ctx.current_might(PORO),
            3 + i32::from(PENALTY),
            "no second penalty"
        );
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(PORO), 3);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_tag_named_the_activation_is_refused_and_so_are_a_spent_list_and_the_other_seat() {
        let mut fixture = ledger(false);
        let mut ctx = fixture.ctx();
        assert!(!activate::offers(&ctx, 0)
            .iter()
            .any(|offer| offer.source == LIST));
        assert_eq!(
            activate::activate(&mut ctx, 0, LIST, WEAKEN),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the usable gate refuses with the engine's one gate reason"
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, LIST, WEAKEN),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        assert!(!ctx.card(LIST).unwrap().exhausted);
        assert!(ctx.blob.queue.is_empty());
        drop(ctx);
        let mut spent = ledger(true);
        let ctx = spent.ctx();
        assert!(matches!(
            activate::legal(&ctx, 0, LIST, WEAKEN),
            Err(Refusal::Exhausted | Refusal::Illegal(Reason::AlreadyEmpowered))
        ));
    }

    #[test]
    fn playing_the_list_names_a_tag_and_the_activation_weakens_a_unit_bearing_it() {
        let mut fixture = ledger(false);
        fixture.table.card_mut(LIST).unwrap().zone = Some(fixtures::HAND);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, LIST).unwrap();
        fixtures::choose(&mut ctx, 0, "Poro").unwrap();
        assert_eq!(named_tag(&ctx, LIST).as_deref(), Some("Poro"));
        activate::activate(&mut ctx, 0, LIST, WEAKEN).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {PORO}}}"), "cancel".to_string()]
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {PORO}}}")).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.current_might(PORO), 3 + i32::from(PENALTY));
    }
}
