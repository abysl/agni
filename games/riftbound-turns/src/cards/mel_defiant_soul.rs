use super::prelude::{
    a_card, activated, ask_discard, banish_by, card_target, discarded_kind, done, named,
    on_empowered, paying_with, unit, usable_if,
};
use super::{Card, Cost, Filter, Flow, Item, Keyword, SelfCost, Source, Stage, Timing, KIND_SPELL};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost::FREE;
pub const MIGHT_AT_MOST: u8 = 3;
pub const DISCARDED: u8 = 1;
pub const ENEMY_UNIT_AT_A_BATTLEFIELD_WITH_THREE_OR_LESS: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Enemy,
    Filter::AtBattlefield,
    Filter::MightAtMost(MIGHT_AT_MOST),
]);

pub fn spells_in_hand(ctx: &Ctx, seat: u8) -> Vec<u32> {
    ctx.hand_of(seat)
        .into_iter()
        .filter(|card| ctx.kind_of(*card) == Some(KIND_SPELL))
        .collect()
}

pub fn a_spell_can_be_discarded_to_empower_her(ctx: &Ctx, source: Source) -> bool {
    !ctx.is_empowered(source.card) && !ctx.hand_of(ctx.controller(source.card)).is_empty()
}

fn empower_by_discarding_a_spell(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    let me = item.kind.source();
    if stage.0 != DISCARDED {
        return match ask_discard(ctx, item, DISCARDED) {
            Some(ask) => Flow::Ask(ask),
            None => {
                ctx.narrate(format!(
                    "{{card {me}}} has no spell to discard · she stays as she is"
                ));
                done()
            }
        };
    }
    if discarded_kind(ctx) != Some(KIND_SPELL) {
        ctx.narrate(format!(
            "the discard was not a spell · {{card {me}}} is not empowered"
        ));
        return done();
    }
    if ctx.empower_by(me, item.controller) {
        ctx.narrate(format!("{{card {me}}} is empowered"));
    }
    done()
}

fn banish_the_lesser(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if banish_by(ctx, unit, item.controller) {
        ctx.narrate(format!(
            "{{card {}}} banishes {{card {unit}}}",
            item.kind.source()
        ));
    }
    done()
}

