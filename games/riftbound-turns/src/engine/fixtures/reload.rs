use super::{choose, Fixture};
use crate::engine::ctx::Ctx;
use crate::engine::{priority, settle, showdown};
use crate::state::GameBlob;

const MAX_RESOLUTION_ACTIONS: usize = 64;

impl Fixture {
    pub fn act_and_reload<R>(&mut self, action: impl FnOnce(&mut Ctx) -> R) -> R {
        let (result, table) = {
            let mut ctx = self.ctx();
            let result = action(&mut ctx);
            settle(&mut ctx).unwrap();
            assert!(ctx.fault.is_none(), "{:?}", ctx.fault);
            (result, ctx.table)
        };
        self.blob = GameBlob::decode(&self.blob.encode()).expect("fixture blob reloads");
        self.commit(table);
        result
    }

    pub fn choose_and_reload(&mut self, seat: u8, label: &str) {
        self.act_and_reload(|ctx| choose(ctx, seat, label).unwrap());
    }

    pub fn resolve_and_reload(&mut self, mut choose_prompt: impl FnMut(&Ctx) -> String) {
        for _ in 0..MAX_RESOLUTION_ACTIONS {
            let ctx = self.ctx();
            if let Some(prompt) = &ctx.blob.prompt {
                let seat = prompt.seat;
                let label = choose_prompt(&ctx);
                drop(ctx);
                self.choose_and_reload(seat, &label);
            } else if let Some(seat) = priority::holder(&ctx) {
                drop(ctx);
                self.act_and_reload(|ctx| priority::pass(ctx, seat).unwrap());
            } else if let Some(combat) = &ctx.blob.showdown {
                let seat = combat.window.focus;
                drop(ctx);
                self.act_and_reload(|ctx| showdown::pass(ctx, seat).unwrap());
            } else {
                assert!(ctx.blob.chain.is_empty());
                assert!(ctx.blob.queue.is_empty());
                return;
            }
        }
        panic!("fixture did not resolve after {MAX_RESOLUTION_ACTIONS} actions");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cards::{honest_broker, TOKEN_GOLD};
    use crate::engine::ctx::{Cause, Killed};
    use crate::engine::fixtures;
    use crate::state::PromptWhy;

    const SOURCE: u32 = 90;

    #[test]
    fn each_action_commits_effects_and_rebuilds_scripts_after_a_blob_roundtrip() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            SOURCE,
            fixtures::BASE,
            0,
            honest_broker::CARD.name,
            2,
        ));
        fixture.table.tokens.push(SOURCE);
        fixture.resolve();
        assert!(fixture.scripts.of_card(SOURCE).is_some());
        assert_eq!(
            fixture.act_and_reload(|ctx| ctx.kill(SOURCE, Cause::Rule)),
            Killed::Yes
        );
        assert!(fixture.table.card(SOURCE).is_none());
        assert!(fixture.scripts.of_card(SOURCE).is_none());
        assert_eq!(
            fixture.blob.chain[0].ability_script.as_deref(),
            Some(honest_broker::CARD.name)
        );
        fixture.resolve_and_reload(|ctx| panic!("unexpected prompt: {:?}", ctx.blob.why));
        assert_eq!(
            fixture
                .table
                .cards
                .iter()
                .filter(|card| card.name == TOKEN_GOLD)
                .count(),
            1
        );
    }

    #[test]
    fn resolution_uses_the_supplied_choices_across_resumed_prompts() {
        let mut fixture = Fixture::enforced();
        fixture.table.cards.push(fixtures::unit(
            SOURCE,
            fixtures::BASE,
            0,
            "Undercover Agent",
            2,
        ));
        fixture.resolve();
        let hand = fixture.ctx().hand_of(0).len();
        fixture.act_and_reload(|ctx| ctx.kill(SOURCE, Cause::Rule));
        let discards = [fixtures::HAND_GEAR, fixtures::HAND_UNIT];
        let mut chosen = 0;
        fixture.resolve_and_reload(|ctx| {
            assert!(matches!(ctx.blob.why, Some(PromptWhy::Discard { .. })));
            let label = format!("{{card {}}}", discards[chosen]);
            chosen += 1;
            label
        });
        assert_eq!(chosen, discards.len());
        for card in discards {
            assert_eq!(
                fixture.table.card(card).unwrap().zone,
                Some(fixtures::TRASH)
            );
        }
        assert_eq!(fixture.ctx().hand_of(0).len(), hand);
        assert_eq!(fixture.blob.seat(0).draws, 2);
    }
}
