//! What keys and pastes do in each mode.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use muster_core::ProjectId;
use muster_core::protocol::{ClientFrame, Command};

use super::{Action, App, Confirmed, Dialog, HarnessChoice, IssueWork, Mode, Then, View};
use crate::input::{Edit, LineInput};
use crate::keys::{encode_key, encode_paste};
use crate::model::Row;

/// Whether `key` is Ctrl+], which leaves a session's terminal. It reaches
/// no common program, which is why telnet chose it too. A terminal without
/// the kitty keyboard protocol sends it as byte 0x1d, which crossterm
/// reports as Ctrl+5, so both are accepted.
pub(crate) fn leaves_terminal(key: &KeyEvent) -> bool {
    key.modifiers.contains(KeyModifiers::CONTROL)
        && matches!(key.code, KeyCode::Char(']') | KeyCode::Char('5'))
}

impl App {
    pub(crate) fn on_key(&mut self, key: KeyEvent) -> Vec<Action> {
        // Repeats count as presses; releases (sent by some terminals) do not.
        if key.kind == KeyEventKind::Release {
            return Vec::new();
        }
        let mut actions = match self.mode.clone() {
            Mode::Terminal(session) => return self.terminal_key(session, &key),
            Mode::Dialog(dialog) => self.dialog_key(dialog, &key),
            Mode::Browse => self.browse_key(&key),
        };
        actions.extend(self.settle());
        actions
    }

