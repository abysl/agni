use super::prelude::{unit, with_statics};
use super::{Card, Grant, SelfCost, Static};
use crate::engine::activate::{self, Borrowing};
use crate::engine::ctx::Ctx;

fn always(_: &Ctx, _: u32) -> bool {
    true
}

pub fn lends_exhaust_abilities(ctx: &Ctx, seat: u8, card: u32) -> bool {
    ctx.card(card).is_some_and(|held| {
        ctx.face_in_play(held)
            && !held.is_hidden()
            && ctx.controller(card) == seat
            && (ctx.is_unit(card) || ctx.is_legend(card) || ctx.is_gear(card))
    })
}

pub fn borrowed_exhaust_abilities(ctx: &Ctx, me: u32) -> Vec<(u32, u8)> {
    let seat = ctx.controller(me);
    let mut sources: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .map(|held| held.id)
        .filter(|card| *card != me && lends_exhaust_abilities(ctx, seat, *card))
        .collect();
    sources.sort_unstable();
    let mut borrowed = Vec::new();
    for source in sources {
        let printed = ctx
            .script(source)
            .map(|script| script.abilities)
            .unwrap_or(&[])
            .iter()
            .enumerate()
            .map(|(index, ability)| (source, index as u8, ability));
        let lent = activate::lent_on_by(ctx, source, Borrowing::Skipped)
            .into_iter()
            .map(|lent| (lent.lender, lent.index, lent.ability));
        for (lender, index, ability) in printed.chain(lent) {
            if ability.timing().is_none() {
                continue;
            }
            if activate::self_cost(ctx, source, ability) != SelfCost::Exhaust {
                continue;
            }
            borrowed.push((lender, index));
        }
    }
    borrowed
}

