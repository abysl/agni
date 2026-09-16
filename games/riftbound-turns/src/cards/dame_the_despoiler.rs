use super::prelude::{
    a_card, card_target, done, empower, might_this_turn, on_attack, on_defend, unit, when,
    when_empowered, UNIT_HERE,
};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage, TargetSpec};
use crate::engine::ctx::Ctx;

pub const EMPOWER: Cost = Cost {
    energy: 5,
    power: &[Power::Domain(Domain::Body)],
};
pub const BONUS: i16 = 1;
pub const TARGET: TargetSpec = a_card(UNIT_HERE, "a unit here whose Might I rise to");

pub fn rise_to(ctx: &Ctx, me: u32, unit: u32) -> i16 {
    let gap = ctx.current_might(unit) - ctx.current_might(me);
    i16::try_from(gap.max(0)).unwrap_or(i16::MAX)
}

fn despoil(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let me = item.kind.source();
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    if !ctx.on_board(me) {
        return done();
    }
    let rise = rise_to(ctx, me, unit);
    if rise > 0 {
        might_this_turn(ctx, item, me, rise, None);
        ctx.narrate(format!(
            "{{card {me}}} rises to the {} Might of {{card {unit}}} this turn",
            ctx.current_might(unit)
        ));
    }
    might_this_turn(ctx, item, me, BONUS, None);
    ctx.narrate(format!("{{card {me}}} gets +{BONUS} Might this turn"));
    done()
}

