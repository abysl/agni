use super::prelude::{a_card, card_target, deal, done, play, spell, with_statics};
use super::{Card, Domain, Filter, Flow, Item, Keyword, Stage, Static};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 4;
pub const ENEMY_CALM_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::Domain(Domain::Calm)]);

pub fn cant_be_countered(_: &Ctx, _: u32) -> bool {
    true
}

fn decree(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        if deal(ctx, item, unit, DAMAGE) {
            ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
        }
    }
    done()
}

pub static CARD: Card = with_statics(
    spell(
        "Decree of Rage",
        &[Keyword::Action],
        &[play(
            &[a_card(ENEMY_CALM_UNIT, "an enemy Calm unit")],
            decree,
        )],
    ),
    &[Static::Uncounterable(cant_be_countered)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, a_spell, counter_spell};
    use crate::cards::{script_of, TargetKind, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority};
    use crate::state::{ChainItem, ItemKind, Origin, PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const DECREE: u32 = 90;
    const MONK: u32 = 91;
    const MY_MONK: u32 = 92;
    const NEGATE: u32 = 93;
    const THEIR_RUNES: [u32; 2] = [46, 47];

    static NEGATE_CARD: Card = prelude::spell(
        "Negate",
        &[Keyword::Reaction],
        &[play(&[a_spell("a spell to counter")], |ctx, item, _| {
            counter_spell(ctx, item, 0);
            Flow::Done
        })],
    );

    fn calm_unit(id: u32, zone: u16, seat: u8, name: &str, might: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Calm".into()],
            ..fixtures::unit(id, zone, seat, name, might)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::spell(
            DECREE,
            fixtures::HAND,
            0,
            "Decree of Rage",
            1,
            1,
        ));
        fixture
            .table
            .cards
            .push(calm_unit(MONK, fixtures::BF1, 1, "Monk", 4));
        fixture
            .table
            .cards
            .push(calm_unit(MY_MONK, fixtures::BASE, 0, "Monk", 4));
        fixture
            .table
            .cards
            .push(fixtures::spell(NEGATE, fixtures::HAND, 1, "Negate", 1, 0));
        for rune in THEIR_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 1, "Mind", false));
        }
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(NEGATE, &NEGATE_CARD);
        assert!(std::ptr::eq(
            fixture.scripts.of_card(DECREE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, DECREE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
    }

    #[test]
    fn the_script_is_an_action_over_an_enemy_calm_unit_that_names_itself_uncounterable() {
        assert!(std::ptr::eq(script_of("Decree of Rage").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, ENEMY_CALM_UNIT);
        assert_eq!(ability.targets[0].kind, TargetKind::Card);
        assert_eq!(DAMAGE, 4);
        assert!(matches!(
            CARD.statics,
            [Static::Uncounterable(applies)] if std::ptr::fn_addr_eq(*applies, cant_be_countered as fn(&Ctx, u32) -> bool)
        ));
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert!(cant_be_countered(&ctx, DECREE));
        let item = ChainItem::new(1, ItemKind::Spell { card: DECREE }, 0, Origin::Hand);
        assert!(ctx.uncounterable(&item));
        let negate = ChainItem::new(2, ItemKind::Spell { card: NEGATE }, 1, Origin::Hand);
        assert!(!ctx.uncounterable(&negate));
    }

    #[test]
    fn only_enemy_calm_units_are_offered_and_the_pick_takes_four() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 91}", "cancel"],
            "the enemy Fury Jinx, the Sprite and the friendly Calm Monk are not offered"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: MONK,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(MONK), "four kills the 4-Might Monk");
        assert!(ctx.blob.log.contains(&"{card 91} takes 4".to_string()));
        assert_eq!(ctx.card(DECREE).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_fury_enemy_a_friendly_calm_unit_and_an_empty_pick_are_refused_and_a_gone_target_is_missed()
    {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        for wrong in [
            fixtures::THEIR_UNIT,
            fixtures::SPRITE,
            MY_MONK,
            fixtures::VI,
            fixtures::GROUNDS,
        ] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an enemy Calm unit"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        ctx.bounce(MONK);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.damage_on(MONK), 0);
        assert!(!ctx
            .events
            .iter()
            .any(|event| matches!(event, Event::DamageDealt { .. })));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_counter_played_on_the_decree_does_nothing_and_the_decree_still_resolves() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, MONK);
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, NEGATE).unwrap();
        fixtures::choose(&mut ctx, 1, "{card 90} on the chain").unwrap();
        assert_eq!(ctx.blob.chain.len(), 2);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(
            !ctx.blob.log.contains(&"{card 90} is countered".to_string()),
            "this can't be countered"
        );
        assert!(ctx.blob.log.contains(&"{card 90} resolves".to_string()));
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: MONK,
            n: DAMAGE,
            source: Cause::Item(1)
        }));
        assert!(!ctx.on_board(MONK));
    }
}
