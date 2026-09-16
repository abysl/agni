use super::prelude::{a_card, bounce, card_target, done, play, spell, stun};
use super::{Card, Cost, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const REPEAT: Cost = Cost {
    energy: 2,
    power: &[],
};

pub const ATTACKING_ENEMY_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::Attacker]);

pub fn dread(ctx: &mut Ctx, unit: u32) -> bool {
    if ctx.is_stunned(unit) {
        ctx.narrate(format!(
            "{{card {unit}}} is already stunned · it returns to its owner's hand instead"
        ));
        return bounce(ctx, unit);
    }
    stun(ctx, unit)
}

fn resolve(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        dread(ctx, unit);
    }
    done()
}

pub static CARD: Card = spell(
    "Existential Dread",
    &[Keyword::Action, Keyword::Repeat(REPEAT)],
    &[play(
        &[a_card(ATTACKING_ENEMY_UNIT, "an attacking enemy unit")],
        resolve,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::play::{self as play_engine, SLOT_REPEAT};
    use crate::state::{PromptWhy, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::table::CardInfo;

    const DREAD: u32 = 90;
    const THEIR_DREAD: u32 = 91;
    const RAIDER: u32 = 92;
    const CHAOS_RUNES: [u32; 2] = [46, 47];

    fn dread_card(id: u32, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Existential Dread", 1, 1);
        card.domain = vec!["Chaos".into()];
        card
    }

    fn raid() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(dread_card(DREAD, 0));
        fixture.table.cards.push(dread_card(THEIR_DREAD, 1));
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture
            .table
            .cards
            .push(fixtures::unit(RAIDER, fixtures::BF1, 1, "Raider", 4));
        fixture.table.card_mut(fixtures::SPRITE).unwrap().zone = Some(fixtures::BF1);
        for rune in CHAOS_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Chaos", false));
        }
        fixture.blob.set_holder(fixtures::BF1, Some(0));
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

    fn attacked(ctx: &mut Ctx) {
        assert!(ctx.mark_attacker(RAIDER));
        assert!(ctx.mark_attacker(fixtures::SPRITE));
        assert!(ctx.mark_defender(fixtures::VI));
    }

    #[test]
    fn the_script_is_a_repeatable_action_over_one_attacking_enemy_unit() {
        assert!(std::ptr::eq(script_of("Existential Dread").unwrap(), &CARD));
        assert!(CARD.has_keyword(Keyword::Action));
        assert!(CARD.has_keyword(Keyword::Repeat(REPEAT)));
        assert_eq!(REPEAT.energy, 2);
        assert!(REPEAT.power.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, ATTACKING_ENEMY_UNIT);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn only_attacking_enemies_are_offered_and_the_chosen_one_is_stunned() {
        let mut fixture = raid();
        let mut ctx = fixture.ctx();
        attacked(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, DREAD).unwrap();
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::OptionalCost {
                item: 1,
                cost: SLOT_REPEAT as u8
            })
        );
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            ["{card 60}", "{card 92}", "cancel"],
            "the defending Vi and the Jinx at rest in their base are no attackers"
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {RAIDER}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(RAIDER)]);
        assert!(!ctx.is_stunned(RAIDER), "nothing until it resolves");
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.is_stunned(RAIDER));
        assert_eq!(ctx.combat_might(RAIDER), 0);
        assert!(ctx.on_board(RAIDER), "a first stun keeps it on the board");
        assert!(ctx.blob.log.contains(&"{card 92} is stunned".to_string()));
        assert_eq!(ctx.card(DREAD).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_unit_already_stunned_returns_to_its_owners_hand_instead() {
        let mut fixture = raid();
        let mut ctx = fixture.ctx();
        attacked(&mut ctx);
        assert!(ctx.stun(RAIDER));
        fixtures::play_from_hand(&mut ctx, 0, DREAD).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {RAIDER}}}")).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.card(RAIDER).unwrap().zone, Some(fixtures::HAND));
        assert_eq!(ctx.card(RAIDER).unwrap().seat, 1, "its owner's hand");
        assert!(ctx.blob.log.contains(
            &"{card 92} is already stunned · it returns to its owner's hand instead".to_string()
        ));
        assert!(ctx
            .blob
            .log
            .contains(&"{card 92} returns to hand".to_string()));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn repeated_on_the_same_attacker_the_second_pass_finds_it_stunned_and_bounces_it() {
        let mut fixture = raid();
        let mut ctx = fixture.ctx();
        attacked(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, DREAD).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["yes", "no", "cancel"]);
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        fixtures::choose(&mut ctx, 0, &format!("{{card {RAIDER}}}")).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 1 }));
        fixtures::choose(&mut ctx, 0, &format!("{{card {RAIDER}}}")).unwrap();
        assert!(ctx.blob.chain[0].repeated());
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            2,
            "three of the five ready runes pay the 1E and the 2E repeat, one of them recycled for the Chaos power"
        );
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.log.contains(&"{card 92} is stunned".to_string()));
        assert!(ctx.blob.log.contains(&"{card 90} repeats".to_string()));
        assert_eq!(
            ctx.card(RAIDER).unwrap().zone,
            Some(fixtures::HAND),
            "820 · the repeat is a second execution and reads the stun the first one applied"
        );
        assert!(ctx.on_board(fixtures::SPRITE));
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn a_token_already_stunned_is_despawned_and_the_bounce_of_a_real_card_drops_its_state() {
        let mut fixture = raid();
        let mut ctx = fixture.ctx();
        attacked(&mut ctx);
        assert!(ctx.stun(fixtures::SPRITE));
        assert!(dread(&mut ctx, fixtures::SPRITE));
        assert!(!ctx.on_board(fixtures::SPRITE), "a token leaves the game");
        assert!(dread(&mut ctx, RAIDER), "a first stun");
        assert!(ctx.is_stunned(RAIDER));
        assert!(dread(&mut ctx, RAIDER), "a second finds it stunned");
        assert_eq!(ctx.card(RAIDER).unwrap().zone, Some(fixtures::HAND));
        assert!(
            !ctx.is_stunned(RAIDER),
            "the stun stays behind on the board"
        );
        assert!(
            !dread(&mut ctx, RAIDER),
            "a card in hand is nothing to dread"
        );
    }

    #[test]
    fn it_waits_for_your_turn_and_refuses_defenders_friendly_attackers_and_units_at_rest() {
        let mut fixture = raid();
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DREAD)),
            Err(Refusal::NotYourTurn)
        );
        attacked(&mut ctx);
        fixtures::play_from_hand(&mut ctx, 0, DREAD).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        for wrong in [fixtures::VI, fixtures::THEIR_UNIT, fixtures::HAND_UNIT] {
            assert_eq!(
                play_engine::choose_targets(&mut ctx, 1, 0, &[wrong]),
                Err(Refusal::Illegal(Reason::NotALegalTarget)),
                "{wrong} is not an attacking enemy unit"
            );
        }
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(DREAD).unwrap().zone, Some(fixtures::HAND));
        let mut quiet = raid();
        let mut ctx = quiet.ctx();
        assert!(ctx.mark_attacker(fixtures::VI));
        fixtures::play_from_hand(&mut ctx, 0, DREAD).unwrap();
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["cancel"],
            "your own attacker is no target: only the way out"
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(DREAD).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
    }
}
