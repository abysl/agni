use super::prelude::{
    done, in_combat, might_this_turn, on_move_to_battlefield, unit, with_statics,
};
use super::{Card, Static};

pub const MIGHT: i16 = 2;

pub static CARD: Card = with_statics(
    unit(
        "Akali, Silent",
        &[],
        &[on_move_to_battlefield(&[], |ctx, item, _| {
            let me = item.kind.source();
            if ctx.on_board(me) {
                might_this_turn(ctx, item, me, MIGHT, None);
            }
            done()
        })],
    ),
    &[Static::Untargetable(|ctx, card| !in_combat(ctx, card))],
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::prelude::{a_unit, card_target, deal, play, spell};
    use crate::cards::{Flow, Trigger, Where, Who};
    use crate::engine::ctx::{Cause, Ctx, Event, Location};
    use crate::engine::fixtures::{self, Fixture};
    use crate::engine::legal::{self, Reason};
    use crate::engine::{
        march, phases, play as playing, priority, prompts, resume, settle, targets,
    };
    use crate::state::{ChainItem, GameBlob, ItemKind, Mode, Origin, TargetRef};
    use crate::Refusal;
    use agni_plugin_sdk::decide::TOP;
    use agni_plugin_sdk::prompt::Pick;
    use agni_plugin_sdk::table::Target;

    const AKALI: u32 = 90;
    const ENEMY_ZAP: u32 = 91;

    static ZAP: Card = spell(
        "Zap",
        &[],
        &[play(&[a_unit("a unit")], |ctx, item, _| {
            if let Some(unit) = card_target(ctx, item, 0) {
                deal(ctx, item, unit, 1);
            }
            done()
        })],
    );

    static PICKER: Card = spell(
        "Picker",
        &[],
        &[crate::cards::prelude::with_candidates(
            play(&[], |_, _, _| Flow::Done),
            |ctx, item, _| {
                let mut units = ctx.units_at(Location::Base(1 - item.controller));
                units.extend(ctx.units_at(Location::Battlefield(fixtures::BF2)));
                units.into_iter().map(TargetRef::Card).collect()
            },
        )],
    );

    fn shadowed() -> Fixture {
        let mut fixture = Fixture::enforced();
        let mut akali = fixtures::unit(AKALI, fixtures::BASE, 0, "Akali, Silent", 4);
        akali.domain = vec!["Calm".into()];
        akali.energy = Some(4);
        akali.power = Some(1);
        fixture.table.cards.push(akali);
        fixture.table.card_mut(fixtures::VI).unwrap().exhausted = true;
        fixture.resolve();
        fixture
    }

    fn hunted() -> Fixture {
        let mut fixture = shadowed();
        fixture
            .table
            .cards
            .push(fixtures::spell(ENEMY_ZAP, fixtures::HAND, 1, "Zap", 1, 0));
        fixture.blob = GameBlob::start(2, 1, Mode::Enforced);
        fixture.blob.seats = vec![Default::default(); 2];
        fixture.resolve();
        fixture.scripts = fixture.scripts.clone().with_script(ENEMY_ZAP, &ZAP);
        fixture
    }

    fn entry(ctx: &Ctx, seat: u8, card: u32) -> crate::engine::ctx::EntryMove {
        crate::engine::ctx::EntryMove {
            card,
            from: ctx.zones.hand,
            from_seat: seat,
            to: ctx.zones.chain,
            to_seat: 0,
            index: TOP,
            hidden: false,
        }
    }

    fn play_from_hand(ctx: &mut Ctx, seat: u8, card: u32) -> Result<(), Refusal> {
        legal::classify(ctx, seat, &entry(ctx, seat, card))?;
        let chain = ctx.zones.chain.unwrap_or(0);
        ctx.table
            .apply_entry(&fixtures::move_action(card, chain, 0), seat)
            .unwrap();
        playing::begin(ctx, seat, card, Origin::Hand, None)?;
        settle(ctx)
    }

    fn pick(ctx: &mut Ctx, seat: u8, option: u16) -> Result<(), Refusal> {
        let prompt = ctx
            .blob
            .prompt
            .as_ref()
            .map(|prompt| prompt.id)
            .unwrap_or(0);
        if let Some(answered) = prompts::answer(ctx, seat, Pick { prompt, option })? {
            resume(ctx, &answered)?;
        }
        settle(ctx)
    }

    fn labels(ctx: &Ctx) -> Vec<String> {
        prompts::offered(ctx)
            .iter()
            .map(|opt| opt.label.clone())
            .collect()
    }

    fn choose(ctx: &mut Ctx, seat: u8, label: &str) -> Result<(), Refusal> {
        let option = labels(ctx)
            .iter()
            .position(|held| held == label)
            .unwrap_or_else(|| panic!("{label} is not offered: {:?}", labels(ctx)))
            as u16;
        pick(ctx, seat, option)
    }

    fn might_counter(ctx: &Ctx, card: u32) -> i32 {
        ctx.table
            .counter(Target::Card(card), crate::engine::ctx::COUNTER_MIGHT)
            .unwrap_or(0)
    }

    fn march_to(ctx: &mut Ctx, from: Location, to: Location) {
        march::standard_move(ctx, 0, AKALI, from, to);
        settle(ctx).unwrap();
    }

    fn resolve(ctx: &mut Ctx, first: u8) {
        priority::pass(ctx, first).unwrap();
        priority::pass(ctx, 1 - first).unwrap();
    }

    #[test]
    fn the_registry_resolves_akali_with_one_move_trigger_and_a_silent_static() {
        assert!(std::ptr::eq(
            crate::cards::script_of("Akali, Silent").unwrap(),
            &CARD
        ));
        assert!(CARD.keywords.is_empty());
        assert_eq!(CARD.abilities.len(), 1);
        let ability = CARD.abilities[0];
        assert_eq!(
            ability.trigger,
            Trigger::Move {
                of: Who::Me,
                to: Where::Battlefield
            }
        );
        assert!(ability.targets.is_empty());
        assert!(!ability.optional);
        assert!(ability.cost.is_none() && ability.condition.is_none());
        assert!(CARD.has_static(Static::Untargetable(|_, _| true)));
        assert_eq!(CARD.statics.len(), 1);
    }

    #[test]
    fn akali_marching_to_a_battlefield_gives_herself_two_might_until_end_of_turn() {
        let mut fixture = shadowed();
        let action = fixtures::move_action(AKALI, fixtures::BF1, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        assert_eq!(ctx.current_might(AKALI), 4);
        march_to(
            &mut ctx,
            Location::Base(0),
            Location::Battlefield(fixtures::BF1),
        );
        assert_eq!(
            ctx.blob.chain.len(),
            1,
            "the move trigger waits on the chain"
        );
        assert!(matches!(
            ctx.blob.chain[0].kind,
            ItemKind::Trigger { source, index: 0 } if source == AKALI
        ));
        assert!(ctx.blob.prompt.is_none(), "the trigger chooses nothing");
        assert_eq!(
            ctx.current_might(AKALI),
            4,
            "the bonus waits for the trigger to resolve"
        );
        resolve(&mut ctx, 0);
        assert!(ctx.blob.chain.is_empty());
        assert_eq!(might_counter(&ctx, AKALI), i32::from(MIGHT));
        assert_eq!(ctx.current_might(AKALI), 6);
        assert_eq!(
            might_counter(&ctx, fixtures::VI),
            0,
            "only Akali grows herself"
        );
        assert!(ctx.blob.log.contains(&"{card 90} triggers".to_string()));
        resolve(&mut ctx, 0);
        assert!(
            ctx.blob.showdown.is_none(),
            "the showdown closes and seat 0 takes the battlefield"
        );
        assert_eq!(ctx.blob.holder(fixtures::BF1), Some(0));
        assert_eq!(
            ctx.current_might(AKALI),
            6,
            "the bonus outlives the showdown"
        );
        let mut blob = ctx.blob.clone();
        let table = ctx.table.clone();
        drop(ctx);
        blob.prompt = None;
        blob.why = None;
        fixture.commit(table);
        fixture.blob = blob;
        let mut ctx = fixture.ctx();
        phases::end_turn(&mut ctx).unwrap();
        assert_eq!(might_counter(&ctx, AKALI), 0);
        assert_eq!(
            ctx.current_might(AKALI),
            4,
            "the bonus is only for the turn"
        );
    }

    #[test]
    fn a_march_home_from_a_battlefield_gives_her_nothing() {
        let mut fixture = shadowed();
        fixture.table.card_mut(AKALI).unwrap().zone = Some(fixtures::BF1);
        fixture.table.card_mut(AKALI).unwrap().seat = 0;
        fixture.blob.set_holder(fixtures::BF1, Some(0));
        fixture.resolve();
        let action = fixtures::move_action(AKALI, fixtures::BASE, 0);
        let mut ctx = fixture.ctx_for(0, &action);
        march_to(
            &mut ctx,
            Location::Battlefield(fixtures::BF1),
            Location::Base(0),
        );
        assert!(ctx.blob.chain.is_empty() && ctx.blob.queue.is_empty());
        assert_eq!(might_counter(&ctx, AKALI), 0);
        assert_eq!(ctx.current_might(AKALI), 4);
    }

    #[test]
    fn an_enemy_spell_is_refused_akali_while_she_is_out_of_combat() {
        let mut fixture = hunted();
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 1, ENEMY_ZAP).unwrap();
        let offered = labels(&ctx);
        assert!(
            offered.contains(&format!("{{card {}}}", fixtures::VI)),
            "seat 0's other unit is fair game: {offered:?}"
        );
        assert!(
            !offered.contains(&format!("{{card {AKALI}}}")),
            "Akali is not offered to the enemy: {offered:?}"
        );
        assert_eq!(
            playing::choose_targets(&mut ctx, 1, 0, &[AKALI]),
            Err(Refusal::Illegal(Reason::NotALegalTarget)),
            "and naming her outright is refused"
        );
        assert!(ctx.blob.prompt.is_some(), "the prompt stays open");
        let ability = ChainItem::new(
            99,
            ItemKind::Trigger {
                source: fixtures::THEIR_UNIT,
                index: 0,
            },
            1,
            Origin::Board,
        );
        let offered = targets::candidates(&ctx, &ability, &a_unit("a unit"));
        assert!(
            offered.contains(&TargetRef::Card(fixtures::VI)),
            "an enemy ability reaches seat 0's other unit: {offered:?}"
        );
        assert!(
            !offered.contains(&TargetRef::Card(AKALI)),
            "and abilities are as silent as spells: {offered:?}"
        );
        assert!(!ctx.in_combat(AKALI));
    }

    #[test]
    fn an_enemy_spell_that_chose_her_in_combat_loses_its_target_when_she_leaves() {
        let mut fixture = hunted();
        let mut ctx = fixture.ctx();
        ctx.table.card_mut(AKALI).unwrap().zone = Some(fixtures::BF2);
        assert!(ctx.mark_attacker(AKALI));
        let mut item = ChainItem::new(1, ItemKind::Spell { card: ENEMY_ZAP }, 1, Origin::Hand);
        let spec = a_unit("a unit");
        assert!(
            targets::candidates(&ctx, &item, &spec).contains(&TargetRef::Card(AKALI)),
            "in combat she is a legal choice"
        );
        item.targets.push(TargetRef::Card(AKALI));
        assert!(targets::valid(&ctx, &item, 0));
        ctx.clear_designation(AKALI);
        assert!(
            !targets::candidates(&ctx, &item, &spec).contains(&TargetRef::Card(AKALI)),
            "out of combat she is offered to nobody"
        );
        assert!(
            !targets::valid(&ctx, &item, 0),
            "356.3.e.2 · the chosen target is illegal at resolution too"
        );
        assert_eq!(card_target(&ctx, &item, 0), None);
    }

    #[test]
    fn an_enemy_choice_taken_while_it_resolves_cannot_name_her_either() {
        let mut fixture = hunted();
        fixture.scripts = fixture.scripts.clone().with_script(ENEMY_ZAP, &PICKER);
        let mut item = ChainItem::new(1, ItemKind::Spell { card: ENEMY_ZAP }, 1, Origin::Hand);
        item.status = crate::state::ItemStatus::Resolving;
        fixture.blob.chain.push(item);
        fixture.blob.open_prompt(crate::state::Ask {
            prompt: agni_plugin_sdk::prompt::Prompt::new(2, 1, 1, 1),
            why: crate::state::PromptWhy::Resume { item: 1, stage: 1 },
        });
        let mut ctx = fixture.ctx();
        let offered = labels(&ctx);
        assert!(
            offered.contains(&format!("{{card {}}}", fixtures::VI)),
            "the enemy still reaches seat 0's other unit while resolving: {offered:?}"
        );
        assert!(
            !offered.contains(&format!("{{card {AKALI}}}")),
            "352.9's non-target choice is still a choice her text refuses: {offered:?}"
        );
        ctx.table.card_mut(AKALI).unwrap().zone = Some(fixtures::BF2);
        assert!(ctx.mark_attacker(AKALI));
        assert!(
            labels(&ctx).contains(&format!("{{card {AKALI}}}")),
            "in combat the same choice reaches her: {:?}",
            labels(&ctx)
        );
    }

    #[test]
    fn her_own_controllers_spell_chooses_her_out_of_combat() {
        let mut fixture = shadowed();
        fixture.resolve();
        fixture.scripts = fixture
            .scripts
            .clone()
            .with_script(fixtures::HAND_SPELL, &ZAP);
        let mut ctx = fixture.ctx();
        play_from_hand(&mut ctx, 0, fixtures::HAND_SPELL).unwrap();
        assert!(labels(&ctx).contains(&format!("{{card {AKALI}}}")));
        choose(&mut ctx, 0, &format!("{{card {AKALI}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(AKALI)]);
        resolve(&mut ctx, 0);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: AKALI,
            n: 1,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(AKALI), 1);
        assert!(ctx.on_board(AKALI));
    }

    #[test]
    fn an_enemy_spell_chooses_akali_once_she_is_designated_in_combat() {
        let mut fixture = hunted();
        let mut ctx = fixture.ctx();
        ctx.table.card_mut(AKALI).unwrap().zone = Some(fixtures::BF2);
        assert!(ctx.mark_attacker(AKALI));
        assert!(ctx.in_combat(AKALI));
        play_from_hand(&mut ctx, 1, ENEMY_ZAP).unwrap();
        assert!(
            labels(&ctx).contains(&format!("{{card {AKALI}}}")),
            "in combat she is choosable: {:?}",
            labels(&ctx)
        );
        choose(&mut ctx, 1, &format!("{{card {AKALI}}}")).unwrap();
        assert_eq!(ctx.blob.chain[0].targets, [TargetRef::Card(AKALI)]);
        resolve(&mut ctx, 1);
        assert!(ctx.events.contains(&Event::DamageDealt {
            card: AKALI,
            n: 1,
            source: Cause::Item(1)
        }));
        assert_eq!(ctx.damage_on(AKALI), 1);
        ctx.clear_designation(AKALI);
        assert!(!ctx.in_combat(AKALI));
    }
}
