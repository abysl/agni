use super::prelude::{a_unit_at_a_battlefield, banish_by, bounce, card_target, done, play, spell};
use super::{Card, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const BANISH_AT_MOST: i32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fate {
    Banished,
    Returned,
}

pub fn fate_of(might: i32) -> Fate {
    if might <= BANISH_AT_MOST {
        Fate::Banished
    } else {
        Fate::Returned
    }
}

fn scatter(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let Some(unit) = card_target(ctx, item, 0) else {
        return done();
    };
    let might = ctx.current_might(unit);
    match fate_of(might) {
        Fate::Banished => {
            banish_by(ctx, unit, item.controller);
        }
        Fate::Returned => {
            ctx.narrate(format!(
                "{{card {unit}}} has {might} Might · too much to banish"
            ));
            bounce(ctx, unit);
        }
    }
    done()
}

pub static CARD: Card = spell(
    "Wind and Ghosts",
    &[Keyword::Action],
    &[play(
        &[a_unit_at_a_battlefield("a unit at a battlefield")],
        scatter,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{might_this_turn, UNIT_AT_BATTLEFIELD};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::play as play_engine;
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const WIND: u32 = 90;
    const BRUTE: u32 = 92;
    const CHAOS_RUNE: u32 = 46;

    fn wind() -> CardInfo {
        CardInfo {
            domain: vec!["Chaos".into()],
            ..fixtures::spell(WIND, fixtures::HAND, 0, "Wind and Ghosts", 3, 1)
        }
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(wind());
        fixture
            .table
            .cards
            .push(fixtures::unit(BRUTE, fixtures::BF1, 1, "Brute", 4));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(WIND).unwrap(), &CARD));
        fixture
    }

    fn cast_at(ctx: &mut Ctx, unit: u32) {
        fixtures::play_from_hand(ctx, 0, WIND).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        fixtures::choose(ctx, 0, &format!("{{card {unit}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(unit)]);
    }

    #[test]
    fn the_script_is_an_action_over_a_unit_at_a_battlefield_split_at_three_might() {
        assert!(std::ptr::eq(script_of("Wind and Ghosts").unwrap(), &CARD));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT_AT_BATTLEFIELD);
        assert_eq!(BANISH_AT_MOST, 3);
        assert_eq!(fate_of(0), Fate::Banished);
        assert_eq!(fate_of(3), Fate::Banished);
        assert_eq!(fate_of(4), Fate::Returned);
        assert_eq!(fate_of(-1), Fate::Banished);
    }

    #[test]
    fn a_three_might_unit_is_banished() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WIND).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "the Sprite and the Brute stand at battlefields; the units in bases do not"
        );
        fixtures::choose(&mut ctx, 0, "{card 60}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!ctx.on_board(fixtures::SPRITE));
        assert!(ctx.events.contains(&Event::Banished {
            card: fixtures::SPRITE,
            owner: 1,
            by: 0,
            token: true
        }));
        assert!(ctx.blob.log.contains(&"{card 60} is banished".to_string()));
        assert_eq!(ctx.card(WIND).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_four_might_unit_returns_to_its_owners_hand_instead() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(1).len();
        cast_at(&mut ctx, BRUTE);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(BRUTE).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(BRUTE).unwrap().seat, 1);
        assert_eq!(ctx.hand_of(1).len(), hand + 1);
        assert!(ctx.banished_of(1).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} has 4 Might · too much to banish".to_string()));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} returns to hand".to_string()));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_might_is_read_as_it_resolves_so_a_response_changes_the_fate() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        cast_at(&mut ctx, fixtures::SPRITE);
        let spell = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &spell, fixtures::SPRITE, 1, None);
        fixtures::pass_until_open(&mut ctx);
        assert!(
            !ctx.on_board(fixtures::SPRITE),
            "a pumped Sprite is a token returned to hand, so it despawns rather than banishes"
        );
        assert!(ctx.banished_of(1).is_empty());
        assert!(ctx
            .blob
            .log
            .contains(&"{card 60} has 4 Might · too much to banish".to_string()));
        let mut shrunk = armed();
        let mut ctx = shrunk.ctx();
        cast_at(&mut ctx, BRUTE);
        let spell = ctx.blob.chain[0].clone();
        might_this_turn(&mut ctx, &spell, BRUTE, -1, None);
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.banished_of(1), [BRUTE], "a shrunk Brute is banished");
    }

    #[test]
    fn a_unit_in_a_base_is_refused_and_a_target_that_left_is_untouched() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, WIND).unwrap();
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is in a base or not a unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        ctx.recall(BRUTE, false);
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.on_board(BRUTE), "recalled to base, it is out of reach");
        assert!(ctx.banished_of(1).is_empty());
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("returns to hand") || line.ends_with("is banished")));
        assert!(ctx.fault.is_none());
    }
}