pub static CARD: Card = unit(
    "Dame the Despoiler",
    &[Keyword::Empower(EMPOWER)],
    &[
        empower(EMPOWER),
        when(on_attack(&[TARGET], despoil), when_empowered),
        when(on_defend(&[TARGET], despoil), when_empowered),
    ],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::this_turn;
    use crate::cards::{script_of, SelfCost, Timing, Trigger, Who};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{activate, priority, settle};
    use crate::state::{ItemKind, PromptWhy, TargetRef};
    use agni_plugin_sdk::table::CardInfo;

    const DAME: u32 = 90;
    const BRUTE: u32 = 91;
    const RUNT: u32 = 92;
    const MIGHT: u8 = 5;
    const EXTRA_RUNES: [u32; 3] = [100, 101, 102];

    fn dame() -> CardInfo {
        CardInfo {
            energy: Some(5),
            domain: vec!["Body".into()],
            ..fixtures::unit(DAME, fixtures::BF1, 0, "Dame the Despoiler", MIGHT)
        }
    }

    fn raid(when_empowered: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dame());
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 8));
        fixture
            .table
            .cards
            .push(fixtures::unit(RUNT, fixtures::BF1, 0, "Runt", 1));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(DAME).unwrap(), &CARD));
        if when_empowered {
            let mut ctx = fixture.ctx();
            activate::activate(&mut ctx, 0, DAME, 0).unwrap();
            priority::pass(&mut ctx, 0).unwrap();
            priority::pass(&mut ctx, 1).unwrap();
            assert!(ctx.is_empowered(DAME));
            let table = ctx.table.clone();
            drop(ctx);
            fixture.commit(table);
        }
        fixture
    }

    #[test]
    fn the_script_prints_empower_with_the_empowered_gated_attack_and_defend_triggers() {
        assert!(std::ptr::eq(
            script_of("Dame the Despoiler").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Empower(EMPOWER)]);
        assert_eq!(CARD.empower_cost(), Some(EMPOWER));
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 3);
        let empower = &CARD.abilities[0];
        assert_eq!(empower.trigger, Trigger::Activated(Timing::Sorcery));
        assert_eq!(empower.cost, Some(EMPOWER));
        assert_eq!(empower.self_cost, SelfCost::Free);
        assert_eq!(empower.label, Some("empower"));
        let attack = &CARD.abilities[1];
        assert_eq!(attack.trigger, Trigger::Attacks(Who::Me));
        let defend = &CARD.abilities[2];
        assert_eq!(defend.trigger, Trigger::Defends(Who::Me));
        for ability in [attack, defend] {
            assert!(!ability.optional);
            assert!(ability.condition.is_some());
            assert_eq!(ability.targets, &[TARGET]);
            assert_eq!(ability.targets[0].filter, UNIT_HERE);
        }
        assert_eq!(BONUS, 1);
    }

    #[test]
    fn empowered_and_attacking_she_rises_to_the_chosen_units_might_then_gets_one_more_for_the_turn()
    {
        let mut fixture = raid(true);
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(DAME));
        settle(&mut ctx).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 2, spec: 0 }));
        let mut offered = fixtures::labels(&ctx);
        offered.sort();
        assert_eq!(
            offered,
            [
                format!("{{card {}}}", fixtures::THEIR_UNIT),
                format!("{{card {DAME}}}"),
                format!("{{card {BRUTE}}}"),
                format!("{{card {RUNT}}}"),
            ],
            "every unit here, herself included · the Sprite elsewhere is not"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 1 } if source == DAME
        ));
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(BRUTE)]);
        assert_eq!(ctx.current_might(DAME), 5, "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(DAME), 9, "8 then +1");
        let mods = &ctx.state_of(DAME).unwrap().might;
        assert_eq!(
            mods.iter().map(|held| held.delta).collect::<Vec<i16>>(),
            [3, 1],
            "the rise and the bonus are two this-turn deltas"
        );
        assert!(ctx.blob.log.contains(&format!(
            "{{card {DAME}}} rises to the 8 Might of {{card {BRUTE}}} this turn"
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {DAME}}} gets +1 Might this turn")));
        assert_eq!(ctx.current_might(BRUTE), 8, "the chosen unit is untouched");
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert_eq!(ctx.current_might(DAME), 5);
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_weaker_pick_or_herself_is_only_the_plus_one_and_defending_reads_the_same() {
        let mut fixture = raid(true);
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_defender(DAME));
        settle(&mut ctx).unwrap();
        assert!(matches!(
            ctx.blob.why,
            Some(PromptWhy::Target { spec: 0, .. })
        ));
        fixtures::choose(&mut ctx, 0, &format!("{{card {RUNT}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 2 } if source == DAME
        ));
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(
            ctx.current_might(DAME),
            6,
            "an increase to 1 is no increase · only the +1"
        );
        let mods = &ctx.state_of(DAME).unwrap().might;
        assert_eq!(
            mods.iter().map(|held| held.delta).collect::<Vec<i16>>(),
            [1]
        );
        assert!(!ctx.blob.log.iter().any(|line| line.contains("rises to")));
        assert_eq!(rise_to(&ctx, DAME, DAME), 0, "her own Might is no rise");
        assert_eq!(rise_to(&ctx, DAME, BRUTE), 2, "8 against her 6 now");
    }

    #[test]
    fn not_empowered_she_triggers_on_neither_attack_nor_defense_and_a_lost_pick_gives_nothing() {
        let mut fixture = raid(false);
        let mut ctx = fixture.ctx();
        assert!(!ctx.is_empowered(DAME));
        assert!(ctx.mark_attacker(DAME));
        settle(&mut ctx).unwrap();
        assert!(
            ctx.blob.chain.is_empty(),
            "828.1.c · no Empowered, no ability"
        );
        assert!(ctx.blob.prompt.is_none());
        ctx.clear_designation(DAME);
        assert!(ctx.mark_defender(DAME));
        settle(&mut ctx).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(DAME), 5);
        drop(ctx);

        let mut fixture = raid(true);
        let mut ctx = fixture.ctx();
        assert!(ctx.mark_attacker(DAME));
        settle(&mut ctx).unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        ctx.table.card_mut(BRUTE).unwrap().zone = Some(fixtures::BASE);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            ctx.current_might(DAME),
            5,
            "the chosen unit left the battlefield · not even the +1"
        );
    }
}