    pub(crate) fn on_paste(&mut self, text: &str) -> Vec<Action> {
        match &mut self.mode {
            Mode::Terminal(session) => {
                let session = *session;
                let screen = self.screen(session);
                screen.scroll_to_bottom();
                vec![Action::Send(ClientFrame::Input {
                    session,
                    data: encode_paste(text, screen.modes),
                })]
            }
            Mode::Dialog(Dialog::AddProject { path: input })
            | Mode::Dialog(Dialog::NewWorktree { branch: input, .. })
            | Mode::Dialog(Dialog::NewSession { prompt: input, .. }) => {
                input.paste(text);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn terminal_key(&mut self, session: muster_core::SessionId, key: &KeyEvent) -> Vec<Action> {
        self.note = None;
        if leaves_terminal(key) {
            self.screen(session).scroll_to_bottom();
            self.mode = Mode::Browse;
            return self.settle();
        }
        // Claude Code suspends itself on Ctrl+Z and waits for a shell's `fg`;
        // a session has no shell above it, so the key would only freeze it.
        let is_claude = self
            .model
            .session(session)
            .is_some_and(|stored| stored.harness == "claude");
        if is_claude
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('z')
        {
            self.complain("Ctrl+Z would freeze Claude Code here, so it was not sent. Ctrl-] goes back to the list.");
            return Vec::new();
        }
        let screen = self.screen(session);
        let modes = screen.modes;
        let data = encode_key(key, modes);
        if data.is_some() {
            // Typing returns to the newest output, as in a terminal.
            screen.scroll_to_bottom();
        }
        data.map(|data| Action::Send(ClientFrame::Input { session, data }))
            .into_iter()
            .collect()
    }

    fn browse_key(&mut self, key: &KeyEvent) -> Vec<Action> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if !control
            && let Some(run) = self.focused_run()
            && let Some(actions) = self.run_key(run, key)
        {
            return actions;
        }
        if self.view == View::Runs && matches!(key.code, KeyCode::Char('n' | 'i')) {
            return self.issues_for_a_run();
        }
        match key.code {
            KeyCode::Char('c') if control => return vec![Action::Quit],
            KeyCode::Char('q') => return vec![Action::Quit],
            KeyCode::Tab | KeyCode::BackTab => {
                let back = key.code == KeyCode::BackTab;
                self.view = match (self.view, back) {
                    (View::Sessions, false) | (View::Runs, true) => {
                        // The queue opens on its most urgent entry.
                        self.needs_selected = self.needs_list.first().copied();
                        View::NeedsYou
                    }
                    (View::NeedsYou, false) | (View::Sessions, true) => {
                        // As the queue, the runs open on what most waits on
                        // the person.
                        self.runs_selected = self.runs_list.first().copied();
                        View::Runs
                    }
                    (View::Runs, false) | (View::NeedsYou, true) => View::Sessions,
                };
            }
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.page_preview(true),
            KeyCode::PageDown => self.page_preview(false),
            KeyCode::End => self.preview_to_bottom(),
            KeyCode::Enter => self.open_selected(),
            KeyCode::Char('a') => {
                self.mode = Mode::Dialog(Dialog::AddProject {
                    path: LineInput::with(&self.cwd.display().to_string()),
                });
            }
            KeyCode::Char('w') => self.new_worktree(),
            KeyCode::Char('i') => return self.open_issues(),
            KeyCode::Char('n') => self.new_session(),
            KeyCode::Char('s') => self.confirm_stop(),
            KeyCode::Char('r') => return self.restart(),
            KeyCode::Char('x') => return self.remove(),
            _ => {}
        }
        Vec::new()
    }

    pub(super) fn move_selection(&mut self, by: isize) {
        let step = |list_len: usize, at: Option<usize>| -> Option<usize> {
            if list_len == 0 {
                return None;
            }
            let at = at.unwrap_or(0) as isize + by;
            Some(at.clamp(0, list_len as isize - 1) as usize)
        };
        match self.view {
            View::Sessions => {
                let rows = self.model.rows();
                let at = self
                    .selected
                    .and_then(|row| rows.iter().position(|r| *r == row));
                self.selected = step(rows.len(), at).map(|index| rows[index]);
            }
            View::NeedsYou => {
                let list = &self.needs_list;
                let at = self
                    .needs_selected
                    .and_then(|need| list.iter().position(|n| *n == need));
                self.needs_selected = step(list.len(), at).map(|index| list[index]);
            }
            View::Runs => {
                let list = &self.runs_list;
                let at = self
                    .runs_selected
                    .and_then(|run| list.iter().position(|r| *r == run));
                self.runs_selected = step(list.len(), at).map(|index| list[index]);
            }
        }
    }

    pub(super) fn open_selected(&mut self) {
        if let Some(session) = self.focused_session() {
            self.mode = Mode::Terminal(session);
            self.note = None;
            return;
        }
        if let Some(run) = self.focused_run() {
            self.open_run(run);
            return;
        }
        match self.selected {
            Some(Row::Worktree(_)) => self.new_session(),
            Some(Row::Project(_)) => self.new_worktree(),
            _ => {}
        }
    }

    fn new_worktree(&mut self) {
        match self.selected.and_then(|row| self.model.project_of(row)) {
            Some(project) => {
                self.mode = Mode::Dialog(Dialog::NewWorktree {
                    project,
                    branch: LineInput::default(),
                });
            }
            None => self.complain("Add a project first: press a."),
        }
    }

    fn open_issues(&mut self) -> Vec<Action> {
        let project = self.selected.and_then(|row| self.model.project_of(row));
        self.open_issues_of(project)
    }

    /// The issues dialog for `project`, loading its issues.
    pub(super) fn open_issues_of(&mut self, project: Option<ProjectId>) -> Vec<Action> {
        let Some(project) = project else {
            self.complain("Add a project first: press a.");
            return Vec::new();
        };
        self.mode = Mode::Dialog(Dialog::Issues {
            project,
            issues: None,
            selected: 0,
            work: IssueWork::Session(HarnessChoice::Claude),
            error: None,
        });
        vec![self.command(Command::ListIssues { project }, Then::ShowIssues(project))]
    }

    fn new_session(&mut self) {
        let workspace = match self.view {
            // On a project, its own checkout.
            View::Sessions => self.selected.and_then(|row| match row {
                Row::Project(project) => self.model.checkout_of(project),
                row => self.model.worktree_of(row),
            }),
            View::NeedsYou => self
                .focused_session()
                .and_then(|session| self.model.worktree_of(Row::Session(session))),
            View::Runs => None,
        };
        match workspace {
            Some(workspace) => {
                // In a run's container, Claude Code is the run's own; a
                // shell can look around.
                let in_container = self
                    .model
                    .workspace(workspace)
                    .is_some_and(|found| found.kind.run().is_some());
                self.mode = Mode::Dialog(Dialog::NewSession {
                    workspace,
                    harness: if in_container {
                        HarnessChoice::Shell
                    } else {
                        HarnessChoice::Claude
                    },
                    prompt: LineInput::default(),
                });
            }
            None => self.complain("Select a worktree first, or create one: press w."),
        }
    }

    fn confirm_stop(&mut self) {
        let Some(session) = self.focused_session() else {
            return self.complain("Select a session to stop.");
        };
        let Some(stored) = self.model.session(session) else {
            return;
        };
        if stored.ended_at.is_some() {
            return self.complain("That session has already ended.");
        }
        self.mode = Mode::Dialog(Dialog::Confirm {
            question: format!("Stop {}?", stored.title),
            then: Confirmed::StopSession(session),
        });
    }

    /// Starts the focused session's program again, if it has ended.
    fn restart(&mut self) -> Vec<Action> {
        let Some(session) = self.focused_session() else {
            self.complain("Select a session to restart.");
            return Vec::new();
        };
        let Some(stored) = self.model.session(session) else {
            return Vec::new();
        };
        if stored.ended_at.is_none() {
            self.complain(format!("{} is still running.", stored.title));
            return Vec::new();
        }
        let command = Command::RestartSession {
            session,
            size: self.terminal_size,
        };
        vec![self.command(command, Then::OpenRestarted)]
    }

    /// Removes what is selected: an ended session from the list at once,
    /// a worktree or a project after asking.
    fn remove(&mut self) -> Vec<Action> {
        if self.view == View::Sessions
            && let Some(Row::Project(project)) = self.selected
        {
            let name = self
                .model
                .project(project)
                .map_or("this project", |found| found.name.as_str());
            self.mode = Mode::Dialog(Dialog::Confirm {
                question: format!(
                    "Stop tracking {name}? Muster forgets it; its repository and files are not touched."
                ),
                then: Confirmed::RemoveProject(project),
            });
            return Vec::new();
        }
        if let Some(session) = self.focused_session() {
            let Some(stored) = self.model.session(session) else {
                return Vec::new();
            };
            if stored.ended_at.is_none() {
                self.complain(format!(
                    "{} is still running; stop it first with s.",
                    stored.title
                ));
                return Vec::new();
            }
            let command = Command::RemoveSession { session };
            return vec![self.command(command, Then::Say("Removed the session."))];
        }
        self.confirm_remove_worktree();
        Vec::new()
    }

    fn confirm_remove_worktree(&mut self) {
        let Some(workspace) = self.selected.and_then(|row| self.model.worktree_of(row)) else {
            return self.complain("Select a worktree or an ended session to remove.");
        };
        if self
            .model
            .workspace(workspace)
            .is_some_and(|found| found.kind.is_checkout())
        {
            return self.complain(
                "That is the project's own checkout, which Muster never removes; select the project to stop tracking it.",
            );
        }
        if self.model.run_of(workspace).is_some() {
            return self.complain(
                "That is a run's container, which goes when the run ends; cancel the run to end it.",
            );
        }
        let branch = self
            .model
            .branch(workspace)
            .unwrap_or("this worktree")
            .to_owned();
        self.mode = Mode::Dialog(Dialog::Confirm {
            question: format!("Remove the worktree for {branch}? The branch is kept."),
            then: Confirmed::RemoveWorktree {
                workspace,
                force: false,
            },
        });
    }

    fn dialog_key(&mut self, dialog: Dialog, key: &KeyEvent) -> Vec<Action> {
        match dialog {
            Dialog::Confirm { then, .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    self.mode = Mode::Browse;
                    vec![self.confirmed(then)]
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.mode = Mode::Browse;
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Dialog::AddProject { mut path } => match path.handle(key) {
                Edit::Submit => {
                    self.mode = Mode::Browse;
                    let root = resolve_path(path.text(), &self.cwd);
                    let command = Command::AddProject { root, name: None };
                    vec![self.command(command, Then::SelectProject)]
                }
                edit => self.keep_editing(edit, Dialog::AddProject { path }),
            },
            Dialog::NewWorktree {
                project,
                mut branch,
            } => match branch.handle(key) {
                Edit::Submit => {
                    self.mode = Mode::Browse;
                    let branch = Some(branch.text().trim().to_owned()).filter(|b| !b.is_empty());
                    let command = Command::CreateWorktree { project, branch };
                    vec![self.command(command, Then::SelectWorktree)]
                }
                edit => self.keep_editing(edit, Dialog::NewWorktree { project, branch }),
            },
            Dialog::Issues {
                project,
                issues,
                selected,
                work,
                error,
            } => {
                let count = issues.as_ref().map_or(0, Vec::len);
                let mut selected = selected;
                let mut work = work;
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.mode = Mode::Browse;
                        return Vec::new();
                    }
                    KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + 1).min(count.saturating_sub(1));
                    }
                    KeyCode::Tab | KeyCode::BackTab => work = work.next(),
                    KeyCode::Enter => {
                        if let Some(issue) = issues.as_ref().and_then(|list| list.get(selected)) {
                            let number = issue.number;
                            self.mode = Mode::Browse;
                            return match work {
                                IssueWork::Session(harness) => {
                                    let command = Command::StartFromIssue {
                                        project,
                                        number,
                                        harness: harness.id().to_owned(),
                                        size: self.terminal_size,
                                    };
                                    self.say(format!("Starting work on #{number}…"));
                                    vec![self.command(command, Then::OpenIssueSession)]
                                }
                                IssueWork::Autopilot => {
                                    let command = Command::StartRun { project, number };
                                    self.say(format!("Handing #{number} to the autopilot…"));
                                    vec![self.command(command, Then::ShowRun)]
                                }
                            };
                        }
                    }
                    _ => {}
                }
                self.mode = Mode::Dialog(Dialog::Issues {
                    project,
                    issues,
                    selected,
                    work,
                    error,
                });
                Vec::new()
            }
            dialog @ (Dialog::RunNotes { .. } | Dialog::RunChanges { .. }) => {
                self.run_dialog_key(dialog, key)
            }
            Dialog::NewSession {
                workspace,
                harness,
                mut prompt,
            } => {
                if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                    let harness = match harness {
                        HarnessChoice::Claude => HarnessChoice::Shell,
                        HarnessChoice::Shell => HarnessChoice::Claude,
                    };
                    self.mode = Mode::Dialog(Dialog::NewSession {
                        workspace,
                        harness,
                        prompt,
                    });
                    return Vec::new();
                }
                let edit = match harness {
                    HarnessChoice::Claude => prompt.handle(key),
                    // A shell takes no prompt: only Enter and Esc matter.
                    HarnessChoice::Shell => match key.code {
                        KeyCode::Enter => Edit::Submit,
                        KeyCode::Esc => Edit::Cancel,
                        _ => Edit::Ignored,
                    },
                };
                match edit {
                    Edit::Submit => {
                        self.mode = Mode::Browse;
                        let prompt = Some(prompt.text().trim().to_owned())
                            .filter(|p| !p.is_empty() && harness == HarnessChoice::Claude);
                        let command = Command::StartSession {
                            workspace,
                            harness: harness.id().to_owned(),
                            title: None,
                            size: self.terminal_size,
                            prompt,
                        };
                        vec![self.command(command, Then::OpenSession)]
                    }
                    edit => self.keep_editing(
                        edit,
                        Dialog::NewSession {
                            workspace,
                            harness,
                            prompt,
                        },
                    ),
                }
            }
        }
    }

    pub(super) fn keep_editing(&mut self, edit: Edit, dialog: Dialog) -> Vec<Action> {
        self.mode = match edit {
            Edit::Cancel => Mode::Browse,
            _ => Mode::Dialog(dialog),
        };
        Vec::new()
    }

    fn confirmed(&mut self, then: Confirmed) -> Action {
        match then {
            Confirmed::StopSession(session) => self.command(
                Command::StopSession { session },
                Then::Say("Stopping the session."),
            ),
            Confirmed::RemoveWorktree { workspace, force } => self.command(
                Command::RemoveWorktree { workspace, force },
                Then::RemovedWorktree(workspace),
            ),
            Confirmed::RemoveProject(project) => self.command(
                Command::RemoveProject { project },
                Then::Say("Muster no longer tracks the project; its repository is untouched."),
            ),
            Confirmed::CancelRun(run) => self.command(
                Command::CancelRun { run },
                Then::Say("Cancelled the run; its container goes."),
            ),
            Confirmed::HandBackRun(run) => self.command(
                Command::HandBackRun { run },
                Then::Say("Handed the run back; the checks run on your work."),
            ),
        }
    }
}

/// A typed path: `~` is the home directory, and a relative path is taken
/// from where `muster` was started.
fn resolve_path(typed: &str, cwd: &Path) -> PathBuf {
    let typed = typed.trim();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = match (typed.strip_prefix("~"), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            home.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(typed),
    };
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}
