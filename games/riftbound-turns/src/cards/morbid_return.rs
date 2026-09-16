use super::prelude::{a_card, card_target, done, play, spell};
use super::{Card, Filter, Flow, Item, Keyword, Stage, KIND_UNIT};
use crate::engine::ctx::Ctx;
use agni_plugin_sdk::decide::{Effect, TOP};

pub const UNIT_IN_YOUR_TRASH: Filter =
    Filter::And(&[Filter::Kind(KIND_UNIT), Filter::InTrash, Filter::Friendly]);

pub fn return_from_trash(ctx: &mut Ctx, card: u32) -> bool {
    if !ctx.in_trash(card) {
        return false;
    }
    let Some(hand) = ctx.zones.hand else {
        return false;
    };
    let owner = ctx.owner(card);
    ctx.emit(Effect::Move {
        card,
        zone: hand,
        seat: owner,
        index: TOP,
    });
    ctx.narrate(format!("{{card {card}}} returns from the trash to hand"));
    true
}

fn morbid(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        return_from_trash(ctx, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Morbid Return",
    &[Keyword::Action],
    &[play(
        &[a_card(UNIT_IN_YOUR_TRASH, "a unit in your trash to return")],
        morbid,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{TargetKind, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{play as play_engine, priority, prompts};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const MORBID: u32 = 90;
    const FALLEN: u32 = 91;
    const THEIR_FALLEN: u32 = 92;
    const TRASHED_SPELL: u32 = 93;
    const CHAOS_RUNE: u32 = 46;

    fn morbid_card(seat: u8) -> CardInfo {
        let mut card = fixtures::spell(MORBID, fixtures::HAND, seat, "Morbid Return", 2, 0);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn graveyard() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(morbid_card(0));
        fixture
            .table
            .cards
            .push(fixtures::unit(FALLEN, fixtures::TRASH, 0, "Fallen", 3));
        fixture.table.cards.push(fixtures::unit(
            THEIR_FALLEN,
            fixtures::TRASH,
            1,
            "Fallen",
            3,
        ));
        fixture.table.cards.push(fixtures::spell(
            TRASHED_SPELL,
            fixtures::TRASH,
            0,
            "Spent",
            1,
            0,
        ));
        fixture
            .table
            .cards
            .push(fixtures::rune(CHAOS_RUNE, 0, "Chaos", false));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(MORBID).unwrap(),
            &CARD
        ));
        fixture
    }

    #[test]
    fn the_script_is_an_action_over_one_friendly_unit_in_the_trash() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Morbid Return").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.keywords, [Keyword::Action]);
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, UNIT_IN_YOUR_TRASH);
        assert_eq!(ability.targets[0].kind, TargetKind::Card);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn morbid_return_moves_the_chosen_unit_from_the_trash_to_its_owners_hand() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        let hand = ctx.hand_of(0).len();
        fixtures::play_from_hand(&mut ctx, 0, MORBID).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 91}", "cancel"],
            "355.10: only your own trash, and only units in it"
        );
        assert_eq!(
            prompts::status(&ctx, PromptWhy::Target { item: 1, spec: 0 }),
            "{card 90}: choose a unit in your trash to return (0 of 1)"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[THEIR_FALLEN]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "the other seat's trash is out of reach"
        );
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[TRASHED_SPELL]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "a spell is not a unit"
        );
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(FALLEN)]);
        assert_eq!(ctx.card(FALLEN).unwrap().zone, Some(fixtures::TRASH));
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(FALLEN).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.hand_of(0).contains(&FALLEN));
        assert!(ctx.effects.contains(&Effect::Move {
            card: FALLEN,
            zone: fixtures::HAND,
            seat: 0,
            index: TOP
        }));
        assert_eq!(ctx.hand_of(0).len(), hand, "one played, one returned");
        assert!(ctx
            .blob
            .log
            .contains(&"{card 91} returns from the trash to hand".to_string()));
        assert_eq!(ctx.card(MORBID).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn a_unit_gone_from_the_trash_before_resolution_stays_where_it_went() {
        let mut fixture = graveyard();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MORBID).unwrap();
        fixtures::choose(&mut ctx, 0, "{card 91}").unwrap();
        ctx.banish(FALLEN);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(FALLEN).unwrap().zone, Some(fixtures::BANISHMENT));
        assert!(!ctx.hand_of(0).contains(&FALLEN));
        assert!(!ctx
            .blob
            .log
            .iter()
            .any(|line| line.ends_with("returns from the trash to hand")));
    }

    #[test]
    fn with_an_empty_trash_the_play_can_only_be_taken_back() {
        let mut fixture = graveyard();
        fixture.table.cards.retain(|card| card.id != FALLEN);
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, MORBID).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["cancel"]);
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(MORBID).unwrap().zone, Some(fixtures::HAND));
        assert!(
            ctx.effects.iter().all(|effect| !matches!(
                effect,
                Effect::Annotate { key, .. } if key == "exhausted"
            )),
            "a cancelled play paid nothing: {:?}",
            ctx.effects
        );
    }
}
