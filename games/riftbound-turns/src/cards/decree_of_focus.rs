use super::prelude::{a_card, card_target, done, might_this_turn, play, spell};
use super::{Card, Domain, Filter, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const BOOST: i16 = 4;
pub const ENEMY_FURY_UNIT: Filter =
    Filter::And(&[Filter::Unit, Filter::Enemy, Filter::Domain(Domain::Fury)]);
pub const FURY_SPELL: Filter = Filter::And(&[Filter::Spell, Filter::Domain(Domain::Fury)]);
pub const PRESSED_BY_FURY: Filter = Filter::And(&[
    Filter::Unit,
    Filter::Friendly,
    Filter::Or(&[
        Filter::InCombatWith(&ENEMY_FURY_UNIT),
        Filter::ChosenByEnemyItem(&FURY_SPELL),
    ]),
]);

fn run(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, BOOST, None);
    }
    done()
}

pub static CARD: Card = spell(
    "Decree of Focus",
    &[Keyword::Reaction],
    &[play(
        &[a_card(
            PRESSED_BY_FURY,
            "a friendly unit facing a Fury unit or a Fury spell",
        )],
        run,
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{self, a_unit};
    use crate::cards::Trigger;
    use crate::engine::ctx::EntryMove;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::Reason;
    use crate::engine::{legal, priority, prompts};
    use crate::state::{PromptWhy, Showdown};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::{Pick, PickRefusal};

    const DECREE: u32 = 90;
    const ZAP: u32 = 91;

    static ZAP_CARD: Card = prelude::spell(
        "Zap",
        &[Keyword::Reaction],
        &[prelude::play(&[a_unit("a unit")], |_, _, _| {
            prelude::done()
        })],
    );

    fn decree(id: u32, seat: u8) -> agni_plugin_sdk::table::CardInfo {
        let mut card = fixtures::spell(id, fixtures::HAND, seat, "Decree of Focus", 1, 0);
        card.domain = vec!["Calm".into()];
        card
    }

    fn armed() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(decree(DECREE, 0));
        for rune in [47, 48] {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Calm", false));
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

    fn in_combat_at_bf1(fixture: &mut Fixture) {
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().zone = Some(fixtures::BF1);
        fixture.blob.set_holder(fixtures::BF1, Some(1));
        fixture.blob.showdown = Some(Showdown {
            combat: true,
            ..Showdown::open(fixtures::BF1, 0, 1)
        });
        fixture.resolve();
    }

    #[test]
    fn the_script_is_a_reaction_with_one_conditional_friendly_target() {
        assert_eq!(CARD.name, "Decree of Focus");
        assert!(CARD.has_keyword(Keyword::Reaction));
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(ability.targets.len(), 1);
        assert_eq!(ability.targets[0].filter, PRESSED_BY_FURY);
        assert_eq!((ability.targets[0].min, ability.targets[0].max), (1, 1));
    }

    #[test]
    fn a_defender_facing_a_fury_attacker_is_the_only_candidate_and_gains_four_this_turn() {
        let mut fixture = armed();
        in_combat_at_bf1(&mut fixture);
        let mut ctx = fixture.ctx();
        ctx.mark_attacker(fixtures::VI);
        ctx.mark_defender(fixtures::THEIR_UNIT);
        assert!(ctx.in_combat(fixtures::VI));
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "cancel".to_string()],
            "the Fury enemy across the combat makes Vi a candidate; the Sprite at base is not"
        );
        let prompt = ctx.blob.prompt.as_ref().unwrap().id;
        assert_eq!(
            prompts::answer(&mut ctx, 1, Pick { prompt, option: 0 }),
            Err(Refusal::Pick(PickRefusal::NotYourPrompt { seat: 0 }))
        );
        fixtures::choose(&mut ctx, 0, &format!("{{card {}}}", fixtures::VI)).unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.current_might(fixtures::VI), 3 + i32::from(BOOST));
        crate::engine::expiry::at_expiration(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 3, "this turn only");
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn against_a_non_fury_attacker_the_prompt_opens_cancel_only_and_a_cancel_returns_the_card() {
        let mut fixture = armed();
        in_combat_at_bf1(&mut fixture);
        fixture.table.card_mut(fixtures::THEIR_UNIT).unwrap().domain = vec!["Calm".into()];
        fixture.resolve();
        let mut ctx = fixture.ctx();
        ctx.mark_attacker(fixtures::VI);
        ctx.mark_defender(fixtures::THEIR_UNIT);
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["cancel"]);
        assert_eq!(
            crate::engine::play::choose_targets(&mut ctx, 1, 0, &[fixtures::VI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget))
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(ctx.card(DECREE).unwrap().zone, Some(fixtures::HAND));
    }

    #[test]
    fn a_unit_chosen_by_an_enemy_fury_spell_qualifies_and_one_chosen_by_a_calm_spell_does_not() {
        let mut fixture = armed();
        fixture
            .table
            .cards
            .push(fixtures::spell(ZAP, fixtures::HAND, 1, "Zap", 1, 0));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ZAP, &ZAP_CARD);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, ZAP)),
            Err(Refusal::NotYourTurn),
            "a reaction with an empty chain has no window on the other seat's turn"
        );
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, ZAP).unwrap();
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::VI)).unwrap();
        assert_eq!(priority::holder(&ctx), Some(1));
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            [format!("{{card {}}}", fixtures::VI), "cancel".to_string()]
        );
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        let mut calm = armed();
        let mut zap = fixtures::spell(ZAP, fixtures::HAND, 1, "Zap", 1, 0);
        zap.domain = vec!["Calm".into()];
        calm.table.cards.push(zap);
        calm.resolve();
        calm.scripts = calm.scripts.clone().with_script(ZAP, &ZAP_CARD);
        let mut ctx = calm.ctx();
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        fixtures::play_from_hand(&mut ctx, 1, ZAP).unwrap();
        fixtures::choose(&mut ctx, 1, &format!("{{card {}}}", fixtures::VI)).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, DECREE).unwrap();
        assert_eq!(fixtures::labels(&ctx), ["cancel"]);
    }
}
