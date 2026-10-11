//! What keys and pastes do in each mode.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use muster_core::protocol::{ClientFrame, Command};

use super::{Action, App, Confirmed, Dialog, HarnessChoice, Mode, Then, View};
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
                let modes = self.screen(session).modes;
                vec![Action::Send(ClientFrame::Input {
                    session,
                    data: encode_paste(text, modes),
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
        if leaves_terminal(key) {
            self.mode = Mode::Browse;
            return self.settle();
        }
        let modes = self.screen(session).modes;
        encode_key(key, modes)
            .map(|data| Action::Send(ClientFrame::Input { session, data }))
            .into_iter()
            .collect()
    }

    fn browse_key(&mut self, key: &KeyEvent) -> Vec<Action> {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if control => return vec![Action::Quit],
            KeyCode::Char('q') => return vec![Action::Quit],
            KeyCode::Tab | KeyCode::BackTab => match self.view {
                View::Sessions => {
                    // The queue opens on its most urgent session.
                    self.view = View::NeedsYou;
                    self.needs_selected = self.needs_list.first().copied();
                }
                View::NeedsYou => self.view = View::Sessions,
            },
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
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
            KeyCode::Char('x') => self.confirm_remove(),
            _ => {}
        }
        Vec::new()
    }

    fn move_selection(&mut self, by: isize) {
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
                    .and_then(|session| list.iter().position(|s| *s == session));
                self.needs_selected = step(list.len(), at).map(|index| list[index]);
            }
        }
    }

    fn open_selected(&mut self) {
        if let Some(session) = self.focused_session() {
            self.mode = Mode::Terminal(session);
            self.note = None;
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
        let Some(project) = self.selected.and_then(|row| self.model.project_of(row)) else {
            self.complain("Add a project first: press a.");
            return Vec::new();
        };
        self.mode = Mode::Dialog(Dialog::Issues {
            project,
            issues: None,
            selected: 0,
            harness: HarnessChoice::Claude,
            error: None,
        });
        vec![self.command(Command::ListIssues { project }, Then::ShowIssues(project))]
    }

    fn new_session(&mut self) {
        let workspace = match self.view {
            View::Sessions => self.selected.and_then(|row| self.model.worktree_of(row)),
            View::NeedsYou => self
                .needs_selected
                .and_then(|session| self.model.worktree_of(Row::Session(session))),
        };
        match workspace {
            Some(workspace) => {
                self.mode = Mode::Dialog(Dialog::NewSession {
                    workspace,
                    harness: HarnessChoice::Claude,
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

    fn confirm_remove(&mut self) {
        let Some(workspace) = self.selected.and_then(|row| self.model.worktree_of(row)) else {
            return self.complain("Select a worktree to remove.");
        };
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
                harness,
                error,
            } => {
                let count = issues.as_ref().map_or(0, Vec::len);
                let mut selected = selected;
                let mut harness = harness;
                match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        self.mode = Mode::Browse;
                        return Vec::new();
                    }
                    KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                    KeyCode::Down | KeyCode::Char('j') => {
                        selected = (selected + 1).min(count.saturating_sub(1));
                    }
                    KeyCode::Tab | KeyCode::BackTab => {
                        harness = match harness {
                            HarnessChoice::Claude => HarnessChoice::Shell,
                            HarnessChoice::Shell => HarnessChoice::Claude,
                        };
                    }
                    KeyCode::Enter => {
                        if let Some(issue) = issues.as_ref().and_then(|list| list.get(selected)) {
                            let command = Command::StartFromIssue {
                                project,
                                number: issue.number,
                                harness: harness.id().to_owned(),
                                size: self.terminal_size,
                            };
                            self.mode = Mode::Browse;
                            self.say(format!("Starting work on #{}…", issue.number));
                            return vec![self.command(command, Then::OpenIssueSession)];
                        }
                    }
                    _ => {}
                }
                self.mode = Mode::Dialog(Dialog::Issues {
                    project,
                    issues,
                    selected,
                    harness,
                    error,
                });
                Vec::new()
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

    fn keep_editing(&mut self, edit: Edit, dialog: Dialog) -> Vec<Action> {
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
