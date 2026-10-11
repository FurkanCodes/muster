//! Runs in the UI (`PRODUCT.md` §6): what the person can do to the run in
//! focus, the dialogs that ask them what to tell its agent, its timeline
//! kept current while it is shown, and news when one comes to need them.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::protocol::Command;
use muster_core::{Run, RunId, RunStage};

use super::{Action, App, Confirmed, Dialog, IssueWork, Mode, Then, View};
use crate::input::{Edit, LineInput};

/// What the person can do to a run at its stage. The daemon decides; this
/// only keeps the keys offered to those that can apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RunActions {
    pub(crate) take_over: bool,
    pub(crate) hand_back: bool,
    pub(crate) retry: bool,
    pub(crate) changes: bool,
    pub(crate) cancel: bool,
}

impl RunActions {
    pub(crate) fn of(stage: RunStage) -> Self {
        Self {
            take_over: matches!(
                stage,
                RunStage::Building
                    | RunStage::Checking
                    | RunStage::AgentReview
                    | RunStage::ReadyForYou
                    | RunStage::NeedsYou
            ),
            hand_back: stage == RunStage::TakenOver,
            retry: stage == RunStage::NeedsYou,
            changes: stage == RunStage::ReadyForYou,
            cancel: !stage.is_over(),
        }
    }
}

impl App {
    /// A key for the run in focus; `None` if the key means nothing for runs,
    /// so it goes on to do what it does elsewhere.
    pub(super) fn run_key(&mut self, run: RunId, key: &KeyEvent) -> Option<Vec<Action>> {
        let found = self.model.run(run)?;
        let (number, stage) = (found.ticket.number, found.stage);
        let can = RunActions::of(stage);
        let allowed = match key.code {
            KeyCode::Char('t') => can.take_over,
            KeyCode::Char('h') => can.hand_back,
            KeyCode::Char('r') => can.retry,
            KeyCode::Char('c') => can.changes,
            KeyCode::Char('x') => can.cancel,
            _ => return None,
        };
        if !allowed {
            self.complain(format!(
                "Not while the run on #{number} is {}.",
                stage.phrase()
            ));
            return Some(Vec::new());
        }
        let actions = match key.code {
            KeyCode::Char('t') => {
                self.say(format!("Taking the run on #{number} over…"));
                vec![self.command(Command::TakeOverRun { run }, Then::OpenTakenOver)]
            }
            KeyCode::Char('h') => {
                self.mode = Mode::Dialog(Dialog::Confirm {
                    question: format!(
                        "Hand the run on #{number} back? The sessions in its container stop, \
                         and the checks run on your work."
                    ),
                    then: Confirmed::HandBackRun(run),
                });
                Vec::new()
            }
            KeyCode::Char('r') => {
                self.mode = Mode::Dialog(Dialog::RunNotes {
                    run,
                    notes: LineInput::default(),
                });
                Vec::new()
            }
            KeyCode::Char('c') => {
                self.mode = Mode::Dialog(Dialog::RunChanges {
                    run,
                    comments: LineInput::default(),
                });
                Vec::new()
            }
            _ => {
                self.mode = Mode::Dialog(Dialog::Confirm {
                    question: format!(
                        "Cancel the run on #{number}? Its container goes; its record and \
                         pull request stay."
                    ),
                    then: Confirmed::CancelRun(run),
                });
                Vec::new()
            }
        };
        Some(actions)
    }

    /// ⏎ on a run: its agent's terminal, once the person has taken it over.
    pub(super) fn open_run(&mut self, run: RunId) {
        match self.model.agent_session_of(run) {
            Some(session) => {
                self.mode = Mode::Terminal(session);
                self.note = None;
            }
            None => {
                let can = self
                    .model
                    .run(run)
                    .is_some_and(|found| RunActions::of(found.stage).take_over);
                if can {
                    self.say("Press t to take the run over; its agent is then yours here.");
                } else {
                    self.say("The run's agent is its own; nothing to open yet.");
                }
            }
        }
    }

