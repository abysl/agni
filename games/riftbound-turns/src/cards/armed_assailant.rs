use super::prelude::unit;
use super::{Card, Keyword};

pub static CARD: Card = unit(
    "Armed Assailant",
    &[Keyword::Accelerate, Keyword::Weaponmaster],
    &[],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::WEAPONMASTER;
    use crate::cards::prelude::{attached_to, is_attached};
    use crate::cards::IMPLICIT_WEAPONMASTER;
    use crate::cards::{script_of, Trigger};
    use crate::engine::cost;
    use crate::engine::ctx::{Ctx, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::prompts;
    use crate::state::{ItemKind, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const ASSAILANT: u32 = 90;
    const BRUTALIZER: u32 = 91;
    const ENERGY: u8 = 6;
    const MIGHT: u8 = 6;
    const ACCELERATED: usize = ENERGY as usize + 1;
    const EXTRA_RUNES: [u32; 4] = [100, 101, 102, 103];

    fn assailant(zone: u16, seat: u8) -> CardInfo {
        CardInfo {
            energy: Some(ENERGY),
            power: Some(1),
            domain: vec!["Fury".into()],
            ..fixtures::unit(ASSAILANT, zone, seat, "Armed Assailant", MIGHT)
        }
    }

    fn armory(ready: usize) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(assailant(fixtures::HAND, 0));
        fixture.table.cards.push(fixtures::gear(
            BRUTALIZER,
            fixtures::BASE,
            0,
            "Brutalizer",
            2,
        ));
        for rune in EXTRA_RUNES {
            fixture
                .table
                .cards
                .push(fixtures::rune(rune, 0, "Fury", false));
        }
        let runes: Vec<u32> = fixture
            .table
            .cards
            .iter()
            .filter(|card| card.is_kind("Rune") && card.owner == 0)
            .map(|card| card.id)
            .collect();
        for (index, rune) in runes.into_iter().enumerate() {
            fixture.table.card_mut(rune).unwrap().exhausted = index >= ready;
        }
        fixture.resolve();
        assert!(std::ptr::eq(
            fixture.scripts.of_card(ASSAILANT).unwrap(),
            &CARD
        ));
        fixture
    }

    fn weaponmaster_prompt(ctx: &Ctx) {
        assert!(
            matches!(ctx.blob.why, Some(PromptWhy::Target { spec: 0, .. })),
            "the Weaponmaster offer follows the entry: {:?}",
            ctx.blob.why
        );
        assert_eq!(
            fixtures::labels(ctx),
            [format!("{{card {BRUTALIZER}}}"), "skip".to_string()]
        );
    }

    #[test]
    fn the_script_prints_accelerate_and_weaponmaster_with_the_shared_play_trigger() {
        assert!(std::ptr::eq(script_of("Armed Assailant").unwrap(), &CARD));
        assert_eq!(CARD.name, "Armed Assailant");
        assert_eq!(CARD.keywords, &[Keyword::Accelerate, Keyword::Weaponmaster]);
        assert!(CARD.statics.is_empty());
        assert!(CARD.replacement.is_none());
        assert!(CARD.additional.is_none());
        assert!(
            CARD.abilities.is_empty(),
            "821 · Weaponmaster is the engine's"
        );
        assert_eq!(WEAPONMASTER.trigger, Trigger::Play);
        assert_eq!(WEAPONMASTER.label, Some("weaponmaster"));
        assert!(WEAPONMASTER.optional);
        let mut fixture = armory(ACCELERATED);
        let ctx = fixture.ctx();
        assert!(cost::can_accelerate(&ctx, ASSAILANT));
    }

    #[test]
    fn accelerating_enters_ready_and_the_weaponmaster_offer_then_attaches_the_equipment() {
        let mut fixture = armory(ACCELERATED);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ASSAILANT).unwrap();
        let why = ctx.blob.why;
        assert!(
            matches!(why, Some(PromptWhy::OptionalCost { .. })),
            "the Accelerate is asked first: {why:?}"
        );
        assert_eq!(
            prompts::status(&ctx, why.unwrap()),
            format!("accelerate {{card {ASSAILANT}}} for 1 energy and 1 Fury power?")
        );
        fixtures::choose(&mut ctx, 0, "yes").unwrap();
        assert_eq!(ctx.location(ASSAILANT), Some(Location::Base(0)));
        assert!(
            !ctx.card(ASSAILANT).unwrap().exhausted,
            "731.6 · accelerated, it enters ready"
        );
        assert!(ctx.ready_runes_of(0).is_empty());
        weaponmaster_prompt(&ctx);
        let runes = ctx.runes_of(0).len();
        fixtures::choose(&mut ctx, 0, &format!("{{card {BRUTALIZER}}}")).unwrap();
        assert!(matches!(
            ctx.blob.chain.last().map(|top| top.kind),
            Some(ItemKind::Trigger { source, index: IMPLICIT_WEAPONMASTER }) if source == ASSAILANT
        ));
        assert!(!is_attached(&ctx, BRUTALIZER));
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(attached_to(&ctx, BRUTALIZER), Some(ASSAILANT));
        assert_eq!(
            ctx.runes_of(0).len(),
            runes - 1,
            "the Calm rune is recycled for the Equip"
        );
        assert_eq!(ctx.current_might(ASSAILANT), i32::from(MIGHT) + 3);
        assert!(!ctx.card(ASSAILANT).unwrap().exhausted, "still ready");
        assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
    }

    #[test]
    fn declining_the_accelerate_enters_exhausted_and_the_offer_can_be_skipped() {
        let mut fixture = armory(ACCELERATED);
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ASSAILANT).unwrap();
        assert!(matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })));
        fixtures::choose(&mut ctx, 0, "no").unwrap();
        assert!(ctx.card(ASSAILANT).unwrap().exhausted);
        assert_eq!(ctx.ready_runes_of(0).len(), 1, "one rune was spared");
        weaponmaster_prompt(&ctx);
        let runes = ctx.runes_of(0).len();
        fixtures::choose(&mut ctx, 0, "skip").unwrap();
        fixtures::pass_until_open(&mut ctx);
        assert!(ctx.blob.chain.is_empty());
        assert!(!is_attached(&ctx, BRUTALIZER));
        assert_eq!(ctx.runes_of(0).len(), runes);
        assert_eq!(ctx.current_might(ASSAILANT), i32::from(MIGHT));
    }

    #[test]
    fn with_runes_for_the_printed_cost_only_the_accelerate_is_not_offered() {
        let mut fixture = armory(ENERGY.into());
        let mut ctx = fixture.ctx();
        fixtures::play_from_hand(&mut ctx, 0, ASSAILANT).unwrap();
        assert!(
            !matches!(ctx.blob.why, Some(PromptWhy::OptionalCost { .. })),
            "an accelerate the seat cannot pay is not asked: {:?}",
            ctx.blob.why
        );
        assert_eq!(ctx.location(ASSAILANT), Some(Location::Base(0)));
        assert!(ctx.card(ASSAILANT).unwrap().exhausted);
        assert!(ctx.ready_runes_of(0).is_empty());
        weaponmaster_prompt(&ctx);
    }
}
