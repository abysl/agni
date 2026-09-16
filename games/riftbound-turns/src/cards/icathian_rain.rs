use super::prelude::{a_unit, card_target, deal, done, play, spell};
use super::{Card, Flow, Item, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const DAMAGE: u8 = 2;
pub const BOLTS: usize = 6;

const A_UNIT: TargetSpec = a_unit("a unit");

fn rain(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    for bolt in 0..BOLTS {
        if let Some(unit) = card_target(ctx, item, bolt) {
            if deal(ctx, item, unit, DAMAGE) {
                ctx.narrate(format!("{{card {unit}}} takes {DAMAGE}"));
            }
        }
    }
    done()
}

pub static CARD: Card = spell("Icathian Rain", &[], &[play(&[A_UNIT; BOLTS], rain)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{Cause, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal;
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const RAIN: u32 = 90;
    const BRUTE: u32 = 91;
    const EXTRA_RUNES: [u32; 4] = [46, 47, 48, 49];

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            domain: vec!["Fury".into(), "Mind".into()],
            ..fixtures::spell(RAIN, fixtures::HAND, 0, "Icathian Rain", 7, 3)
        });
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BASE, 1, "Brute", 5));
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Mind", false));
        }
        fixture.resolve();
        fixture
    }

    fn damage_events(ctx: &Ctx) -> Vec<(u32, u8)> {
        ctx.events
            .iter()
            .filter_map(|event| match event {
                Event::DamageDealt {
                    card,
                    n,
                    source: Cause::Item(1),
                } => Some((*card, *n)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn icathian_rain_has_no_timing_keyword_and_six_separate_unit_choices() {
        assert!(std::ptr::eq(script_of("Icathian Rain").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert_eq!(CARD.abilities[0].targets.len(), BOLTS);
        assert!(CARD.abilities[0]
            .targets
            .iter()
            .all(|spec| (spec.min, spec.max) == (1, 1)));
    }

    #[test]
    fn six_bolts_spread_over_three_units_kill_the_small_and_wound_the_brute() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAIN).unwrap();
        for (spec, unit) in [
            (0, fixtures::SPRITE),
            (1, fixtures::SPRITE),
            (2, fixtures::THEIR_UNIT),
            (3, BRUTE),
            (4, BRUTE),
            (5, fixtures::VI),
        ] {
            assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec }));
            fixtures::choose(&mut ctx, 0, &format!("{{card {unit}}}")).unwrap();
        }
        assert!(ctx.blob.prompt.is_none(), "six choices, six prompts");
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            1,
            "seven energy from the eight ready runes, paid after the choices"
        );
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            damage_events(&ctx),
            [
                (fixtures::SPRITE, DAMAGE),
                (fixtures::SPRITE, DAMAGE),
                (fixtures::THEIR_UNIT, DAMAGE),
                (BRUTE, DAMAGE),
                (BRUTE, DAMAGE),
                (fixtures::VI, DAMAGE),
            ]
        );
        assert!(!ctx.on_board(fixtures::SPRITE), "four on three Might");
        assert!(!ctx.on_board(fixtures::THEIR_UNIT), "two on two Might");
        assert!(ctx.on_board(BRUTE), "four on five Might");
        assert!(ctx.on_board(fixtures::VI), "two on three Might");
        assert_eq!(ctx.damage_on(BRUTE), 4);
        assert_eq!(ctx.card(RAIN).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn it_is_refused_short_of_seven_energy_and_a_gone_target_is_skipped() {
        let mut fixture = armed();
        for rune in [48, 49] {
            fixture.table.card_mut(rune).unwrap().exhausted = true;
        }
        let ctx = fixture.ctx();
        let entry = crate::engine::ctx::EntryMove {
            card: RAIN,
            from: ctx.zones.hand,
            from_seat: 0,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        };
        assert_eq!(
            legal::classify(&ctx, 0, &entry),
            Err(Refusal::NotEnoughRunes {
                needed: 7,
                ready: 6
            })
        );
        drop(ctx);
        let mut paid = armed();
        let mut ctx = paid.ctx();
        fixtures::play_from_hand(&mut ctx, 0, RAIN).unwrap();
        for _ in 0..BOLTS {
            fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        }
        ctx.bounce(fixtures::SPRITE);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            damage_events(&ctx).is_empty(),
            "359.3.e.7 · every bolt is ignored"
        );
        assert_eq!(ctx.card(RAIN).unwrap().zone, Some(fixtures::TRASH));
    }
}
