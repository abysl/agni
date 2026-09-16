use super::*;
use crate::proto::{UndoProposal, UndoStatus};
use std::collections::VecDeque;

const MAX_UNDO_ACTIONS: usize = 64;

pub(super) struct Checkpoint {
    pub log_len: usize,
    state: LogState,
    view: TableView,
    dealer: BTreeMap<u32, CardFace>,
    hidden: BTreeMap<u32, HiddenPlay>,
    next_card_id: u32,
}

#[derive(Default)]
pub(super) struct UndoHistory {
    checkpoints: VecDeque<Checkpoint>,
    revision: u64,
    next_request: u64,
    proposal: Option<UndoProposal>,
}

impl UndoHistory {
    pub fn cancel(&mut self) {
        self.proposal = None;
        self.revision += 1;
    }

    pub fn changed(&mut self, action: &LogAction) {
        self.cancel();
        if matches!(
            action,
            LogAction::Genesis { .. }
                | LogAction::Join { .. }
                | LogAction::Deal { .. }
                | LogAction::Clear { .. }
                | LogAction::Reset
        ) {
            self.checkpoints.clear();
        }
    }

    pub fn remember(&mut self, checkpoint: Checkpoint) {
        self.checkpoints.push_back(checkpoint);
        while self.checkpoints.len() > MAX_UNDO_ACTIONS {
            self.checkpoints.pop_front();
        }
    }
}

fn refused(reason: &str) -> SessionError {
    FoldError::Rejected {
        reason: Some(reason.into()),
    }
    .into()
}

impl HostSession {
    pub(super) fn undo_checkpoint(&mut self) -> Result<Checkpoint, SessionError> {
        Ok(Checkpoint {
            log_len: self.log.len(),
            state: self.state_cache.clone(),
            view: self.mirror.clone(),
            dealer: self.dealer.clone(),
            hidden: self.hidden_plays.clone(),
            next_card_id: self.next_card_id,
        })
    }

    pub fn undo_status(&self) -> UndoStatus {
        UndoStatus {
            revision: self.undo.revision,
            available: self.undo.checkpoints.len() as u32,
            proposal: self.undo.proposal.clone(),
        }
    }

    pub fn request_undo(
        &mut self,
        from: u8,
        actions: u32,
        revision: u64,
    ) -> Result<bool, SessionError> {
        self.seated(from)?;
        if self.undo.proposal.is_some() {
            return Err(refused("an undo request is already waiting for agreement"));
        }
        if revision != self.undo.revision {
            return Err(refused("the table changed; request undo again"));
        }
        if actions == 0 || actions as usize > self.undo.checkpoints.len() {
            return Err(refused("that many actions are not available to undo"));
        }
        if self.roster.iter().any(|seat| !seat.connected) {
            return Err(refused("all players must be connected to agree to undo"));
        }
        let waiting = self
            .roster
            .iter()
            .filter(|seat| seat.seat != from)
            .map(|seat| seat.seat)
            .collect();
        self.undo.next_request += 1;
        self.undo.proposal = Some(UndoProposal {
            id: self.undo.next_request,
            requester: from,
            actions,
            waiting,
        });
        self.finish_undo()
    }

    pub fn vote_undo(&mut self, from: u8, id: u64, accept: bool) -> Result<bool, SessionError> {
        self.seated(from)?;
        let Some(proposal) = self.undo.proposal.as_mut() else {
            return Err(refused("that undo request is no longer active"));
        };
        if proposal.id != id || !proposal.waiting.contains(&from) {
            return Err(refused("this seat cannot answer that undo request"));
        }
        if !accept {
            self.undo.cancel();
            return Ok(false);
        }
        proposal.waiting.retain(|seat| *seat != from);
        self.finish_undo()
    }

    fn finish_undo(&mut self) -> Result<bool, SessionError> {
        let Some(proposal) = &self.undo.proposal else {
            return Ok(false);
        };
        if !proposal.waiting.is_empty() {
            return Ok(false);
        }
        let index = self.undo.checkpoints.len() - proposal.actions as usize;
        let checkpoint = &self.undo.checkpoints[index];
        self.engine
            .restore(&agni_sim::log::encode_state(&checkpoint.state))?;
        let checkpoint = self
            .undo
            .checkpoints
            .remove(index)
            .expect("validated undo target");
        self.undo.checkpoints.truncate(index);
        self.log.truncate(checkpoint.log_len);
        self.state_cache = checkpoint.state;
        self.mirror = checkpoint.view;
        self.dealer = checkpoint.dealer;
        self.hidden_plays = checkpoint.hidden;
        self.next_card_id = checkpoint.next_card_id;
        self.sent_faces.clear();
        self.undo.cancel();
        Ok(true)
    }
}