pub static CARD: Card = unit(
    "Mel, Defiant Soul",
    &[Keyword::Empower(EMPOWER)],
    &[
        usable_if(
            named(
                paying_with(
                    activated(Timing::Sorcery, EMPOWER, &[], empower_by_discarding_a_spell),
                    SelfCost::Free,
                ),
                "empower (discard a spell)",
            ),
            a_spell_can_be_discarded_to_empower_her,
        ),
        on_empowered(
            &[a_card(
                ENEMY_UNIT_AT_A_BATTLEFIELD_WITH_THREE_OR_LESS,
                "an enemy unit at a battlefield with 3 Might or less to banish",
            )],
            banish_the_lesser,
        ),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{activate, play as play_engine, priority};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MEL: u32 = 90;
    const THEIR_BRUTE: u32 = 91;
    const THEIR_SCOUT: u32 = 92;

    fn mel() -> CardInfo {
        CardInfo {
            energy: Some(5),
            domain: vec!["Chaos".into()],
            ..fixtures::unit(MEL, fixtures::BASE, 0, "Mel, Defiant Soul", 4)
        }
    }

    fn salon() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(mel());
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_BRUTE, fixtures::BF1, 1, "Brute", 5));
        fixture
            .table
            .cards
            .push(fixtures::unit(THEIR_SCOUT, fixtures::BF1, 1, "Scout", 2));
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(MEL).unwrap(), &CARD));
        fixture
    }

    fn her_offers(ctx: &Ctx) -> Vec<activate::Offer> {
        activate::offers(ctx, 0)
            .into_iter()
            .filter(|offer| offer.source == MEL)
            .collect()
    }

    fn activate_and_resolve(ctx: &mut Ctx) -> u16 {
        activate::activate(ctx, 0, MEL, 0).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        let item = ctx.blob.chain[0].id;
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        item
    }

    #[test]
    fn the_script_prints_a_free_empower_paid_by_a_discard_and_banishes_when_empowered() {
        assert!(std::ptr::eq(script_of("Mel, Defiant Soul").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Empower(Cost::FREE)]);
        assert_eq!(CARD.empower_cost(), Some(Cost::FREE));
        assert_eq!(CARD.abilities.len(), 2);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(Cost::FREE));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert!(empower.usable.is_some());
        assert!(empower.targets.is_empty());
        let banish = &CARD.abilities[1];
        assert_eq!(banish.trigger, Trigger::Empowered);
        assert!(!banish.optional);
        assert_eq!(banish.targets.len(), 1);
        assert_eq!((banish.targets[0].min, banish.targets[0].max), (1, 1));
        assert_eq!(
            banish.targets[0].filter,
            ENEMY_UNIT_AT_A_BATTLEFIELD_WITH_THREE_OR_LESS
        );
        assert_eq!(MIGHT_AT_MOST, 3);
    }

    #[test]
    fn discarding_a_spell_empowers_her_and_she_banishes_a_lesser_enemy_at_a_battlefield() {
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        assert_eq!(spells_in_hand(&ctx, 0), [fixtures::HAND_SPELL]);
        let offers = her_offers(&ctx);
        assert_eq!(offers.len(), 1);
        assert_eq!(
            offers[0].label,
            format!("{{card {MEL}}}: empower (discard a spell)")
        );
        let ready = ctx.ready_runes_of(0).len();
        let item = activate_and_resolve(&mut ctx);
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Discard {
                item,
                stage: DISCARDED
            }),
            "the discard is asked as the ability resolves"
        );
        assert!(!ctx.is_empowered(MEL), "not before the discard");
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.card(fixtures::HAND_SPELL).unwrap().zone,
            Some(fixtures::TRASH)
        );
        assert_eq!(ctx.ready_runes_of(0).len(), ready, "no energy, no power");
        assert!(ctx.is_empowered(MEL));
        assert!(ctx.events.contains(&Event::Empowered { card: MEL, by: 0 }));
        assert!(ctx.blob.chain.is_empty(), "the trigger waits on its target");
        let trigger = ctx
            .blob
            .queue
            .iter()
            .find(|pending| {
                matches!(pending.item.kind, ItemKind::Trigger { source, index: 1 } if source == MEL)
            })
            .map(|pending| pending.item.id)
            .expect("the become-Empowered trigger is pending");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Target {
                item: trigger,
                spec: 0
            })
        );
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        let mut expected = [
            format!("{{card {}}}", fixtures::SPRITE),
            format!("{{card {THEIR_SCOUT}}}"),
        ];
        expected.sort();
        assert_eq!(
            offered, expected,
            "the 5 Might Brute and the enemy in their base are not offered"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, trigger, 0, &[THEIR_BRUTE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, trigger, 0, &[fixtures::THEIR_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::SPRITE)).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == MEL
        ));
        assert!(ctx.on_board(fixtures::SPRITE), "not until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(ctx.on_board(THEIR_SCOUT));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {MEL}}} banishes {{card {}}}",
            fixtures::SPRITE
        )));
        assert_eq!(
            activate::activate(&mut ctx, 0, MEL, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered))
        );
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn with_an_empty_hand_the_empower_is_refused_and_a_unit_discard_leaves_her_unempowered() {
        let mut fixture = salon();
        fixture
            .table
            .cards
            .retain(|card| card.zone != Some(fixtures::HAND) || card.owner != 0);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        assert!(ctx.hand_of(0).is_empty());
        assert!(her_offers(&ctx).is_empty());
        assert_eq!(
            activate::activate(&mut ctx, 0, MEL, 0),
            Err(Refusal::Illegal(Reason::AlreadyEmpowered)),
            "the usable gate refuses without a card to discard"
        );
        assert_eq!(
            activate::activate(&mut ctx, 1, MEL, 0),
            Err(Refusal::Illegal(Reason::NotYourCard))
        );
        drop(ctx);

        let mut blind = salon();
        for card in blind.table.cards.iter_mut() {
            if card.zone == Some(fixtures::HAND) && card.owner == 0 {
                card.name = String::new();
                card.kind = None;
            }
        }
        blind.resolve();
        let ctx = blind.ctx();
        assert!(
            spells_in_hand(&ctx, 0).is_empty(),
            "the plugin cannot read hands"
        );
        assert_eq!(
            her_offers(&ctx).len(),
            1,
            "the offer stands on the hand's size, not its faces"
        );
        drop(ctx);

        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        activate_and_resolve(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_UNIT)).unwrap();
        assert!(!ctx.is_empowered(MEL));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.log.contains(&format!(
            "the discard was not a spell · {{card {MEL}}} is not empowered"
        )));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn with_no_lesser_enemy_at_a_battlefield_the_trigger_fizzles() {
        let mut fixture = salon();
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BASE);
        fixture.table.card_mut(THEIR_SCOUT).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        activate_and_resolve(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert!(ctx.is_empowered(MEL));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert!(ctx.on_board(fixtures::SPRITE));
        assert!(ctx.on_board(THEIR_SCOUT));
        assert!(ctx.on_board(THEIR_BRUTE));
        assert!(ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("no legal target")));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_lone_lesser_enemy_is_chosen_unasked_and_banished() {
        let mut fixture = salon();
        fixture.table.card_mut(THEIR_SCOUT).unwrap().zone = Some(fixtures::BASE);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        activate_and_resolve(&mut ctx);
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert!(
            ctx.blob.prompt.is_none(),
            "one legal target needs no question"
        );
        assert_eq!(ctx.blob.chain.len(), 1);
        assert_eq!(
            ctx.blob.chain[0].targets,
            [TargetRef::Card(fixtures::SPRITE)]
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "engine gap · non-resource costs at the pay stage (355.10.c, 357): the discard-a-spell Empower cost must be picked at the pay stage among the spells in hand alone and refused otherwise, uncounterable; today the discard is any hand card asked at resolution (mel_defiant_soul::spells_in_hand is the candidate list)"]
    fn the_discard_is_paid_at_activation_and_offers_only_spells() {
        let mut fixture = salon();
        let mut ctx = fixture.ctx();
        activate::activate(&mut ctx, 0, MEL, 0).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::HAND_SPELL)],
            "the unit in hand is not a spell"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::HAND_SPELL)).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the paid ability waits on the chain"
        );
        assert!(!ctx.is_empowered(MEL));
    }
}
