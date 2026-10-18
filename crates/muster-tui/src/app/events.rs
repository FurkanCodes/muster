//! What keys and pastes do in each mode.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use muster_core::WorkspaceId;
use muster_core::protocol::{ClientFrame, Command};

use super::{Action, App, Confirmed, Dialog, HarnessChoice, Mode, Then, View, harness_choice};
use crate::input::{Edit, LineInput};
use crate::keys::{encode_key, encode_paste};
use crate::model::{Row, needs_attention};

/// Whether `key` gives the keys back from a session's terminal to the
/// list: Shift+Q, the person's choice, so a capital Q never reaches a
/// session's program. A terminal sends it as `Q`, or, with the kitty
/// keyboard protocol, as `q` with Shift. Ctrl+], the key before it, still
/// works; a terminal without the kitty protocol sends it as byte 0x1d,
/// which crossterm reports as Ctrl+5, so both are accepted.
pub(crate) fn leaves_terminal(key: &KeyEvent) -> bool {
    let (control, alt) = (
        key.modifiers.contains(KeyModifiers::CONTROL),
        key.modifiers.contains(KeyModifiers::ALT),
    );
    let shift_q = key.code == KeyCode::Char('Q')
        || (key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::SHIFT));
    (shift_q && !control && !alt) || (control && matches!(key.code, KeyCode::Char(']' | '5')))
}

/// The screen Ctrl+1, Ctrl+2 or Ctrl+3 goes to, from anywhere, a session's
/// terminal included: no program there uses them. Terminals tell them from
/// plain digits only with the kitty keyboard protocol (without it, Ctrl+3
/// is Esc), so on the list the plain digits work too. Settings, the fourth,
/// is `4` on the list only: without the kitty protocol Ctrl+4 and Ctrl+\
/// are one byte, which a session's program may want (a shell's quit key).
pub(crate) fn screen_key(key: &KeyEvent) -> Option<View> {
    if !key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match key.code {
        KeyCode::Char('1') => Some(View::Sessions),
        KeyCode::Char('2') => Some(View::NeedsYou),
        KeyCode::Char('3') => Some(View::Tickets),
        _ => None,
    }
}

impl App {
    pub(crate) fn on_key(&mut self, key: KeyEvent) -> Vec<Action> {
        // Repeats count as presses; releases (sent by some terminals) do not.
        if key.kind == KeyEventKind::Release {
            return Vec::new();
        }
        // Typed ahead, as the person knows the screens behind.
        self.skip_welcome();
        // An error stays until the person has seen it and pressed on.
        if self.note.as_ref().is_some_and(|note| note.error) {
            self.note = None;
        }
        let mut actions = match self.mode.clone() {
            Mode::Terminal(session) => return self.terminal_key(session, &key),
            Mode::Dialog(dialog) => self.dialog_key(dialog, &key),
            Mode::Changes => self.changes_key(&key),
            Mode::Browse => self.browse_key(&key),
        };
        actions.extend(self.settle());
        actions
    }

