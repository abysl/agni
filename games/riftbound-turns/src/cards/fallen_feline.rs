use super::prelude::{done, play, unit};
use super::tianna_crownguard::holds_the_line;
use super::{base_name, Card, Flow, Item, NameKind, Stage, KIND_SPELL};
use crate::engine::ctx::Ctx;

pub const NAME_A_SPELL: u8 = 0;
pub const STAGE_NAMED: u8 = 1;

pub fn named_spell(ctx: &Ctx, feline: u32) -> Option<String> {
    ctx.named(feline).map(str::to_string)
}

pub fn is_a_spell_named(ctx: &Ctx, card: u32, name: &str) -> bool {
    let name = name.trim();
    !name.is_empty()
        && ctx.card(card).is_some_and(|held| {
            held.kind.as_deref() == Some(KIND_SPELL) && base_name(&held.name) == name
        })
}

pub fn locks(ctx: &Ctx, me: u32, seat: u8, card: u32, name: &str) -> bool {
    ctx.script(me)
        .is_some_and(|script| std::ptr::eq(script, &CARD))
        && ctx.controller(me) != seat
        && holds_the_line(ctx, me)
        && is_a_spell_named(ctx, card, name)
}

pub fn cannot_play(ctx: &Ctx, seat: u8, card: u32) -> bool {
    let mut felines: Vec<u32> = ctx
        .table
        .cards
        .iter()
        .filter(|held| {
            ctx.script(held.id)
                .is_some_and(|script| std::ptr::eq(script, &CARD))
        })
        .map(|held| held.id)
        .collect();
    felines.sort_unstable();
    felines.into_iter().any(|feline| {
        named_spell(ctx, feline).is_some_and(|name| locks(ctx, feline, seat, card, &name))
    })
}

fn name_a_spell(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    if stage.0 == STAGE_NAMED || named_spell(ctx, item.kind.source()).is_some() {
        return done();
    }
    Flow::Ask(ctx.ask_name(item, NameKind::Spell, STAGE_NAMED))
}