    /// The issues dialog, for handing one of the selected project's issues
    /// to the autopilot.
    pub(super) fn issues_for_a_run(&mut self) -> Vec<Action> {
        let project = self
            .focused_run()
            .and_then(|run| self.model.run(run))
            .map(|run| run.project)
            .or_else(|| self.selected.and_then(|row| self.model.project_of(row)));
        let actions = self.open_issues_of(project);
        if let Mode::Dialog(Dialog::Issues { work, .. }) = &mut self.mode {
            *work = IssueWork::Autopilot;
        }
        actions
    }

    /// Keys in the dialogs that ask what to tell a run's agent.
    pub(super) fn run_dialog_key(&mut self, dialog: Dialog, key: &KeyEvent) -> Vec<Action> {
        match dialog {
            Dialog::RunNotes { run, mut notes } => match notes.handle(key) {
                Edit::Submit => {
                    self.mode = Mode::Browse;
                    let notes = Some(notes.text().trim().to_owned()).filter(|n| !n.is_empty());
                    let command = Command::RetryRun { run, notes };
                    vec![self.command(command, Then::Say("Sent the run back to work."))]
                }
                edit => self.keep_editing(edit, Dialog::RunNotes { run, notes }),
            },
            Dialog::RunChanges { run, mut comments } => {
                match comments.handle(key) {
                    Edit::Submit if comments.text().trim().is_empty() => {
                        self.complain("Say what to change: the agent gets nothing else.");
                        self.mode = Mode::Dialog(Dialog::RunChanges { run, comments });
                        Vec::new()
                    }
                    Edit::Submit => {
                        self.mode = Mode::Browse;
                        let comments = comments.text().trim().to_owned();
                        let command = Command::RequestChanges { run, comments };
                        vec![self.command(
                        command,
                        Then::Say("Sent the run back to its agent; its pull request is a draft again."),
                    )]
                    }
                    edit => self.keep_editing(edit, Dialog::RunChanges { run, comments }),
                }
            }
            other => {
                self.mode = Mode::Dialog(other);
                Vec::new()
            }
        }
    }

    /// Shows the run a reply named in the runs list, once it is there.
    pub(super) fn take_wanted_run(&mut self) {
        let Some(run) = self.wanted_run else {
            return;
        };
        if !self.runs_list.contains(&run) {
            return;
        }
        self.wanted_run = None;
        self.view = View::Runs;
        self.runs_selected = Some(run);
    }

    /// Asks for the timeline of the run in focus, when it is new to the
    /// screen or has moved since it was asked for.
    pub(super) fn ask_history(&mut self) -> Option<Action> {
        let run = self.focused_run()?;
        let found = self.model.run(run)?;
        let key = (run, found.stage, found.attempt);
        if self.history_asked == Some(key) {
            return None;
        }
        self.history_asked = Some(key);
        Some(self.command(Command::RunHistory { run }, Then::RunHistory(run)))
    }

    /// A notification for a run that has just come to wait on the person,
    /// if they are looking elsewhere and want to be told.
    pub(super) fn run_news(&self, run: RunId) -> Option<Action> {
        if !self.notify || self.focused != Some(false) {
            return None;
        }
        let run = self.model.run(run)?;
        Some(Action::Notify(run_waits(run)?))
    }
}

/// What a run waiting on the person says, in a notification.
fn run_waits(run: &Run) -> Option<String> {
    let number = run.ticket.number;
    match run.stage {
        RunStage::NeedsYou => Some(format!(
            "The run on #{number} needs you: {}",
            run.reason.as_deref().unwrap_or("it stopped")
        )),
        RunStage::ReadyForYou => Some(match &run.pr {
            Some(pr) => format!(
                "The run on #{number} is ready for you: pull request #{}",
                pr.number
            ),
            None => format!("The run on #{number} is ready for you"),
        }),
        _ => None,
    }
}
