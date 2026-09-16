use super::ferrous_forerunner::play_mechs;
use super::prelude::{a_play_location, done, play, spell, zone_target, Location};
use super::{Card, Cost, Domain, Flow, Item, Keyword, Power, Stage};
use crate::engine::ctx::Ctx;

pub const MECHS: usize = 1;
pub const FLOW: Cost = Cost {
    energy: 2,
    power: &[Power::Domain(Domain::Mind)],
};

fn design(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    let seat = item.controller;
    let at = zone_target(item, 0)
        .and_then(|zone| Location::of_zone(zone, seat, &ctx.zones))
        .unwrap_or(Location::Base(seat));
    play_mechs(ctx, seat, at, MECHS);
    done()
}

pub static CARD: Card = spell(
    "Iterative Design",
    &[Keyword::Flow(FLOW)],
    &[play(&[a_play_location("where the Mech is played")], design)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::ferrous_forerunner::{tests::mechs_of, MECH_MIGHT};
    use crate::cards::rumble_mechanized_menace::{is_mech, MECH_TOKEN};
    use crate::cards::{script_of, Trigger};
    use crate::engine::ctx::{EntryMove, Event};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Intent};
    use crate::engine::play as play_engine;
    use crate::engine::settle;
    use crate::state::{Leave, Origin, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::decide::{Effect, TOP};
    use agni_plugin_sdk::table::CardInfo;

    const DESIGN: u32 = 90;
    const THEIR_DESIGN: u32 = 91;
    const MIND_RUNE: u32 = 46;

    fn design_card(id: u32, zone: u16, seat: u8) -> CardInfo {
        let mut card = fixtures::spell(id, zone, seat, "Iterative Design", 2, 0);
        card.domain = vec!["Mind".into()];
        card
    }

    fn armed(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(design_card(DESIGN, zone, 0));
        fixture.table.cards.push(design_card(THEIR_DESIGN, zone, 1));
        fixture
            .table
            .cards
            .push(fixtures::rune(MIND_RUNE, 0, "Mind", false));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        fixture
    }

    fn drag(ctx: &Ctx, seat: u8, card: u32, from: u16) -> EntryMove {
        EntryMove {
            card,
            from: Some(from),
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_a_flow_spell_that_asks_where_the_mech_is_played() {
        assert!(std::ptr::eq(script_of("Iterative Design").unwrap(), &CARD));
        assert_eq!(CARD.name, "Iterative Design");
        assert_eq!(CARD.keywords, [Keyword::Flow(FLOW)]);
        assert_eq!(CARD.flow_cost(), Some(FLOW));
        assert_eq!(
            (FLOW.energy, FLOW.power),
            (2, &[Power::Domain(Domain::Mind)][..])
        );
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Play);
        assert_eq!(
            ability.targets,
            &[a_play_location("where the Mech is played")]
        );
        assert!(ability.candidates.is_none());
        assert_eq!(MECHS, 1);
        assert_eq!(MECH_MIGHT, 3, "a 3 might Mech");
    }

    #[test]
    fn one_exhausted_three_might_mech_enters_at_the_chosen_location() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, DESIGN).unwrap();
        assert_eq!(ctx.blob.why, Some(PromptWhy::Target { item: 1, spec: 0 }));
        assert_eq!(fixtures::labels(&ctx), ["{zone 8}", "{zone 9}", "cancel"]);
        fixtures::choose(&mut ctx, 0, "{zone 9}").unwrap();
        assert!(mechs_of(&ctx, 0).is_empty(), "nothing until it resolves");
        let next = ctx.table.next_id;
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        let mechs = mechs_of(&ctx, 0);
        assert_eq!(mechs, [next]);
        let mech = mechs[0];
        assert_eq!(
            ctx.location(mech),
            Some(Location::Battlefield(fixtures::BF1))
        );
        assert!(ctx.is_token(mech));
        assert!(ctx.is_unit(mech));
        assert!(
            is_mech(&ctx, mech),
            "the token is a Mech for every Mech aura"
        );
        assert_eq!(ctx.current_might(mech), i32::from(MECH_MIGHT));
        assert!(ctx.card(mech).unwrap().exhausted, "it enters exhausted");
        assert!(
            ctx.card(mech).unwrap().domain.is_empty(),
            "187.1 · domainless"
        );
        assert_eq!(ctx.controller(mech), 0);
        assert!(ctx.effects.iter().any(|effect| matches!(
            effect,
            Effect::Spawn { face, zone, seat: 0, owner: Some(0) }
                if face.name == MECH_TOKEN && *zone == fixtures::BF1
        )));
        assert!(ctx.events.iter().any(|event| matches!(
            event,
            Event::Played { card, controller: 0, origin: Origin::Board, .. } if *card == mech
        )));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{seat 0}} plays {{card {mech}}} to {{zone 9}}")));
        assert!(mechs_of(&ctx, 1).is_empty());
        assert_eq!(ctx.card(DESIGN).unwrap().zone, Some(fixtures::TRASH));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn from_the_trash_the_flow_play_needs_a_mind_rune_offers_the_base_and_is_banished() {
        let mut fixture = armed(fixtures::TRASH);
        fixture.blob.set_holder(fixtures::BF1, None);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, 0, DESIGN, fixtures::TRASH)),
            Ok(Intent::Play {
                card: DESIGN,
                origin: Origin::Trash {
                    leave: Leave::Banish
                },
                location: None,
                on_chain: true,
            })
        );
        let ready = ctx.ready_runes_of(0).len();
        play_engine::begin(
            &mut ctx,
            0,
            DESIGN,
            Origin::Trash {
                leave: Leave::Banish,
            },
            None,
        )
        .unwrap();
        settle(&mut ctx).unwrap();
        assert_eq!(
            fixtures::labels(&ctx),
            ["{zone 8}", "cancel"],
            "the base is the only location"
        );
        fixtures::choose(&mut ctx, 0, "{zone 8}").unwrap();
        assert_eq!(
            ctx.ready_runes_of(0).len(),
            ready - usize::from(FLOW.energy)
        );
        fixtures::pass_until_open(&mut ctx);
        let mechs = mechs_of(&ctx, 0);
        assert_eq!(mechs.len(), MECHS);
        assert_eq!(ctx.location(mechs[0]), Some(Location::Base(0)));
        assert_eq!(ctx.banished_of(0), [DESIGN]);
        assert!(ctx.trash_of(0).is_empty());
        drop(ctx);
        let mut poor = armed(fixtures::TRASH);
        poor.table.card_mut(MIND_RUNE).unwrap().domain = vec!["Fury".into()];
        poor.resolve();
        let ctx = poor.ctx();
        assert_eq!(
            legal::classify(&ctx, 0, &drag(&ctx, 0, DESIGN, fixtures::TRASH)),
            Err(Refusal::NoPowerOf),
            "the Flow cost wants a Mind rune"
        );
    }

    #[test]
    fn the_other_seat_is_refused_and_a_cancelled_design_plays_nothing() {
        let mut fixture = armed(fixtures::HAND);
        let mut ctx = fixture.ctx();
        assert_eq!(
            legal::classify(&ctx, 1, &drag(&ctx, 1, THEIR_DESIGN, fixtures::HAND)),
            Err(Refusal::NotYourTurn)
        );
        fixtures::play_from_hand(&mut ctx, 0, DESIGN).unwrap();
        assert!(fixtures::choose(&mut ctx, 1, "{zone 8}").is_err());
        fixtures::choose(&mut ctx, 0, "cancel").unwrap();
        assert_eq!(ctx.card(DESIGN).unwrap().zone, Some(fixtures::HAND));
        assert!(ctx.blob.chain.is_empty());
        assert!(mechs_of(&ctx, 0).is_empty());
        assert!(ctx.blob.is_neutral_open());
    }

    #[test]
    #[ignore = "engine gap · Token::Mech: engine/ctx.rs has no Mech face, so the Mech rides ferrous_forerunner::spawn_mech through play_mechs; with the token the spawn is spawn(ctx, seat, Token::Mech, at, false) and cards/mod.rs knows the name"]
    fn the_engine_knows_the_mech_as_a_token_name() {
        assert!(crate::cards::is_token_name(MECH_TOKEN));
    }
}
