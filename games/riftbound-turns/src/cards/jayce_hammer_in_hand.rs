use super::prelude::{choosing, chosen_mode, done, grant_this_turn, mode, on_readied, unit};
use super::{Card, Flow, Item, Keyword, ModeSpec, Stage};
use crate::engine::ctx::Ctx;

pub const ASSAULT: u8 = 2;
pub const DEFLECT: u8 = 2;
pub const STAGE_GIVE: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Assault,
    Deflect,
    Ganking,
}

pub const MODES: [Mode; 3] = [Mode::Assault, Mode::Deflect, Mode::Ganking];
pub const CHOOSE_ONE: &[ModeSpec] = &[
    mode("Assault 2", &[], give_chosen),
    mode("Deflect 2", &[], give_chosen),
    mode("Ganking", &[], give_chosen),
];

pub fn keyword_of(mode: Mode) -> Keyword {
    match mode {
        Mode::Assault => Keyword::Assault(ASSAULT),
        Mode::Deflect => Keyword::Deflect(DEFLECT),
        Mode::Ganking => Keyword::Ganking,
    }
}

pub fn label_of(mode: Mode) -> &'static str {
    match mode {
        Mode::Assault => "Assault 2",
        Mode::Deflect => "Deflect 2",
        Mode::Ganking => "Ganking",
    }
}

pub fn mode_of(item: &Item) -> Option<Mode> {
    MODES.get(usize::from(chosen_mode(item)?)).copied()
}

pub fn give(ctx: &mut Ctx, me: u32, mode: Mode) -> bool {
    if !ctx.on_board(me) {
        ctx.narrate(format!(
            "{{card {me}}} has left the board · nothing is given"
        ));
        return false;
    }
    let keyword = keyword_of(mode);
    grant_this_turn(ctx, me, keyword);
    ctx.narrate(format!("{{card {me}}} gets {} this turn", label_of(mode)));
    true
}

fn give_chosen(ctx: &mut Ctx, item: &Item, _: Stage) -> Flow {
    if let Some(mode) = mode_of(item) {
        give(ctx, item.kind.source(), mode);
    }
    done()
}

fn choose_one(ctx: &mut Ctx, item: &Item, stage: Stage) -> Flow {
    match stage.0 {
        STAGE_GIVE => give_chosen(ctx, item, stage),
        _ => Flow::Ask(ctx.ask_resume(item, STAGE_GIVE, 1, 1)),
    }
}

