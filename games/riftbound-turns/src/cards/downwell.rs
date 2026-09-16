use super::prelude::{bounce, done, friendly_gear, play, spell, units_on_board};
use super::{Card, Flow, Item, Stage};
use crate::engine::ctx::Ctx;

pub fn everything_on_the_board(ctx: &Ctx) -> Vec<u32> {
    let mut cards = units_on_board(ctx);
    for seat in 0..ctx.players() {
        cards.extend(friendly_gear(ctx, seat));
    }
    cards.sort_unstable();
    cards.dedup();
    cards
}

fn flood(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let cards = everything_on_the_board(ctx);
    let returned = cards.into_iter().filter(|card| bounce(ctx, *card)).count();
    let source = item.kind.source();
    ctx.narrate(format!(
        "{{card {source}}} returns {returned} units and gear to their owners' hands"
    ));
    done()
}

pub static CARD: Card = spell("Downwell", &[], &[play(&[], flood)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attach_gear, is_attached};
    use crate::cards::{script_of, Keyword, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{legal, priority};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const DOWNWELL: u32 = 90;
    const THEIR_DOWNWELL: u32 = 91;
    const MY_GEAR: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const MY_GOLD: u32 = 94;
    const FIRST_EXTRA_RUNE: u32 = 100;

    fn downwell(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Downwell", 8, 2);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(downwell(DOWNWELL, 0));
        fixture.table.cards.push(downwell(THEIR_DOWNWELL, 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Trinket", 1));
        fixture.table.cards.push(fixtures::gold(MY_GOLD, 0, true));
        fixture.table.tokens.push(MY_GOLD);
        for rune in FIRST_EXTRA_RUNE..FIRST_EXTRA_RUNE + 8 {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
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
    fn the_script_is_a_targetless_sorcery_that_sees_every_unit_and_gear() {
        assert!(std::ptr::eq(script_of("Downwell").unwrap(), &CARD));
        assert!(!CARD.has_keyword(Keyword::Action));
        assert_eq!(CARD.abilities.len(), 1);
        assert_eq!(CARD.abilities[0].trigger, Trigger::Play);
        assert!(CARD.abilities[0].targets.is_empty());
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            everything_on_the_board(&ctx),
            [
                fixtures::VI,
                fixtures::SPRITE,
                fixtures::THEIR_UNIT,
                MY_GEAR,
                THEIR_GEAR,
                MY_GOLD
            ],
            "units then gear, tokens included, battlefields and runes excluded"
        );
    }

    #[test]
    fn every_unit_and_gear_returns_to_its_owners_hand_and_tokens_vanish() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, MY_GEAR, fixtures::VI);
        let my_hand = ctx.hand_of(0).len();
        let their_hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, DOWNWELL).unwrap();
        assert!(ctx.blob.prompt.is_none(), "355.10.d · nothing is targeted");
        assert!(
            ctx.on_board(fixtures::VI),
            "nothing happens before it resolves"
        );
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        for (card, seat) in [
            (fixtures::VI, 0),
            (fixtures::THEIR_UNIT, 1),
            (MY_GEAR, 0),
            (THEIR_GEAR, 1),
        ] {
            let held = ctx.card(card).unwrap();
            assert_eq!(held.zone, Some(fixtures::HAND), "{card}");
            assert_eq!(held.seat, seat, "{card} goes to its owner's hand");
        }
        for token in [fixtures::SPRITE, MY_GOLD] {
            assert!(ctx.effects.contains(&Effect::Despawn { card: token }));
            assert!(ctx.card(token).is_none());
        }
        assert!(!is_attached(&ctx, MY_GEAR));
        assert_eq!(
            ctx.hand_of(0).len(),
            my_hand - 1 + 2,
            "Vi and the Trinket, Downwell gone"
        );
        assert_eq!(
            ctx.hand_of(1).len(),
            their_hand + 2,
            "Jinx and their Trinket"
        );
        assert!(
            ctx.on_board(fixtures::GROUNDS) && ctx.on_board(fixtures::ROCKFALL),
            "battlefields stay"
        );
        assert_eq!(
            ctx.runes_of(0).len(),
            10,
            "runes are not gear · two were recycled for the power"
        );
        assert_eq!(
            ctx.blob.holder(fixtures::BF2),
            None,
            "an emptied battlefield falls open"
        );
        assert!(ctx
            .blob
            .log
            .contains(&"{card 90} returns 6 units and gear to their owners' hands".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(DOWNWELL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn downwell_needs_eight_energy_and_two_chaos_and_the_turn() {
        let mut fixture = armed();
        let ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DOWNWELL)),
            Err(Refusal::NotYourTurn)
        );
        drop(ctx);
        let mut poor = armed();
        poor.table
            .cards
            .retain(|card| !(FIRST_EXTRA_RUNE + 4..FIRST_EXTRA_RUNE + 8).contains(&card.id));
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, DOWNWELL)),
            Err(Refusal::NotEnoughRunes {
                needed: 8,
                ready: 7
            }),
            "seven ready runes cannot pay eight energy"
        );
    }
}
