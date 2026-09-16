use super::prelude::{unit, with_statics, MIGHTY};
use super::{Card, Grant, Keyword, Static};
use crate::engine::ctx::Ctx;

pub static MIGHTY_KEYWORDS: &[Grant] = &[
    Grant::Keyword(Keyword::Deflect(1)),
    Grant::Keyword(Keyword::Ganking),
    Grant::Keyword(Keyword::Shield(1)),
];

fn granted_bonus(ctx: &Ctx, card: u32, pick: fn(Keyword) -> Option<u8>) -> i32 {
    ctx.state_of(card)
        .map(|row| {
            row.granted
                .iter()
                .filter_map(|(keyword, _)| pick(*keyword))
                .map(i32::from)
                .sum()
        })
        .unwrap_or(0)
}

pub fn might_before_my_keywords(ctx: &Ctx, card: u32) -> i32 {
    let stored: i32 = ctx
        .state_of(card)
        .map(|row| row.might.iter().map(|held| i32::from(held.delta)).sum())
        .unwrap_or(0);
    let mut total = ctx.printed_might(card) + i32::from(ctx.is_buffed(card)) + stored;
    if ctx.is_attacker(card) {
        total += granted_bonus(ctx, card, |keyword| match keyword {
            Keyword::Assault(n) => Some(n),
            _ => None,
        });
    }
    if ctx.is_defender(card) {
        total += granted_bonus(ctx, card, |keyword| match keyword {
            Keyword::Shield(n) => Some(n),
            _ => None,
        });
    }
    total.max(0)
}

fn mighty(ctx: &Ctx, card: u32) -> bool {
    might_before_my_keywords(ctx, card) >= MIGHTY
}

pub static CARD: Card = with_statics(
    unit("Fiora - Victorious", &[], &[]),
    &[Static::While(mighty, MIGHTY_KEYWORDS)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{grant_this_turn, legend, this_turn, with_statics as statics_of};
    use crate::cards::{script_of, Scope};
    use crate::engine::ctx::{Location, COUNTER_BUFFED};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{march, statics};
    use crate::state::{ChainItem, ItemKind, Origin};
    use crate::Refusal;
    use agni_plugin_sdk::decide::Effect;
    use agni_plugin_sdk::table::Target;

    const FIORA: u32 = 90;

    fn afield() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            FIORA,
            fixtures::BF1,
            0,
            "Fiora - Victorious",
            4,
        ));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(FIORA).unwrap(), &CARD));
        fixture
    }

    fn keywords(ctx: &Ctx, card: u32) -> Vec<Keyword> {
        statics::grants_on(ctx, card)
            .into_iter()
            .filter_map(|grant| match grant {
                Grant::Keyword(keyword) => Some(keyword),
                _ => None,
            })
            .collect()
    }

    fn across(ctx: &Ctx) -> Result<(), Refusal> {
        march::legal_destination(
            ctx,
            FIORA,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
    }

    fn unbuff(ctx: &mut Ctx, card: u32) {
        ctx.emit(Effect::Counter {
            target: Target::Card(card),
            counter: COUNTER_BUFFED,
            delta: -1,
        });
    }

    #[test]
    fn the_script_is_a_unit_whose_while_grants_three_keywords_at_five_might() {
        assert!(std::ptr::eq(
            script_of("Fiora - Victorious").unwrap(),
            &CARD
        ));
        assert!(CARD.abilities.is_empty());
        assert!(
            CARD.keywords.is_empty(),
            "the keywords are hers only while Mighty"
        );
        assert!(matches!(
            CARD.statics,
            [Static::While(
                _,
                [
                    Grant::Keyword(Keyword::Deflect(1)),
                    Grant::Keyword(Keyword::Ganking),
                    Grant::Keyword(Keyword::Shield(1))
                ]
            )]
        ));
    }

    #[test]
    fn a_buff_makes_her_mighty_and_defending_she_reads_six_with_the_keywords() {
        let mut fixture = afield();
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(FIORA), 4);
        assert!(keywords(&ctx, FIORA).is_empty());
        assert_eq!(ctx.deflect_of(FIORA), 0);
        assert_eq!(
            across(&ctx),
            Err(Refusal::Illegal(Reason::NeedsGanking)),
            "at 4 she has no Ganking"
        );
        assert!(ctx.buff(FIORA));
        assert_eq!(
            keywords(&ctx, FIORA),
            [Keyword::Deflect(1), Keyword::Ganking, Keyword::Shield(1)],
            "708 · Mighty at 5"
        );
        assert_eq!(ctx.current_might(FIORA), 5);
        assert_eq!(ctx.deflect_of(FIORA), 1);
        assert_eq!(across(&ctx), Ok(()));
        assert!(ctx.mark_defender(FIORA));
        assert_eq!(
            ctx.current_might(FIORA),
            6,
            "476 · the buff makes her Mighty, Shield adds one more as a defender"
        );
        unbuff(&mut ctx, FIORA);
        assert_eq!(
            ctx.current_might(FIORA),
            4,
            "476.3 example · the buff gone, neither it nor Shield applies"
        );
        assert!(keywords(&ctx, FIORA).is_empty());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_this_turn_might_or_a_granted_shield_while_defending_counts_toward_mighty() {
        let mut fixture = afield();
        let mut ctx = fixture.ctx();
        let spell = ChainItem::new(
            7,
            ItemKind::Spell {
                card: fixtures::HAND_SPELL,
            },
            0,
            Origin::Hand,
        );
        let until = this_turn(&ctx);
        ctx.might(FIORA, 1, until, None, spell.id);
        assert_eq!(might_before_my_keywords(&ctx, FIORA), 5);
        assert_eq!(
            keywords(&ctx, FIORA),
            [Keyword::Deflect(1), Keyword::Ganking, Keyword::Shield(1)]
        );
        ctx.expire(until);
        assert!(keywords(&ctx, FIORA).is_empty());
        assert!(grant_this_turn(&mut ctx, FIORA, Keyword::Shield(1)));
        assert!(
            keywords(&ctx, FIORA).is_empty(),
            "a granted Shield is Might only while she defends"
        );
        assert!(ctx.mark_defender(FIORA));
        assert_eq!(might_before_my_keywords(&ctx, FIORA), 5);
        assert_eq!(
            ctx.current_might(FIORA),
            6,
            "the granted Shield's one and her own Shield's one"
        );
        assert_eq!(ctx.deflect_of(FIORA), 1);
        ctx.clear_designation(FIORA);
        assert_eq!(ctx.current_might(FIORA), 4);
        assert_eq!(ctx.deflect_of(FIORA), 0);
    }

    fn always(_: &Ctx, _: u32, _: u32) -> bool {
        true
    }

    static RALLYING: Card = statics_of(
        legend("Rallying", &[], &[]),
        &[Static::Aura {
            scope: Scope::FriendlyUnits,
            when: always,
            grants: &[Grant::Might(1)],
        }],
    );

    #[test]
    #[ignore = "engine gap · might_before_my_keywords reads no aura Might, the engine owes own_grants a re-entrant Might reading that excludes the querying static (476)"]
    fn a_plus_one_aura_from_a_legend_makes_her_mighty() {
        let mut fixture = afield();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::LEGEND_CARD, &RALLYING);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.current_might(FIORA), 5);
        assert_eq!(
            keywords(&ctx, FIORA),
            [Keyword::Deflect(1), Keyword::Ganking, Keyword::Shield(1)]
        );
        assert!(ctx.mark_defender(FIORA));
        assert_eq!(ctx.current_might(FIORA), 6);
    }
}
