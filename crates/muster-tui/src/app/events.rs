//! What keys and pastes do in each mode.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use muster_core::WorkspaceId;
use muster_core::keys::{Action as Does, Screen};
use muster_core::protocol::{ClientFrame, Command};

use super::{Action, App, Confirmed, Dialog, HarnessChoice, Mode, Then, View, harness_choice};
use crate::input::{Edit, LineInput};
use crate::keys::{encode_key, encode_paste};
use crate::model::{Row, needs_attention};

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
    /// The screen of the list shown, whose keys a key press is.
    pub(super) fn key_screen(&self) -> Screen {
        match self.view {
            View::Sessions => Screen::Sessions,
            View::NeedsYou => Screen::NeedsYou,
            View::Tickets => Screen::Tickets,
            View::Settings => Screen::Settings,
        }
    }

    /// What `key` does on `screen`, as the person's keys say
    /// (`muster_core::keys`).
    pub(super) fn does(&self, screen: Screen, key: &KeyEvent) -> Option<Does> {
        crate::keys::chord(key).and_then(|chord| self.keymap.action(screen, chord))
    }

    pub(crate) fn on_key(&mut self, key: KeyEvent) -> Vec<Action> {
        // Repeats count as presses; releases (sent by some terminals) do not.
        if key.kind == KeyEventKind::Release {
            return Vec::new();
        }
        // Typed ahead, as the person knows the screens behind.
        self.skip_welcome();
        // What a key changes may move what the pointer is over.
        self.panels.hover = None;
        // An error stays until the person has seen it and pressed on.
        if self.note.as_ref().is_some_and(|note| note.error) {
            self.note = None;
        }
        if let Some(drag) = &self.panels.drag {
            // A key lets go of a drag, the layout as it was; a line pressed
            // in the list and gone nowhere yet was only a click.
            let only_pressed = matches!(drag, super::drag::Drag::Line { to: None, .. });
            self.cancel_drag();
            if !only_pressed {
                return self.settle();
            }
        }
        if self.panels.arranging.is_some() {
            let mut actions = self.arrange_key(&key);
            actions.extend(self.settle());
            return actions;
        }
        let mut actions = match self.mode.clone() {
            Mode::Terminal(session) => return self.terminal_key(session, &key),
            Mode::Dialog(dialog) => self.dialog_key(dialog, &key),
            Mode::Menu(menu) => self.menu_key(menu, &key),
            Mode::Help(help) => self.help_key(help, &key),
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
            Mode::Dialog(Dialog::AddProject {
                path,
                picker,
                clone,
            }) => {
                let (path, picker, clone) = (path.clone(), picker.clone(), clone.clone());
                self.picker_paste(path, picker, clone, text)
            }
            Mode::Dialog(Dialog::CloneProject(choice)) => {
                let choice = choice.clone();
                self.clone_paste(choice, text)
            }
            Mode::Help(help) => {
                let help = help.clone();
                self.help_paste(help, text);
                Vec::new()
            }
            Mode::Dialog(
                Dialog::NewWorktree { branch: input, .. }
                | Dialog::NewSession { prompt: input, .. }
                | Dialog::Rename { name: input, .. }
                | Dialog::NamingModel { model: input, .. },
            ) => {
                input.paste(text);
                Vec::new()
            }
            Mode::Dialog(Dialog::Plugin(_)) => {
                self.plugin_paste(text);
                Vec::new()
            }
            Mode::Dialog(Dialog::TicketsFrom { .. }) => {
                self.tickets_from_paste(text);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn terminal_key(&mut self, session: muster_core::SessionId, key: &KeyEvent) -> Vec<Action> {
        self.note = None;
        self.selection = None;
        // Every key is the program's but the few the person's keys keep for
        // Muster here.
        match self.does(Screen::InSession, key) {
            Some(Does::LeaveSession) => return self.leave_terminal(session),
            Some(Does::FoldList) => return self.toggle_list(),
            _ => {}
        }
        if let Some(view) = screen_key(key) {
            let mut actions = self.leave_terminal(session);
            actions.extend(self.show(view));
            return actions;
        }
        // A coding agent suspends itself on Ctrl+Z and waits for a shell's
        // `fg`; a session has no shell above it, so the key would only
        // freeze it.
        let suspends = self.model.session(session).and_then(|stored| {
            harness_choice::suspends_itself(&stored.harness, self.added_agents())
        });
        if let Some(agent) = suspends
            && key.modifiers.contains(KeyModifiers::CONTROL)
            && key.code == KeyCode::Char('z')
        {
            self.complain(format!(
                "Ctrl+Z would freeze {agent} here, so it was not sent. {} goes back to the list.",
                self.key(Does::LeaveSession)
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
        if control && key.code == KeyCode::Char('c') {
            return vec![Action::Quit];
        }
        // The key that leaves a session's terminal, pressed once too often
        // on the list, must not quit; nor Ctrl+Q, which reaches programs.
        let leaves = crate::keys::chord(key)
            .is_some_and(|chord| self.keymap.chords(Does::LeaveSession).contains(&chord));
        if leaves || (control && key.code == KeyCode::Char('q')) {
            return Vec::new();
        }
        let does = self.does(self.key_screen(), key);
        match does {
            Some(Does::Quit) => return vec![Action::Quit],
            Some(Does::NextTab) => return self.show(self.view.next(false)),
            Some(Does::PreviousTab) => return self.show(self.view.next(true)),
            Some(Does::SessionsTab) => return self.show(View::Sessions),
            Some(Does::NeedsYouTab) => return self.show(View::NeedsYou),
            Some(Does::TicketsTab) => return self.show(View::Tickets),
            Some(Does::SettingsTab) => return self.show(View::Settings),
            Some(Does::LastAlarm) => return self.go_to_last_alarm(),
            Some(Does::MovePanels) => return self.start_arranging(),
            Some(Does::FoldList) => return self.toggle_list(),
            Some(Does::Help) => {
                self.open_help();
                return Vec::new();
            }
            // Back to the main screen; there, nothing to go back to.
            Some(Does::Back) => return self.show(View::Sessions),
            _ => {}
        }
        match self.view {
            View::Tickets => return self.tickets_key(key, does),
            View::Settings => return self.settings_key(key, does),
            View::Sessions | View::NeedsYou => {}
        }
        let panel = crate::keys::chord(key)
            .and_then(|chord| self.keymap.panel(self.key_screen(), chord))
            .map(muster_core::keys::PanelAction::name);
        if let (None, Some(panel)) = (does, panel) {
            return self.toggle_panel(&panel);
        }
        match (key.code, does) {
            (KeyCode::Up, _) | (_, Some(Does::MoveUp)) => self.move_selection(-1),
            (KeyCode::Down, _) | (_, Some(Does::MoveDown)) => self.move_selection(1),
            (KeyCode::PageUp, _) => self.page_preview(true),
            (KeyCode::PageDown, _) => self.page_preview(false),
            (KeyCode::End, _) => self.preview_to_bottom(),
            (KeyCode::Enter, _) => self.open_selected(),
            (_, Some(does)) => return self.list_action(does),
            _ => {}
        }
        Vec::new()
    }

    /// Does `does` on the selected line of *Sessions* or *Needs you*, as
    /// its key, or its item in the menu, says.
    pub(super) fn list_action(&mut self, does: Does) -> Vec<Action> {
        match does {
            Does::AddProject => return self.open_add_project(),
            Does::CloneProject => return self.open_clone_project(),
            Does::NewWorktree => self.new_worktree(),
            Does::ShowTickets => {
                let project = self.selected.and_then(|row| self.model.project_of(row));
                self.show_tickets(project);
                return self.settle();
            }
            Does::NewSession => self.new_session(),
            Does::FullScreen => return self.open_full_screen(),
            Does::StopSession => self.confirm_stop(),
            Does::RestartSession => return self.restart(),
            Does::Close => return self.remove(),
            Does::Checks => return self.check(),
            Does::Review => return self.review(),
            Does::Tidy => return self.ask_tidy(),
            Does::MoveBranch => return self.ask_move(),
            Does::Rename => return self.ask_rename(),
            Does::OpenPullRequest => return self.open_pull_request(),
            Does::PullRequestInBrowser => return self.browse_pull_request(),
            Does::ShowChanges => return self.open_changes(),
            Does::Fold => self.toggle_fold(),
            Does::Shell => return self.toggle_shell(),
            Does::PinSession => return self.toggle_pin(),
            _ => {}
        }
        Vec::new()
    }

    /// Shows `view`. The queue opens on its most urgent session, without
    /// one kept from the last visit; tickets that could not be fetched are
    /// tried again; the settings are read afresh, as they may have been
    /// edited by hand, and the plugins with them.
    pub(super) fn show(&mut self, view: View) -> Vec<Action> {
        let mut read = None;
        if view == View::NeedsYou && self.view != View::NeedsYou {
            self.needs.selected = self.model.needs_you().first().copied();
        }
        if view == View::Tickets && self.view != View::Tickets {
            self.forget_failed_tickets();
        }
        if view == View::Settings && self.view != View::Settings {
            self.settings
                .by_project
                .retain(|_, settings| matches!(settings, super::Settings::Loading));
            read = Some(self.read_plugins());
        }
        self.view = view;
        let mut actions = self.settle();
        actions.extend(read);
        actions
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
                let list = &self.needs.list;
                let at = self
                    .needs
                    .selected
                    .and_then(|session| list.iter().position(|s| *s == session));
                self.needs.selected = step(list.len(), at).map(|index| list[index]);
            }
            View::Tickets => {
                let rows = self.ticket_rows();
                let at = self
                    .tickets
                    .selected
                    .as_ref()
                    .and_then(|row| rows.iter().position(|r| r == row));
                self.tickets.selected = step(rows.len(), at).map(|index| rows[index].clone());
            }
            View::Settings => {
                let rows = self.setting_rows();
                let at = self
                    .settings
                    .selected
                    .and_then(|row| rows.iter().position(|r| *r == row));
                self.settings.selected = step(rows.len(), at).map(|index| rows[index]);
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
            && self.needs.selected == Some(session)
            && !self.model.session(session).is_some_and(needs_attention)
        {
            self.needs.selected = None;
        }
        // Settled, a session that filled the window is given its size
        // beside the list again.
        self.full_screen = false;
        self.settle()
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
            let open = self.key(Does::OpenPullRequest);
            self.complain(format!(
                "No pull request from this branch is known; {open} opens one."
            ));
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
                    "{name} is a plain folder, not a git repository, so it has no worktrees: press {} for a session in it.",
                    self.key(Does::NewSession)
                ));
            }
            Some(project) => {
                self.mode = Mode::Dialog(Dialog::NewWorktree {
                    project,
                    branch: LineInput::default(),
                });
            }
            None => self.complain(format!(
                "Add a project first: press {}.",
                self.key(Does::AddProject)
            )),
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
                        self.last_harness.clone()
                    },
                    prompt: LineInput::default(),
                });
            }
            None => self.complain(format!(
                "Select a worktree first, or create one: press {}.",
                self.key(Does::NewWorktree)
            )),
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
            Dialog::Rename { session, name } => self.rename_key(session, name, key),
            Dialog::Keys(rebinding) => self.rebind_key(rebinding, key),
            Dialog::Plugin(dialog) => self.plugin_dialog_key(dialog, key),
            Dialog::NamingModel {
                agent,
                model,
                listed,
            } => self.naming_model_key(agent, model, listed, key),
            Dialog::AddProject {
                path,
                picker,
                clone,
            } => self.picker_key(path, picker, clone, key),
            Dialog::CloneProject(choice) => self.clone_key(choice, key),
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
                        named_for: None,
                    };
                    vec![self.command(command, Then::SelectWorktree)]
                }
                edit => self.keep_editing(edit, Dialog::NewWorktree { project, branch }),
            },
            Dialog::NewSession {
                workspace,
                harness,
                prompt,
            } => self.new_session_key(workspace, harness, prompt, key),
        }
    }

    /// A key in the dialog starting a session: ⇥ and ⇧⇥ choose the
    /// harness, and its prompt is typed where it takes one.
    fn new_session_key(
        &mut self,
        workspace: WorkspaceId,
        harness: HarnessChoice,
        mut prompt: LineInput,
        key: &KeyEvent,
    ) -> Vec<Action> {
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            let harness = if key.code == KeyCode::Tab {
                harness.next(self.added_agents())
            } else {
                harness.previous(self.added_agents())
            };
            self.mode = Mode::Dialog(Dialog::NewSession {
                workspace,
                harness,
                prompt,
            });
            return Vec::new();
        }
        let edit = if harness.takes_prompt() {
            prompt.handle(key)
        } else {
            // A shell, or an agent that takes none, takes no
            // prompt: only Enter and Esc matter.
            match key.code {
                KeyCode::Enter => Edit::Submit,
                KeyCode::Esc => Edit::Cancel,
                _ => Edit::Ignored,
            }
        };
        match edit {
            Edit::Submit => {
                self.mode = Mode::Browse;
                let prompt = Some(prompt.text().trim().to_owned())
                    .filter(|p| !p.is_empty() && harness.takes_prompt());
                let command = Command::StartSession {
                    workspace,
                    harness: harness.id().to_owned(),
                    title: None,
                    size: self.session_size(),
                    prompt,
                    started_by: None,
                };
                self.last_harness = harness;
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
            Confirmed::RemovePlugin(id) => self.remove_plugin(id),
        };
        vec![action]
    }
}
