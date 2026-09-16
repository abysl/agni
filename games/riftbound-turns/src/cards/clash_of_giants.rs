use super::prelude::{a_unit, another_unit, card_target, deal, done, play, spell};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub fn might_as_damage(ctx: &Ctx, unit: u32) -> u8 {
    u8::try_from(ctx.current_might(unit).max(0)).unwrap_or(u8::MAX)
}

fn clash(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let first = card_target(ctx, item, 0).filter(|unit| ctx.on_board(*unit));
    let second = card_target(ctx, item, 1).filter(|unit| ctx.on_board(*unit));
    let (Some(first), Some(second)) = (first, second) else {
        return done();
    };
    if first == second {
        return done();
    }
    let to_second = might_as_damage(ctx, first);
    let to_first = might_as_damage(ctx, second);
    ctx.narrate(format!(
        "{{card {first}}} and {{card {second}}} deal {to_second} and {to_first} to each other"
    ));
    deal(ctx, item, second, to_second);
    deal(ctx, item, first, to_first);
    done()
}

pub static CARD: Card = spell(
    "Clash of Giants",
    &[],
    &[play(
        &[a_unit("a unit"), another_unit("another unit")],
        clash,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{might_this_turn, ANOTHER_UNIT, UNIT};
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::{Cause, EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority};
    use crate::state::PromptWhy;
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const CLASH: u32 = 90;
    const THEIR_CLASH: u32 = 91;
    const BRUTE: u32 = 92;
    const MY_EXTRA: [u32; 3] = [100, 101, 102];

    fn clash_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Clash of Giants", 6, 2);
        card.domain = vec!["Body".into()];
        card
    }

    fn arena() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(clash_card(CLASH, 0));
        fixture.table.cards.push(clash_card(THEIR_CLASH, 1));
        for rune in MY_EXTRA {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Body", false));
        }
        fixture.table.card_mut(fixtures::RUNE_A).unwrap().exhausted = false;
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 5));
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

    fn cast_at(ctx: &mut Ctx, first: u32, second: u32) {
        fixtures::play_from_hand(ctx, 0, CLASH).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {first}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(ctx, 0, &format!("{{card {second}}}")).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1);
    }

    #[test]
    fn the_script_is_a_sorcery_choosing_any_unit_then_another() {
        assert!(std::ptr::eq(script_of("Clash of Giants").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 2);
        assert_eq!(ability.targets[0].filter, UNIT);
        assert_eq!(ability.targets[1].filter, ANOTHER_UNIT);
        assert!(ability
            .targets
            .iter()
            .all(|spec| (spec.min, spec.max) == (1, 1)));
    }

    #[test]
    fn the_two_deal_their_current_mights_to_each_other_anywhere_on_the_board() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, CLASH).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 50}", "{card 60}", "{card 81}", "{card 92}", "cancel"],
            "any unit, in a base or at a battlefield, mine or theirs"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 81}", "{card 92}", "cancel"],
            "the first pick is not another unit"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        let spell = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &spell, fixtures::VI, 3, None);
        assert!(damage_events(&ctx).is_empty(), "nothing before it resolves");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(
            damage_events(&ctx),
            [(BRUTE, 6), (fixtures::VI, 5)],
            "Vi at 3 + 3 deals 6, the Brute deals its 5 back"
        );
        assert!(!ctx.on_board(BRUTE), "six kills the 5-Might Brute");
        assert_eq!(ctx.damage_on(fixtures::VI), 5);
        assert!(ctx.on_board(fixtures::VI), "5 damage on 6 Might survives");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 50} and {card 92} deal 6 and 5 to each other".to_string()));
        assert_eq!(ctx.card(CLASH).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn two_enemy_units_may_be_pitted_against_each_other_and_a_zero_might_unit_deals_nothing() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, fixtures::THEIR_UNIT, fixtures::SPRITE);
        let spell = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &spell, fixtures::THEIR_UNIT, -2, None);
        assert_eq!(ctx.current_might(fixtures::THEIR_UNIT), 0);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(
            damage_events(&ctx),
            [(fixtures::THEIR_UNIT, 3)],
            "the Sprite deals its 3; a 0-Might Jinx deals nothing at all"
        );
        assert!(!ctx.on_board(fixtures::THEIR_UNIT));
        assert_eq!(ctx.damage_on(fixtures::SPRITE), 0);
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn when_either_unit_has_left_the_board_nobody_is_dealt_anything() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, fixtures::VI, BRUTE);
        ctx.table
            .apply_entry(&fixtures::move_action(BRUTE, fixtures::HAND, 1), 1)
            .unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(damage_events(&ctx).is_empty());
        assert_eq!(ctx.damage_on(fixtures::VI), 0);
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.contains("to each other")));
        assert_eq!(ctx.card(CLASH).unwrap().zone, Some(fixtures::TRASH));
    }

    #[test]
    fn the_same_unit_twice_and_non_units_are_refused_and_the_other_seat_waits_for_its_turn() {
        let mut fixture = arena();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_CLASH)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, CLASH).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::GROUNDS]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a battlefield is not a unit"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTE}}}")).unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[BRUTE]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the other unit is another unit"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 1, &[fixtures::HAND_UNIT]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a unit in hand is not on the board"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(CLASH).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.is_neutral_open());
    }
}
