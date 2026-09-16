use super::prelude::{a_friendly_unit, card_target, done, might_this_turn, play, shroud, spell};
use super::{Card, Cost, Flow, Item, Keyword, Stage};
use crate::engine::ctx::Ctx;

pub const MIGHT: i16 = 1;
pub const FLOW: Cost = Cost {
    energy: 2,
    power: &[],
};

fn veil(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(unit) = card_target(ctx, item, 0) {
        might_this_turn(ctx, item, unit, MIGHT, None);
        shroud(ctx, unit);
        ctx.narrate(format!(
            "{{card {unit}}} gets +{MIGHT} and can't be chosen by enemy spells and abilities this turn"
        ));
    }
    done()
}

pub static CARD: Card = spell(
    "Twilight Shroud",
    &[Keyword::Flow(FLOW)],
    &[play(&[a_friendly_unit("a friendly unit to shroud")], veil)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, spell as spell_card};
    use crate::cards::script_of;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{play as play_engine, targets};
    use crate::state::{ChainItem, ItemKind, Leave, Origin, PromptWhy, TargetRef, FLAG_SHROUDED};

    const SHROUD: u32 = 90;
    const STUPEFY: u32 = 91;

    static STUN: Card = spell_card(
        "Stun",
        &[],
        &[play(&[a_unit("a unit")], |_, _, _| Flow::Done)],
    );

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut shroud = fixtures::spell(SHROUD, zone, 0, "Twilight Shroud", 1, 0);
        shroud.domain = vec!["Calm".into()];
        fixture.table.cards.push(shroud);
        fixture
            .table
            .cards
            .push(fixtures::spell(STUPEFY, fixtures::HAND, 1, "Stun", 1, 0));
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(STUPEFY, &STUN);
        fixture
    }

    fn candidates_of(ctx: &Ctx, seat: u8) -> Vec<TargetRef> {
        let item = ChainItem::new(9, ItemKind::Spell { card: STUPEFY }, seat, Origin::Hand);
        targets::candidates(ctx, &item, &STUN.abilities[0].targets[0])
    }

    #[test]
    fn the_shrouded_unit_is_hidden_from_enemy_spells_and_not_from_friendly_ones() {
        assert!(std::ptr::eq(script_of("Twilight Shroud").unwrap(), &CARD));
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, SHROUD).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{card 50}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.current_might(fixtures::VI), 4);
        assert!(ctx.has_flag(fixtures::VI, FLAG_SHROUDED));
        assert!(
            !candidates_of(&ctx, 1).contains(&TargetRef::Card(fixtures::VI)),
            "an enemy spell finds no candidate"
        );
        assert!(
            candidates_of(&ctx, 0).contains(&TargetRef::Card(fixtures::VI)),
            "a friendly one does"
        );
        assert_eq!(ctx.card(SHROUD).unwrap().zone, Some(fixtures::TRASH));
        crate::engine::expiry::clear_stuns(&mut ctx);
        assert!(!ctx.has_flag(fixtures::VI, FLAG_SHROUDED));
    }

    #[test]
    fn from_the_trash_it_is_banished_and_an_enemy_unit_is_refused() {
        let mut fixture = armed(fixtures::TRASH);
        let mut ctx = fixture.ctx();
        play_engine::begin(
            &mut ctx,
            0,
            SHROUD,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        assert_eq!(
            play_engine::choose_targets(&mut ctx, 1, 0, &[fixtures::THEIR_UNIT]),
            Err(crate::Refusal::Illegal(
                crate::engine::legal::Reason::NotALegalTarget
            ))
        );
        fixtures::choose(&mut ctx, 0, "{card 50}").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert_eq!(ctx.banished_of(0), [SHROUD]);
        assert!(ctx.has_flag(fixtures::VI, FLAG_SHROUDED));
    }
}