pub static CARD: Card = unit("Fallen Feline", &[], &[play(&[], name_a_spell)]);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{move_unit, Location, Moved};
    use crate::cards::{script_of, Trigger};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{activate, priority};
    use crate::state::{ItemKind, PromptWhy};
    use crate::Refusal;
    use agni_plugin_sdk::table::CardInfo;

    const FELINE: u32 = 90;
    const THEIR_DEFY: u32 = 91;
    const THEIR_STUPEFY: u32 = 92;
    const MY_DEFY: u32 = 93;
    const ORDER_RUNE: u32 = 46;
    const SPARE_RUNE: u32 = 47;

    fn feline(zone: u16) -> CardInfo {
        CardInfo {
            energy: Some(2),
            power: Some(1),
            domain: vec!["Order".into()],
            ..fixtures::unit(FELINE, zone, 0, "Fallen Feline", 3)
        }
    }

    fn alley(zone: u16) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(feline(zone));
        let mut defy = fixtures::spell(THEIR_DEFY, fixtures::HAND, 1, "Defy (Alternate Art)", 1, 0);
        defy.domain = vec!["Mind".into()];
        fixture.table.cards.push(defy);
        let mut stupefy = fixtures::spell(THEIR_STUPEFY, fixtures::HAND, 1, "Stupefy", 1, 0);
        stupefy.domain = vec!["Mind".into()];
        fixture.table.cards.push(stupefy);
        fixture
            .table
            .cards
            .push(fixtures::spell(MY_DEFY, fixtures::HAND, 0, "Defy", 1, 0));
        fixture
            .table
            .cards
            .push(fixtures::rune(ORDER_RUNE, 0, "Order", false));
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(FELINE).unwrap(),
            &CARD
        ));
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: agni_plugin_sdk::decide::TOP,
            hidden: false,
        }
    }

    #[test]
    fn the_script_is_one_play_trigger_that_names_on_the_chain_and_the_lock_reads_the_name() {
        assert!(std::ptr::eq(script_of("Fallen Feline").unwrap(), &CARD));
        assert!(CARD.keywords.is_empty());
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.names, None, "she names as the trigger resolves");
        assert_eq!(CARD.abilities.len(), 1);
        let naming = &CARD.abilities[usize::from(NAME_A_SPELL)];
        assert_eq!(naming.trigger, Trigger::Play);
        assert!(naming.targets.is_empty());
        assert!(!naming.optional);
        let mut fixture = alley(fixtures::BF1);
        let ctx = fixture.ctx();
        assert_eq!(named_spell(&ctx, FELINE), None);
        assert!(!cannot_play(&ctx, 1, THEIR_DEFY), "no name, no lock");
    }

    #[test]
    fn a_name_matches_a_spell_by_its_base_name_and_never_a_unit() {
        let mut fixture = alley(fixtures::BF1);
        let ctx = fixture.ctx();
        assert!(is_a_spell_named(&ctx, THEIR_DEFY, "Defy"));
        assert!(is_a_spell_named(&ctx, MY_DEFY, "Defy"));
        assert!(!is_a_spell_named(&ctx, THEIR_STUPEFY, "Defy"));
        assert!(!is_a_spell_named(&ctx, THEIR_DEFY, "Stupefy"));
        assert!(!is_a_spell_named(&ctx, THEIR_DEFY, ""));
        assert!(
            !is_a_spell_named(&ctx, fixtures::THEIR_UNIT, "Jinx"),
            "a unit is not a spell"
        );
        assert!(!is_a_spell_named(&ctx, 999, "Defy"));
    }

    #[test]
    fn the_lock_binds_opponents_at_a_battlefield_only_and_never_her_own_controller() {
        let mut fixture = alley(fixtures::BASE);
        let mut ctx = fixture.ctx();
        assert!(
            !locks(&ctx, FELINE, 1, THEIR_DEFY, "Defy"),
            "from the base she locks nothing"
        );
        assert_eq!(
            move_unit(
                &mut ctx,
                &fixtures::effect_of(0),
                FELINE,
                Location::Battlefield(fixtures::BF1)
            ),
            Some(Moved::Moved)
        );
        assert!(locks(&ctx, FELINE, 1, THEIR_DEFY, "Defy"));
        assert!(
            !locks(&ctx, FELINE, 1, THEIR_STUPEFY, "Defy"),
            "another spell"
        );
        assert!(
            !locks(&ctx, FELINE, 0, MY_DEFY, "Defy"),
            "her own controller is not an opponent"
        );
        assert!(
            !locks(&ctx, fixtures::VI, 1, THEIR_DEFY, "Defy"),
            "Vi is not a Feline"
        );
        assert!(ctx.stun(FELINE));
        assert!(
            locks(&ctx, FELINE, 1, THEIR_DEFY, "Defy"),
            "a stunned Feline still stands at the battlefield"
        );
        ctx.recall(FELINE, true);
        assert!(!locks(&ctx, FELINE, 1, THEIR_DEFY, "Defy"));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn her_play_trigger_parks_on_the_name_prompt_and_until_it_is_answered_the_opponents_defy_still_plays(
    ) {
        let mut fixture = alley(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FELINE).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: NAME_A_SPELL }) if source == FELINE
        ));
        priority::pass(&mut ctx, 0).unwrap();
        assert!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STUPEFY)).is_ok(),
            "no name is stored, so the lock never applies"
        );
        assert!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DEFY)).is_ok(),
            "the Defy she would lock plays as ever"
        );
        priority::pass(&mut ctx, 1).unwrap();
        assert_eq!(ctx.blob.chain.len(), 1, "parked on the name");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Name {
                item: 2,
                kind: NameKind::Spell,
                stage: STAGE_NAMED
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        let offered = fixtures::labels(&ctx);
        assert_eq!(
            offered,
            crate::cards::spell_names(),
            "762: the catalog's spells, never the table's"
        );
        assert!(offered.contains(&"Defy".to_string()));
        assert!(offered.contains(&"Stupefy".to_string()));
        assert!(
            !offered.contains(&"Spark".to_string()),
            "her controller's own hand does not shape the list either"
        );
        assert_eq!(named_spell(&ctx, FELINE), None);
        assert!(!cannot_play(&ctx, 1, THEIR_DEFY));
        fixtures::choose(&mut ctx, 0, "Stupefy").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(named_spell(&ctx, FELINE).as_deref(), Some("Stupefy"));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {FELINE}}} names Stupefy")));
        assert!(
            !cannot_play(&ctx, 1, THEIR_STUPEFY),
            "she stands in the base, so nothing is locked yet"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn her_controller_names_a_spell_and_the_opponents_copies_are_refused_while_she_stands_afield() {
        let mut fixture = alley(fixtures::HAND);
        fixture
            .table
            .cards
            .push(fixtures::rune(SPARE_RUNE, 0, "Fury", false));
        fixture.resolve();
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FELINE).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::choose(&mut ctx, 0, "Defy").unwrap();
        assert_eq!(named_spell(&ctx, FELINE).as_deref(), Some("Defy"));
        assert!(
            !cannot_play(&ctx, 1, THEIR_DEFY),
            "in the base she locks nothing"
        );
        assert_eq!(
            move_unit(
                &mut ctx,
                &fixtures::effect_of(0),
                FELINE,
                Location::Battlefield(fixtures::BF1),
            ),
            Some(Moved::Moved)
        );
        assert!(cannot_play(&ctx, 1, THEIR_DEFY));
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert_eq!(
            legal::classify(&ctx, 0, &entry(&ctx, 0, MY_DEFY)).map(|_| ()),
            Ok(()),
            "her own controller is not an opponent"
        );
        fixtures::play_from_hand(&mut ctx, 0, MY_DEFY).unwrap();
        fixtures::choose(
            &mut ctx,
            0,
            &format!("{{card {}}} on the chain", fixtures::HAND_SPELL),
        )
        .unwrap();
        assert_eq!(ctx.blob.chain.len(), 2, "her own Defy answers the Spark");
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DEFY)),
            Err(Refusal::Illegal(Reason::NoSpells))
        );
        assert!(legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STUPEFY)).is_ok());
        assert!(
            !activate::flow_offers(&ctx, 1)
                .iter()
                .any(|offer| offer.source == THEIR_DEFY),
            "a locked name is not offered from the trash either"
        );
        ctx.recall(FELINE, true);
        assert!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DEFY)).is_ok(),
            "recalled to the base she locks nothing"
        );
        assert!(ctx.fault.is_none());
    }

    #[test]
    #[ignore = "rules question · the closing assertion plays her controller's Defy after the Spark has resolved and expects it on the chain, but Defy chooses a spell to counter and the chain is empty, so 355.8 keeps it off the chain (the play parks on a target prompt offering only cancel); everything before that line runs — the name prompt, the stored name, the lock on the opponent's Defy while she stands at a battlefield — and her_controller_names_a_spell_and_the_opponents_copies_are_refused_while_she_stands_afield covers it with the Defy played while the Spark is still on the chain"]
    fn playing_her_names_a_spell_and_the_opponents_copies_are_refused_while_she_stands_afield() {
        let mut fixture = alley(fixtures::HAND);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, FELINE).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::choose(&mut ctx, 0, "Defy").unwrap();
        assert_eq!(named_spell(&ctx, FELINE).as_deref(), Some("Defy"));
        assert_eq!(
            move_unit(
                &mut ctx,
                &fixtures::effect_of(0),
                FELINE,
                Location::Battlefield(fixtures::BF1)
            ),
            Some(Moved::Moved)
        );
        assert!(cannot_play(&ctx, 1, THEIR_DEFY));
        fixtures::play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        priority::pass(&mut ctx, 0).unwrap();
        assert_eq!(
            legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_DEFY)),
            Err(Refusal::Illegal(Reason::NoSpells))
        );
        assert!(legal::classify(&ctx, 1, &entry(&ctx, 1, THEIR_STUPEFY)).is_ok());
        priority::pass(&mut ctx, 1).unwrap();
        fixtures::play_from_hand(&mut ctx, 0, MY_DEFY).unwrap();
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "her own controller plays Defy as ever"
        );
    }
}
