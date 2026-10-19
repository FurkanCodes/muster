//! The workspace's timeline in the changes pane (`PRODUCT.md` §6): each
//! agent's turn there, what changed between turns, and each undoing,
//! newest first. ⏎ shows what one changed; `z` asks what putting the files
//! back as they were before it would do, and does it once the person says
//! yes, with the branch too if they ask (`PRD-14`).

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::WorkspaceId;
use muster_core::protocol::Command;
use muster_core::timeline::{Entry, EntryKind, Timeline, UndoPlan, turn_of_session};

use super::changes::ChangeRow;
use super::{Action, App, Dialog, Mode, Then};

/// The timeline's lines in the pane, after the change's.
pub(super) fn rows(timeline: Option<&Result<Timeline, String>>) -> Vec<ChangeRow<'_>> {
    match timeline {
        None => Vec::new(),
        Some(Err(error)) => vec![ChangeRow::Error(format!(
            "The timeline could not be read: {error}"
        ))],
        Some(Ok(timeline)) if timeline.entries.is_empty() => vec![
            ChangeRow::Heading("Turns".to_owned()),
            ChangeRow::Note("None kept yet: each agent's turn here is, from now on.".to_owned()),
        ],
        Some(Ok(timeline)) => {
            let mut rows = vec![ChangeRow::Heading(format!(
                "Turns · {}",
                timeline.entries.len()
            ))];
            rows.extend(timeline.entries.iter().map(ChangeRow::Entry));
            rows
        }
    }
}

/// What an entry was, in a few words: `Claude Code · turn 3`.
pub(crate) fn label(timeline: Option<&Timeline>, entry: &Entry) -> String {
    match &entry.kind {
        EntryKind::Turn { agent, .. } => {
            match timeline.and_then(|timeline| turn_of_session(timeline, entry)) {
                Some(turn) => format!("{agent} · turn {turn}"),
                None => agent.clone(),
            }
        }
        EntryKind::Between => "Edits between turns".to_owned(),
        EntryKind::Undo { of } => format!("Put back as before #{of}"),
        EntryKind::Moved { onto } => format!("Moved onto {}", muster_core::text::one_line(onto)),
    }
}

impl App {
    /// Asks for the open pane's timeline, from a daemon that keeps them,
    /// unless an answer is on its way.
    pub(super) fn ask_timeline(&mut self) -> Option<Action> {
        if !self.knows_timelines() {
            return None;
        }
        let pane = self.changes.as_mut()?;
        if pane.asking_timeline {
            return None;
        }
        pane.asking_timeline = true;
        let workspace = pane.workspace;
        Some(self.command(Command::Timeline { workspace }, Then::Timeline(workspace)))
    }

    /// `z`: asks what putting the files back from the selected entry would
    /// do, to ask the person.
    pub(super) fn ask_undo_plan(&mut self) -> Option<Action> {
        if !self.knows_timelines() {
            self.complain(
                "The daemon runs an older muster that keeps no turns: `muster reload` moves it \
                 onto this one, every session running on.",
            );
            return None;
        }
        let pane = self.changes.as_ref()?;
        let Some(number) = pane.selected_entry() else {
            self.complain(
                "Choose a turn under Turns to put the files back as they were before it.",
            );
            return None;
        };
        let workspace = pane.workspace;
        Some(self.command(
            Command::UndoPlan { workspace, number },
            Then::UndoPlan(workspace),
        ))
    }

    /// The daemon said what an undo would do: the person is asked.
    pub(super) fn on_undo_plan(
        &mut self,
        workspace: WorkspaceId,
        outcome: Result<UndoPlan, String>,
    ) {
        let plan = match outcome {
            Ok(plan) => plan,
            Err(error) => {
                self.complain(error);
                return;
            }
        };
        let Some(pane) = self
            .changes
            .as_ref()
            .filter(|pane| pane.workspace == workspace && self.mode == Mode::Changes)
        else {
            return;
        };
        let timeline = match &pane.timeline {
            Some(Ok(timeline)) => Some(timeline),
            _ => None,
        };
        let what = timeline
            .and_then(|timeline| {
                let entry = timeline
                    .entries
                    .iter()
                    .find(|entry| entry.number == plan.number)?;
                Some(label(Some(timeline), entry))
            })
            .unwrap_or_else(|| format!("#{}", plan.number));
        let agent_remembers = timeline
            .and_then(|timeline| {
                timeline
                    .entries
                    .iter()
                    .find(|entry| entry.number == plan.number)
            })
            .and_then(|entry| match &entry.kind {
                EntryKind::Turn { agent, .. } => Some(agent.clone()),
                _ => None,
            });
        self.mode = Mode::Dialog(Dialog::Undo {
            workspace,
            plan,
            what,
            agent_remembers,
        });
    }

    /// A key in the dialog asking whether to put the files back.
    pub(super) fn undo_key(
        &mut self,
        workspace: WorkspaceId,
        plan: &UndoPlan,
        key: &KeyEvent,
    ) -> Vec<Action> {
        let move_branch = match key.code {
            KeyCode::Char('y') | KeyCode::Enter => false,
            KeyCode::Char('b')
                if plan
                    .branch
                    .as_ref()
                    .is_some_and(|branch| branch.refused.is_none()) =>
            {
                true
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.mode = Mode::Changes;
                return Vec::new();
            }
            _ => return Vec::new(),
        };
        self.mode = Mode::Changes;
        self.say("Putting the files back…");
        vec![self.command(
            Command::Undo {
                workspace,
                number: plan.number,
                move_branch,
            },
            Then::Undone(workspace, plan.number),
        )]
    }

    /// The files are back, or why not.
    pub(super) fn on_undone(
        &mut self,
        workspace: WorkspaceId,
        number: u64,
        outcome: Result<(u64, bool), String>,
    ) -> Vec<Action> {
        let (entry, moved) = match outcome {
            Ok(done) => done,
            Err(error) => {
                self.complain(error);
                return Vec::new();
            }
        };
        let branch = if moved {
            ", and the branch with them"
        } else {
            ""
        };
        self.say(format!(
            "The files are as they were before #{number}{branch}. {} on #{entry} brings them back.",
            self.key(muster_core::keys::Action::PutBack)
        ));
        let Some(pane) = self
            .changes
            .as_mut()
            .filter(|pane| pane.workspace == workspace)
        else {
            return Vec::new();
        };
        pane.diff = None;
        pane.selected = Some(super::changes::Pick::Entry(entry));
        let mut actions: Vec<Action> = self.ask_changes().into_iter().collect();
        actions.extend(self.ask_timeline());
        actions
    }
}

#[cfg(test)]
#[path = "timeline_tests.rs"]
mod tests;