pub static CARD: Card = with_statics(
    unit("Heimerdinger - Inventor", &[], &[]),
    &[Static::While(
        always,
        &[Grant::Borrowed(borrowed_exhaust_abilities)],
    )],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{activated, done, exhausting_self, gear, named, spell};
    use crate::cards::{script_of, Cost, Flow, Item, Resolved, Stage, Timing, GRANTED};
    use crate::engine::ctx::Cause;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::priority;
    use crate::state::{GameBlob, ItemKind};
    use agni_plugin_sdk::table::CardInfo;

    const HEIMER: u32 = 90;
    const LEE: u32 = 91;
    const THEIR_LEE: u32 = 92;
    const TURRET: u32 = 93;
    const HAND_LEE: u32 = 94;
    const GARDENS: u32 = fixtures::GROUNDS;

    fn nothing(_: &mut Ctx, _: &Item, _: Stage) -> Flow {
        done()
    }

    static TURRET_CARD: Card = gear(
        "Turret",
        &[],
        &[
            named(
                activated(Timing::Sorcery, Cost::FREE, &[], nothing),
                "free of exhaust",
            ),
            named(
                exhausting_self(activated(Timing::Reaction, Cost::FREE, &[], nothing)),
                "exhaust to fire",
            ),
        ],
    );

    static QUICK: Card = spell(
        "Quick",
        &[],
        &[named(
            exhausting_self(activated(Timing::Sorcery, Cost::FREE, &[], nothing)),
            "a spell's ability",
        )],
    );

    fn lee(id: u32, zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            domain: vec!["Calm".into()],
            ..fixtures::unit(id, zone, seat, "Lee Sin - Ascetic", 5)
        }
    }

    fn with_scripts(mut fixture: Fixture) -> Fixture {
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(TURRET, &TURRET_CARD)
            .with_script(fixtures::HAND_SPELL, &QUICK);
        fixture
    }

    fn workshop() -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(CardInfo {
            energy: Some(3),
            power: Some(1),
            domain: vec!["Mind".into()],
            ..fixtures::unit(HEIMER, fixtures::BF1, 0, "Heimerdinger - Inventor", 3)
        });
        fixture.table.cards.push(lee(LEE, fixtures::BASE, 0));
        fixture.table.cards.push(lee(THEIR_LEE, fixtures::BASE, 1));
        fixture.table.cards.push(lee(HAND_LEE, fixtures::HAND, 0));
        fixture
            .table
            .cards
            .push(fixtures::gear(TURRET, fixtures::BASE, 0, "Turret", 2));
        fixture.table.card_mut(fixtures::LEGEND_CARD).unwrap().name = "Kha'Zix - Voidreaver".into();
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        with_scripts(fixture)
    }

    fn gardens_workshop(heimerdinger_at: u16) -> Fixture {
        let mut fixture = workshop();
        fixture.table.card_mut(GARDENS).unwrap().name = "Gardens of Becoming".into();
        fixture.table.card_mut(HEIMER).unwrap().zone = Some(heimerdinger_at);
        fixture.table.card_mut(fixtures::VI).unwrap().zone = Some(fixtures::BF1);
        with_scripts(fixture)
    }

    #[test]
    fn heimerdinger_is_a_unit_whose_whole_text_borrows_abilities() {
        assert!(std::ptr::eq(
            script_of("Heimerdinger - Inventor").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.abilities.is_empty());
        assert!(matches!(
            CARD.statics,
            [Static::While(_, [Grant::Borrowed(_)])]
        ));
    }

    #[test]
    fn he_borrows_the_exhaust_abilities_of_friendly_legends_units_and_gear_in_play() {
        let mut fixture = workshop();
        let ctx = fixture.ctx();
        assert!(lends_exhaust_abilities(&ctx, 0, LEE));
        assert!(lends_exhaust_abilities(&ctx, 0, fixtures::LEGEND_CARD));
        assert!(lends_exhaust_abilities(&ctx, 0, TURRET));
        assert!(
            !lends_exhaust_abilities(&ctx, 0, THEIR_LEE),
            "friendly only"
        );
        assert!(!lends_exhaust_abilities(&ctx, 0, HAND_LEE), "in play only");
        assert!(
            !lends_exhaust_abilities(&ctx, 0, fixtures::GROUNDS),
            "legends, units and gear, not battlefields"
        );
        assert!(!lends_exhaust_abilities(&ctx, 0, fixtures::HAND_SPELL));
        assert_eq!(
            borrowed_exhaust_abilities(&ctx, HEIMER),
            [
                (fixtures::LEGEND_CARD, 1),
                (fixtures::LEGEND_CARD, 2),
                (LEE, 0),
                (TURRET, 1),
            ],
            "Kha'Zix's two XP abilities, Lee Sin's buff and the Turret's exhaust ability; the Turret's free ability and Kha'Zix's combat trigger are not exhaust abilities"
        );
        let mut theirs = workshop();
        theirs.table.card_mut(HEIMER).unwrap().owner = 1;
        let mut theirs = with_scripts(theirs);
        let ctx = theirs.ctx();
        assert_eq!(
            borrowed_exhaust_abilities(&ctx, HEIMER),
            [(THEIR_LEE, 0)],
            "an enemy Heimerdinger borrows from his own side"
        );
    }

    #[test]
    fn activating_a_borrowed_ability_exhausts_him_and_not_its_owner() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        let borrowed = borrowed_exhaust_abilities(&ctx, HEIMER);
        let (from, _) = borrowed
            .iter()
            .copied()
            .find(|(source, _)| *source == LEE)
            .unwrap();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == HEIMER && offer.label.contains("buff me"))
            .expect("his strip lists Lee Sin's buff");
        activate::activate(&mut ctx, 0, HEIMER, offer.index).unwrap();
        assert!(ctx.card(HEIMER).unwrap().exhausted, "he pays the exhaust");
        assert!(!ctx.card(from).unwrap().exhausted, "Lee Sin does not");
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert!(
            ctx.is_buffed(HEIMER),
            "'me' in a borrowed ability is Heimerdinger"
        );
        assert!(!ctx.is_buffed(from));
    }

    #[test]
    fn a_pending_borrowed_activation_resolves_after_its_lender_left_play() {
        let mut fixture = workshop();
        let mut ctx = fixture.ctx();
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == HEIMER && offer.label.contains("buff me"))
            .unwrap();
        assert_eq!(offer.index, GRANTED + 2, "after Kha'Zix's two XP abilities");
        activate::activate(&mut ctx, 0, HEIMER, offer.index).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Lent { holder, lender, index: 0 } if holder == HEIMER && lender == LEE
        ));
        priority::pass(&mut ctx, 0).unwrap();
        ctx.kill(LEE, Cause::Rule);
        assert!(!ctx.on_board(LEE));
        assert_eq!(
            borrowed_exhaust_abilities(&ctx, HEIMER),
            [
                (fixtures::LEGEND_CARD, 1),
                (fixtures::LEGEND_CARD, 2),
                (TURRET, 1),
            ],
            "the Turret's ability now sits at his third lent index"
        );
        let saved_item = ctx.blob.chain[0].clone();
        let saved_table = ctx.table.clone();
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        fixture.table = saved_table;
        fixture.blob = GameBlob::decode(&saved_blob).unwrap();
        fixture.scripts = Resolved::of(&fixture.table);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.blob.chain[0], saved_item);
        priority::pass(&mut ctx, 1).unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(
            ctx.is_buffed(HEIMER),
            "727.1.c.3.a: the item still resolves Lee Sin's text, not the Turret's"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn he_borrows_an_exhaust_ability_a_friendly_unit_itself_only_holds_by_grant() {
        let mut fixture = gardens_workshop(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert_eq!(
            borrowed_exhaust_abilities(&ctx, HEIMER),
            [
                (GARDENS, GRANTED),
                (fixtures::LEGEND_CARD, 1),
                (fixtures::LEGEND_CARD, 2),
                (LEE, 0),
                (TURRET, 1),
            ],
            "Vi carries the Gardens' ability, so he has it too"
        );
        let offer = activate::offers(&ctx, 0)
            .into_iter()
            .find(|offer| offer.source == HEIMER && offer.label.contains("gain 1 XP"))
            .expect("his strip lists the Gardens' ability");
        activate::activate(&mut ctx, 0, HEIMER, offer.index).unwrap();
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Lent { holder, lender, index: GRANTED } if holder == HEIMER && lender == GARDENS
        ));
        assert!(ctx.card(HEIMER).unwrap().exhausted, "he pays the exhaust");
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        let saved_item = ctx.blob.chain[0].clone();
        let saved_table = ctx.table.clone();
        let saved_blob = ctx.blob.encode();
        drop(ctx);
        fixture.table = saved_table;
        fixture.blob = GameBlob::decode(&saved_blob).unwrap();
        fixture.scripts = Resolved::of(&fixture.table);
        let mut ctx = fixture.ctx();
        assert_eq!(ctx.blob.chain[0], saved_item);
        assert!(ctx.card(HEIMER).unwrap().exhausted);
        assert!(!ctx.card(fixtures::VI).unwrap().exhausted);
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.xp(0), 1);
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut here = gardens_workshop(fixtures::BF1);
        let ctx = here.ctx();
        let lent: Vec<(u32, u8)> = activate::lent_on(&ctx, HEIMER)
            .iter()
            .map(|lent| (lent.lender, lent.index))
            .collect();
        assert_eq!(
            lent,
            [
                (GARDENS, GRANTED),
                (fixtures::LEGEND_CARD, 1),
                (fixtures::LEGEND_CARD, 2),
                (LEE, 0),
                (TURRET, 1),
            ],
            "standing at the Gardens himself he holds their ability once"
        );
    }
}