pub static CARD: Card = unit(
    "Jayce, Hammer in Hand",
    &[],
    &[choosing(on_readied(&[], choose_one), CHOOSE_ONE)],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{this_turn, Location};
    use crate::cards::{script_of, Trigger, Who};
    use crate::engine::ctx::Event;
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::{march, phases, priority, settle};
    use crate::state::{ChainItem, ItemKind, Origin, Phase, PromptWhy};
    use agni_plugin_sdk::table::CardInfo;

    const JAYCE: u32 = 90;

    fn jayce(exhausted: bool) -> CardInfo {
        CardInfo {
            energy: Some(4),
            power: Some(1),
            domain: vec!["Body".into()],
            exhausted,
            ..fixtures::unit(JAYCE, fixtures::BF1, 0, "Jayce, Hammer in Hand", 5)
        }
    }

    fn workshop(exhausted: bool) -> Fixture {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(jayce(exhausted));
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        assert!(std::ptr::eq(fixture.scripts.of_card(JAYCE).unwrap(), &CARD));
        fixture
    }

    fn resolve_the_chain(ctx: &mut Ctx) {
        while !ctx.blob.chain.is_empty() && ctx.blob.prompt.is_none() {
            let holder = priority::holder(ctx).expect("someone holds priority");
            priority::pass(ctx, holder).unwrap();
        }
    }

    fn jayce_items(ctx: &Ctx) -> usize {
        ctx.blob
            .chain
            .iter()
            .filter(|item| matches!(item.kind, ItemKind::Trigger { source, .. } if source == JAYCE))
            .count()
    }

    fn across(ctx: &Ctx) -> bool {
        march::legal_destination(
            ctx,
            JAYCE,
            Location::Battlefield(fixtures::BF1),
            Location::Battlefield(fixtures::BF2),
        )
        .is_ok()
    }

    #[test]
    fn the_script_is_one_untargeted_readied_trigger_over_three_named_modes() {
        assert!(std::ptr::eq(
            script_of("Jayce, Hammer in Hand").unwrap(),
            &CARD
        ));
        assert_eq!(CARD.name, "Jayce, Hammer in Hand");
        assert!(CARD.keywords.is_empty());
        assert!(!CARD.has_keyword(Keyword::Assault(0)));
        assert!(!CARD.has_keyword(Keyword::Deflect(0)));
        assert!(!CARD.has_keyword(Keyword::Ganking));
        assert!(CARD.statics.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = &CARD.abilities[0];
        assert_eq!(ability.trigger, Trigger::Readied(Who::Me));
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.candidates.is_none());
        assert_eq!(MODES.len(), 3);
        assert_eq!(
            ability
                .modes
                .iter()
                .map(|mode| mode.label)
                .collect::<Vec<_>>(),
            MODES.map(label_of)
        );
        assert!(ability.modes.iter().all(|mode| mode.targets.is_empty()));
        let mut item = ChainItem::new(
            1,
            ItemKind::Trigger {
                source: JAYCE,
                index: 0,
            },
            0,
            Origin::Board,
        );
        assert_eq!(mode_of(&item), None);
        for (index, mode) in MODES.iter().enumerate() {
            item.set_mode(0, index as u8);
            assert_eq!(mode_of(&item), Some(*mode));
        }
        item.set_mode(0, 3);
        assert_eq!(mode_of(&item), None);
        assert_eq!(keyword_of(Mode::Assault), Keyword::Assault(2));
        assert_eq!(keyword_of(Mode::Deflect), Keyword::Deflect(2));
        assert_eq!(keyword_of(Mode::Ganking), Keyword::Ganking);
        assert_eq!(MODES.map(label_of), ["Assault 2", "Deflect 2", "Ganking"]);
    }

    #[test]
    fn each_mode_gives_him_its_keyword_for_the_turn_only() {
        let mut fixture = workshop(false);
        let mut ctx = fixture.ctx();
        assert!(give(&mut ctx, JAYCE, Mode::Assault));
        assert!(ctx.has_keyword(JAYCE, Keyword::Assault(2)));
        assert_eq!(
            ctx.current_might(JAYCE),
            5,
            "Assault counts while attacking"
        );
        ctx.mark_attacker(JAYCE);
        assert_eq!(ctx.current_might(JAYCE), 7);
        ctx.clear_designation(JAYCE);
        assert!(give(&mut ctx, JAYCE, Mode::Deflect));
        assert_eq!(ctx.deflect_of(JAYCE), 2);
        assert!(!across(&ctx));
        assert!(give(&mut ctx, JAYCE, Mode::Ganking));
        assert!(ctx.has_keyword(JAYCE, Keyword::Ganking));
        assert!(across(&ctx));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {JAYCE}}} gets Ganking this turn")));
        let until = this_turn(&ctx);
        ctx.expire(until);
        assert!(!ctx.has_keyword(JAYCE, Keyword::Assault(2)));
        assert_eq!(ctx.deflect_of(JAYCE), 0);
        assert!(!ctx.has_keyword(JAYCE, Keyword::Ganking));
        assert!(!across(&ctx));
        assert!(ctx.fault.is_none());
    }

    #[test]
    fn the_gift_refuses_a_jayce_off_the_board() {
        let mut fixture = workshop(false);
        let mut ctx = fixture.ctx();
        ctx.trash(JAYCE);
        assert!(!give(&mut ctx, JAYCE, Mode::Ganking));
        assert!(!ctx.has_keyword(JAYCE, Keyword::Ganking));
        assert!(ctx.blob.log.contains(&format!(
            "{{card {JAYCE}}} has left the board · nothing is given"
        )));
    }

    #[test]
    fn the_awaken_step_fires_the_trigger_and_parks_it_on_the_choice() {
        let mut fixture = workshop(true);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        assert!(ctx.events.contains(&Event::Readied { card: JAYCE, by: 0 }));
        assert_eq!(jayce_items(&ctx), 1);
        resolve_the_chain(&mut ctx);
        assert_eq!(jayce_items(&ctx), 1, "parked on the choice");
        assert_eq!(
            ctx.blob.why,
            Some(PromptWhy::Resume {
                item: 1,
                stage: STAGE_GIVE
            })
        );
        assert_eq!(ctx.blob.prompt.as_ref().unwrap().seat, 0);
        assert!(!ctx.has_keyword(JAYCE, Keyword::Assault(2)));
        assert_eq!(ctx.deflect_of(JAYCE), 0);
        assert!(!ctx.has_keyword(JAYCE, Keyword::Ganking));
        assert_eq!(ctx.blob.phase(), Some(Phase::Beginning));
        fixtures::choose(&mut ctx, 0, "Deflect 2").unwrap();
        assert!(ctx.blob.chain.is_empty());
        assert!(ctx.blob.prompt.is_none());
        assert_eq!(ctx.deflect_of(JAYCE), 2);
        assert!(!ctx.has_keyword(JAYCE, Keyword::Assault(2)));
        assert!(!ctx.has_keyword(JAYCE, Keyword::Ganking));
        assert!(ctx
            .blob
            .log
            .contains(&format!("{{card {JAYCE}}} gets Deflect 2 this turn")));
        assert_eq!(ctx.blob.phase(), Some(Phase::Action));
        assert!(ctx.fault.is_none());
        drop(ctx);
        let mut fixture = workshop(false);
        let mut ctx = fixture.ctx();
        assert!(!ctx.ready(JAYCE), "already ready is not becoming ready");
        settle(&mut ctx).unwrap();
        assert_eq!(jayce_items(&ctx), 0);
    }

    #[test]
    fn readied_he_is_asked_which_of_the_three_to_take_and_the_pick_is_given_this_turn() {
        let mut fixture = workshop(true);
        let mut ctx = fixture.ctx();
        phases::start_turn(&mut ctx);
        resolve_the_chain(&mut ctx);
        assert!(matches!(ctx.blob.why, Some(PromptWhy::Resume { .. })));
        assert_eq!(
            fixtures::labels(&ctx),
            ["Assault 2", "Deflect 2", "Ganking"]
        );
        fixtures::choose(&mut ctx, 0, "Ganking").unwrap();
        assert!(ctx.has_keyword(JAYCE, Keyword::Ganking));
        assert!(across(&ctx));
    }
}