    pub(crate) fn on_paste(&mut self, text: &str) -> Vec<Action> {
        self.skip_welcome();
        match &mut self.mode {
            Mode::Terminal(session) => {
                let session = *session;
                self.selection = None;
                let screen = self.screen(session);
                screen.scroll_to_bottom();
                vec![Action::Send(ClientFrame::Input {
                    session,
                    data: encode_paste(text, screen.modes),
                })]
            }
            Mode::Dialog(Dialog::AddProject { path, picker }) => {
                let (path, picker) = (path.clone(), picker.clone());
                self.picker_paste(path, picker, text)
            }
            Mode::Dialog(
                Dialog::NewWorktree { branch: input, .. }
                | Dialog::NewSession { prompt: input, .. },
            ) => {
                input.paste(text);
                Vec::new()
            }
            Mode::Dialog(Dialog::TicketsFrom {
                tracker,
                site,
                key,
                on_key,
                ..
            }) => {
                match super::tickets_from::focused(*tracker, *on_key) {
                    super::tickets_from::Field::Site => site.paste(text),
                    super::tickets_from::Field::Key => key.paste(text),
                    super::tickets_from::Field::None => {}
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn terminal_key(&mut self, session: muster_core::SessionId, key: &KeyEvent) -> Vec<Action> {
        self.note = None;
        self.selection = None;
        if leaves_terminal(key) {
            return self.leave_terminal(session);
        }
        if let Some(view) = screen_key(key) {
            let mut actions = self.leave_terminal(session);
            actions.extend(self.show(view));
            return actions;
        }
        // A coding agent suspends itself on Ctrl+Z and waits for a shell's
        // `fg`; a session has no shell above it, so the key would only
        // freeze it.
        let suspends = self
            .model
            .session(session)
            .and_then(|stored| harness_choice::suspends_itself(&stored.harness));
        if let Some(agent) = suspends
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('z')
        {
            self.complain(format!(
                "Ctrl+Z would freeze {agent} here, so it was not sent. Shift-Q goes back to the list."
            ));
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
        if let Some(view) = screen_key(key) {
            return self.show(view);
        }
        match key.code {
            KeyCode::Char('c') if control => return vec![Action::Quit],
            // Shift+Q leaves a session's terminal; pressed once too often
            // on the list, it must not quit.
            _ if leaves_terminal(key) => return Vec::new(),
            KeyCode::Char('q') if control => return Vec::new(),
            KeyCode::Char('q') => return vec![Action::Quit],
            // The tabs, round and back.
            KeyCode::Tab | KeyCode::BackTab => {
                return self.show(self.view.next(key.code == KeyCode::BackTab));
            }
            KeyCode::Char('1') => return self.show(View::Sessions),
            KeyCode::Char('2') => return self.show(View::NeedsYou),
            KeyCode::Char('3') => return self.show(View::Tickets),
            KeyCode::Char('4') => return self.show(View::Settings),
            // Back to the main screen; there, nothing to go back to.
            KeyCode::Esc | KeyCode::Backspace if self.view != View::Sessions => {
                return self.show(View::Sessions);
            }
            _ if self.view == View::Tickets => return self.tickets_key(key),
            _ if self.view == View::Settings => return self.settings_key(key),
            _ => {}
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.page_preview(true),
            KeyCode::PageDown => self.page_preview(false),
            KeyCode::End => self.preview_to_bottom(),
            KeyCode::Enter => self.open_selected(),
            KeyCode::Char('a') => return self.open_add_project(),
            KeyCode::Char('w') => self.new_worktree(),
            KeyCode::Char('i') => {
                let project = self.selected.and_then(|row| self.model.project_of(row));
                self.show_tickets(project);
                return self.settle();
            }
            KeyCode::Char('n') => self.new_session(),
            KeyCode::Char('f') => return self.open_full_screen(),
            KeyCode::Char('s') => self.confirm_stop(),
            KeyCode::Char('r') => return self.restart(),
            KeyCode::Char('x') => return self.remove(),
            KeyCode::Char('c') if self.view == View::Sessions => return self.check(),
            KeyCode::Char('v') if self.view == View::Sessions => return self.review(),
            KeyCode::Char('t') if self.view == View::Sessions => return self.ask_tidy(),
            KeyCode::Char('u') if self.view == View::Sessions => return self.ask_move(),
            KeyCode::Char('o') if self.view == View::Sessions => return self.open_pull_request(),
            KeyCode::Char('b') if self.view == View::Sessions => return self.browse_pull_request(),
            KeyCode::Char('d') => return self.open_changes(),
            KeyCode::Char('e') if self.view == View::Sessions => self.toggle_fold(),
            _ => {}
        }
        Vec::new()
    }

    /// Shows `view`. The queue opens on its most urgent session, without
    /// one kept from the last visit; tickets that could not be fetched are
    /// tried again; the settings are read afresh, as they may have been
    /// edited by hand.
    pub(super) fn show(&mut self, view: View) -> Vec<Action> {
        if view == View::NeedsYou && self.view != View::NeedsYou {
            self.needs_selected = self.model.needs_you().first().copied();
        }
        if view == View::Tickets && self.view != View::Tickets {
            self.forget_failed_tickets();
        }
        if view == View::Settings && self.view != View::Settings {
            self.settings
                .retain(|_, settings| matches!(settings, super::Settings::Loading));
        }
        self.view = view;
        self.settle()
    }

    pub(super) fn move_selection(&mut self, by: isize) {
        let step = |list_len: usize, at: Option<usize>| -> Option<usize> {
            if list_len == 0 {
                return None;
            }
            Some(at.unwrap_or(0).saturating_add_signed(by).min(list_len - 1))
        };
        match self.view {
            View::Sessions => {
                let rows = self.rows();
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
            View::Tickets => {
                let rows = self.ticket_rows();
                let at = self
                    .tickets_selected
                    .as_ref()
                    .and_then(|row| rows.iter().position(|r| r == row));
                self.tickets_selected = step(rows.len(), at).map(|index| rows[index].clone());
            }
            View::Settings => {
                let rows = self.setting_rows();
                let at = self
                    .settings_selected
                    .and_then(|row| rows.iter().position(|r| *r == row));
                self.settings_selected = step(rows.len(), at).map(|index| rows[index]);
            }
        }
    }

    pub(super) fn open_selected(&mut self) {
        if let Some(session) = self.focused_session() {
            // Beside the list, at the size it is already shown at.
            self.mode = Mode::Terminal(session);
            self.full_screen = false;
            self.note = None;
            return;
        }
        match self.selected {
            Some(Row::Worktree(_)) => self.new_session(),
            // A plain folder has no worktrees: a session in the folder.
            Some(Row::Project(project)) if self.model.is_plain_folder(project) => {
                self.new_session();
            }
            Some(Row::Project(_)) => self.new_worktree(),
            _ => {}
        }
    }

    /// `f`: the selected session's terminal, with the keys, over the whole
    /// window.
    fn open_full_screen(&mut self) -> Vec<Action> {
        let Some(session) = self.focused_session() else {
            self.complain("Select a session to open it full screen.");
            return Vec::new();
        };
        self.mode = Mode::Terminal(session);
        self.full_screen = true;
        self.note = None;
        self.fit_attached()
    }

    /// Gives the keys back to the list. A session that filled the window
    /// goes back beside it, at that size.
    pub(super) fn leave_terminal(&mut self, session: muster_core::SessionId) -> Vec<Action> {
        self.screen(session).scroll_to_bottom();
        self.selection = None;
        self.mode = Mode::Browse;
        // Back from a session of the queue that waits no more: the person
        // dealt with it, so the queue moves on rather than keep it.
        if self.view == View::NeedsYou
            && self.needs_selected == Some(session)
            && !self.model.session(session).is_some_and(needs_attention)
        {
            self.needs_selected = None;
        }
        let was_full = std::mem::take(&mut self.full_screen);
        let mut actions = self.settle();
        if was_full {
            actions.extend(self.fit_attached());
        }
        actions
    }

    /// `c`: the project's checks in the selected workspace, opened as they
    /// run.
    fn check(&mut self) -> Vec<Action> {
        let Some(workspace) = self.hands_on_workspace("run the checks there") else {
            return Vec::new();
        };
        let size = self.session_size();
        vec![self.command(
            Command::CheckWorkspace { workspace, size },
            Then::OpenSession,
        )]
    }

    /// The workspace selected for a stage by hand: a worktree or checkout,
    /// or one a session runs in; not a run's container, whose run does its
    /// own.
    pub(super) fn hands_on_workspace(&mut self, what: &str) -> Option<WorkspaceId> {
        let Some(workspace) = self.selected.and_then(|row| self.model.worktree_of(row)) else {
            self.complain(format!(
                "Select a worktree, or a session in one, to {what}."
            ));
            return None;
        };
        if self.model.run_of(workspace).is_some() {
            self.complain("A run does that itself; hand it back to have it done.");
            return None;
        }
        Some(workspace)
    }

    /// `v`: Claude Code reviews the selected workspace's change, opened as
    /// it starts.
    fn review(&mut self) -> Vec<Action> {
        let Some(workspace) = self.hands_on_workspace("review its change") else {
            return Vec::new();
        };
        let size = self.session_size();
        vec![self.command(
            Command::ReviewWorkspace { workspace, size },
            Then::OpenSession,
        )]
    }

    /// `o`: pushes the selected workspace's branch and opens a draft pull
    /// request from it.
    fn open_pull_request(&mut self) -> Vec<Action> {
        let Some(workspace) = self.hands_on_workspace("open a pull request from it") else {
            return Vec::new();
        };
        self.say("Pushing the branch and opening a draft pull request…");
        vec![self.command(
            Command::OpenPullRequest { workspace },
            Then::ShowPullRequest,
        )]
    }

    /// `b`: the pull request from the selected worktree's branch, in the
    /// browser.
    fn browse_pull_request(&mut self) -> Vec<Action> {
        let found = self
            .selected
            .and_then(|row| self.model.worktree_of(row))
            .and_then(|workspace| self.model.workspace(workspace))
            .and_then(|workspace| workspace.pull_request.as_ref());
        if let Some(found) = found {
            vec![Action::Open(found.url.clone())]
        } else {
            self.complain("No pull request from this branch is known; o opens one.");
            Vec::new()
        }
    }

    fn new_worktree(&mut self) {
        match self.selected.and_then(|row| self.model.project_of(row)) {
            Some(project) if self.model.is_plain_folder(project) => {
                let name = self
                    .model
                    .project(project)
                    .map_or("That project", |found| found.name.as_str());
                self.complain(format!(
                    "{name} is a plain folder, not a git repository, so it has no worktrees: press n for a session in it."
                ));
            }
            Some(project) => {
                self.mode = Mode::Dialog(Dialog::NewWorktree {
                    project,
                    branch: LineInput::default(),
                });
            }
            None => self.complain("Add a project first: press a."),
        }
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
            View::Tickets | View::Settings => None,
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
                        self.last_harness
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
            size: self.session_size(),
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
                    "Stop tracking {name}? Muster forgets it; nothing in its folder is touched."
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
                self.mode = Mode::Dialog(Dialog::Confirm {
                    question: format!("Close {}? It stops and leaves the list.", stored.title),
                    then: Confirmed::CloseSession(session),
                });
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
                    self.confirmed(then)
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    self.mode = Mode::Browse;
                    Vec::new()
                }
                _ => Vec::new(),
            },
            Dialog::Undo {
                workspace, plan, ..
            } => self.undo_key(workspace, &plan, key),
            Dialog::Tidy { workspace, harness } => self.tidy_key(workspace, harness, key),
            Dialog::AddProject { path, picker } => self.picker_key(path, picker, key),
            dialog @ Dialog::TicketsFrom { .. } => self.tickets_from_key(dialog, key),
            Dialog::AssignAgent {
                project,
                key: ticket,
                harness,
                another,
                into,
                on,
            } => self.assign_key(project, ticket, harness, (another, into, on), key),
            Dialog::NewWorktree {
                project,
                mut branch,
            } => match branch.handle(key) {
                Edit::Submit => {
                    self.mode = Mode::Browse;
                    let branch = Some(branch.text().trim().to_owned()).filter(|b| !b.is_empty());
                    let command = Command::CreateWorktree {
                        project,
                        branch,
                        on: None,
                    };
                    vec![self.command(command, Then::SelectWorktree)]
                }
                edit => self.keep_editing(edit, Dialog::NewWorktree { project, branch }),
            },
            Dialog::NewSession {
                workspace,
                harness,
                mut prompt,
            } => {
                if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
                    let harness = if key.code == KeyCode::Tab {
                        harness.next()
                    } else {
                        harness.previous()
                    };
                    self.mode = Mode::Dialog(Dialog::NewSession {
                        workspace,
                        harness,
                        prompt,
                    });
                    return Vec::new();
                }
                let edit = if harness.is_agent() {
                    prompt.handle(key)
                } else {
                    // A shell takes no prompt: only Enter and Esc matter.
                    match key.code {
                        KeyCode::Enter => Edit::Submit,
                        KeyCode::Esc => Edit::Cancel,
                        _ => Edit::Ignored,
                    }
                };
                match edit {
                    Edit::Submit => {
                        self.mode = Mode::Browse;
                        self.last_harness = harness;
                        let prompt = Some(prompt.text().trim().to_owned())
                            .filter(|p| !p.is_empty() && harness.is_agent());
                        let command = Command::StartSession {
                            workspace,
                            harness: harness.id().to_owned(),
                            title: None,
                            size: self.session_size(),
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

    fn confirmed(&mut self, then: Confirmed) -> Vec<Action> {
        let action = match then {
            Confirmed::StopSession(session) => self.command(
                Command::StopSession { session },
                Then::Say("Stopping the session."),
            ),
            Confirmed::CloseSession(session) => {
                self.closing.push(session);
                self.say("Closing the session…");
                self.command(Command::StopSession { session }, Then::Closing(session))
            }
            Confirmed::RemoveWorktree { workspace, force } => self.command(
                Command::RemoveWorktree { workspace, force },
                Then::RemovedWorktree(workspace),
            ),
            Confirmed::MoveOnto(workspace) => self.confirmed_move(workspace),
            Confirmed::RemoveProject(project) => self.command(
                Command::RemoveProject { project },
                Then::Say("Muster no longer tracks the project; its folder is untouched."),
            ),
            Confirmed::CloseTicket { project, key } => {
                self.say(format!(
                    "Closing {} in {}…",
                    key.shown(),
                    self.tracker_of(project)
                ));
                self.command(
                    Command::CloseIssue {
                        project,
                        key: key.clone(),
                    },
                    Then::TicketClosed(project, key),
                )
            }
            Confirmed::CleanUpTicket { project, key } => {
                return self.clean_up_ticket(project, &key);
            }
        };
        vec![action]
    }
}
