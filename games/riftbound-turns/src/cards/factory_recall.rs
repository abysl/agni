use super::prelude::{bounce, card_target, done, play, spell, target, GEAR};
use super::{Card, Flow, Item, Keyword, Stage, TargetKind, TargetSpec};
use crate::engine::ctx::Ctx;

const RECALLED: TargetSpec = target(GEAR, 1, 1, TargetKind::Card, "a gear to return");

fn recall(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(gear) = card_target(ctx, item, 0) {
        bounce(ctx, gear);
    }
    done()
}

pub static CARD: Card = spell(
    "Factory Recall",
    &[Keyword::Action],
    &[play(&[RECALLED], recall)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{attach_gear, is_attached};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{play as play_engine, priority};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const RECALL: u32 = 90;
    const THEIR_RECALL: u32 = 91;
    const MY_GEAR: u32 = 92;
    const THEIR_GEAR: u32 = 93;
    const THEIR_GOLD: u32 = 94;
    const CHAOS_RUNE: u32 = 100;

    fn recall(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Factory Recall", 1, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(recall(RECALL, 0));
        fixture.table.cards.push(recall(THEIR_RECALL, 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(MY_GEAR, fixtures::BASE, 0, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gear(THEIR_GEAR, fixtures::BASE, 1, "Trinket", 1));
        fixture
            .table
            .cards
            .push(fixtures::gold(THEIR_GOLD, 1, false));
        fixture.table.tokens.push(THEIR_GOLD);
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
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

    fn both_pass(ctx: &mut Ctx) {
        priority::pass(ctx, 0).unwrap();
        priority::pass(ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
    }

    #[test]
    fn the_script_is_an_action_over_one_gear() {
        assert!(std::ptr::eq(script_of("Factory Recall").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(!CARD.has_keyword(Keyword::Reaction));
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
        assert_eq!(ability.targets[0].filter, GEAR);
    }

    #[test]
    fn an_enemy_gear_returns_to_its_owners_hand() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let their_hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, RECALL).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 92}", "{card 93}", "{card 94}", "cancel"],
            "every gear on the board, friendly, enemy or a Gold token"
        );
        fixtures::choose(&mut ctx, 0, "{card 93}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(THEIR_GEAR)]);
        both_pass(&mut ctx);
        let gear = ctx.card(THEIR_GEAR).unwrap();
        assert_eq!(gear.zone, Some(fixtures::HAND));
        assert_eq!(gear.seat, 1, "the owner's hand, not the caster's");
        assert_eq!(ctx.hand_of(1).len(), their_hand + 1);
        assert!(ctx.effects.contains(&Effect::Move {
            card: THEIR_GEAR,
            zone: fixtures::HAND,
            seat: 1,
            index: TOP
        }));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 93} returns to hand".to_string()));
        assert_eq!(ctx.blob.log.last().unwrap(), "{card 90} resolves");
        assert_eq!(ctx.card(RECALL).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    fn an_attached_friendly_gear_is_detached_and_a_gold_token_vanishes() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        attach_gear(&mut ctx, MY_GEAR, fixtures::VI);
        assert!(is_attached(&ctx, MY_GEAR));
        fixtures::play_from_hand(&mut ctx, 0, RECALL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 92}").unwrap();
        both_pass(&mut ctx);
        assert_eq!(ctx.card(MY_GEAR).unwrap().zone, Some(fixtures::HAND));
        assert!(!is_attached(&ctx, MY_GEAR));
        assert!(ctx.state_of(MY_GEAR).is_none());
        drop(ctx);

        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        let their_hand = ctx.hand_of(1).len();
        fixtures::play_from_hand(&mut ctx, 0, RECALL).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 94}").unwrap();
        both_pass(&mut ctx);
        assert!(ctx.effects.contains(&Effect::Despawn { card: THEIR_GOLD }));
        assert!(ctx.card(THEIR_GOLD).is_none());
        assert_eq!(ctx.hand_of(1).len(), their_hand, "a token is not a card");
    }

    #[test]
    fn units_and_cards_off_the_board_are_refused_and_the_action_waits_for_its_turn() {
        let mut fixture = armed();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_RECALL)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, RECALL).unwrap();
        for wrong in [fixtures::VI, fixtures::HAND_GEAR, fixtures::GROUNDS] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not a gear on the board"
            );
        }
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(RECALL).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
