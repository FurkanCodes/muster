//! The link to the daemon: connecting to it and losing it, the frames it
//! sends (the state, terminals' output, replies to commands), and the news
//! told to a person who looks elsewhere.

use muster_core::SessionId;
use muster_core::protocol::{
    ClientFrame, Delta, DetachReason, ErrorBody, ErrorCode, Reply, ServerFrame,
};

use super::{
    Action, App, Confirmed, Dialog, Link, Mode, Settings, Then, TicketText, Tickets, View,
};
use crate::model::{Row, needs_attention};

impl App {
    pub(crate) fn on_connected(&mut self, daemon_version: String) -> Vec<Action> {
        self.link = Link::Connected { daemon_version };
        self.keep_in_view();
        self.attached = None;
        self.shell_stream.session = None;
        self.plugin_stream.session = None;
        // A new daemon knows nothing of where the keys are.
        self.focus_told = None;
        self.pending.clear();
        // Asked of the last daemon, never answered: asked again.
        self.forget_unanswered();
        // A new connection is a new daemon, which numbers its terminals'
        // output afresh: positions in the old screens mean nothing to it.
        self.screens.clear();
        // A new daemon may have outlived the notch, or come with none.
        self.notch_started = false;
        let mut actions = vec![Action::Send(ClientFrame::Subscribe)];
        actions.extend(self.looking());
        // Programs in sessions asking their terminal's colours are told the
        // person's, by a daemon that knows how (`ARC-09`).
        if self.daemon_protocol >= muster_core::colours::TERMINAL_PROTOCOL
            && !self.colours.is_empty()
        {
            actions.push(Action::Send(ClientFrame::Colours(self.colours.clone())));
        }
        actions
    }

    pub(crate) fn on_disconnected(&mut self, reason: String) {
        self.link = Link::Lost(reason);
        self.attached = None;
        self.shell_stream.session = None;
        self.plugin_stream.session = None;
        self.focus_told = None;
        self.pending.clear();
        self.forget_unanswered();
        self.forget_plugin_fetch();
        self.plugin_changing = None;
        // The reason takes lines from the list.
        self.keep_in_view();
    }

    // Frames from the daemon.

    pub(crate) fn on_frame(&mut self, frame: ServerFrame) -> Vec<Action> {
        let mut news = None;
        let mut copied = None;
        let settings = matches!(
            frame,
            ServerFrame::Snapshot(_) | ServerFrame::Delta(Delta::Preferences(_))
        );
        match frame {
            ServerFrame::Snapshot(snapshot) => self.model.replace(snapshot),
            ServerFrame::Delta(delta) => {
                let session = match &delta {
                    Delta::SessionStatus { session, .. } => Some(*session),
                    Delta::SessionUpserted(session) => Some(session.id),
                    _ => None,
                };
                let before = session.and_then(|id| self.model.session(id).map(needs_attention));
                let status_before =
                    session.and_then(|id| self.model.session(id).map(|stored| stored.status));
                let was_ended = session
                    .and_then(|id| self.model.session(id))
                    .is_some_and(|stored| stored.ended_at.is_some());
                self.model.apply(delta);
                if before == Some(false) {
                    news = session.and_then(|id| self.on_news(id));
                }
                if let Some(id) = session {
                    self.reattach_if_reopened(id, was_ended);
                    if let Some(status) = status_before {
                        self.flash_if_changed(id, status);
                    }
                }
            }
            ServerFrame::Reply { req_id, outcome } => return self.on_reply(req_id, outcome),
            ServerFrame::Replay {
                session,
                seq,
                data,
                reset,
                live_from,
            } => {
                let screen = self.screen(session);
                screen.replay(seq, &data, reset, live_from);
                // What the program copied as the person looked on, which
                // reached this client late.
                copied = screen
                    .take_copy()
                    .filter(|_| self.attached == Some(session));
            }
            ServerFrame::Output { session, seq, data } => {
                let screen = self.screen(session);
                screen.output(seq, &data);
                // What the program copied as the person looked on.
                copied = screen
                    .take_copy()
                    .filter(|_| self.attached == Some(session));
            }
            ServerFrame::Modes { session, modes } => self.screen(session).modes = modes,
            ServerFrame::Detached { session, reason } => {
                self.screen(session).ended = Some(reason);
            }
            ServerFrame::Reveal { session } => return self.reveal(session),
            ServerFrame::Welcome(_) | ServerFrame::Incompatible(_) | ServerFrame::Error(_) => {}
        }
        if settings {
            // The person's keys, as the daemon keeps them; ones that do not
            // read leave Muster's, the Settings screen saying why.
            self.keymap =
                muster_core::keys::Keymap::with(&self.model.preferences().keys).unwrap_or_default();
        }
        let mut actions: Vec<Action> = news.into_iter().collect();
        if let Some(text) = copied {
            match text.lines().count() {
                0 | 1 => self.say("The session copied a line."),
                lines => self.say(format!("The session copied {lines} lines.")),
            }
            actions.push(Action::Copy(text));
        }
        actions.extend(self.start_notch());
        actions.extend(self.settle());
        actions
    }

    /// A session that had ended reopened (restarted, maybe by another
    /// client). A stream the daemon ended because it held no terminal for
    /// it does not come back on its own, so attach again.
    fn reattach_if_reopened(&mut self, session: SessionId, was_ended: bool) {
        let reopened = was_ended
            && self
                .model
                .session(session)
                .is_some_and(|stored| stored.ended_at.is_none());
        let stream_gone = self
            .screens
            .get(&session)
            .is_some_and(|screen| screen.ended == Some(DetachReason::NotRunning));
        if reopened && stream_gone && self.attached == Some(session) {
            self.attached = None;
        }
    }

    /// A notification for a session that has come to need the person, if
    /// they are looking elsewhere and want to be told.
    pub(super) fn news(&self, session: SessionId) -> Option<Action> {
        // Muster's helper tells the person with the Mac's own notifications.
        if !self.notify || self.focused != Some(false) || self.model.notifier() {
            return None;
        }
        if !self.model.preferences().notifications {
            return None;
        }
        let stored = self.model.session(session)?;
        let kind = muster_core::telling::Kind::of(stored);
        if !needs_attention(stored) || !self.model.preferences().telling.of(kind).on {
            return None;
        }
        let project = self
            .model
            .project_of(Row::Session(session))
            .and_then(|id| self.model.project(id))
            .map(|project| project.name.as_str());
        let branch = self.model.branch(stored.workspace);
        Some(Action::Notify(muster_core::news::told(
            stored, project, branch,
        )))
    }

    /// The person's terminal window gained or lost focus. Looking away
    /// stops watching the attached session, so what happens meanwhile is
    /// news; looking back watches it again, and on the Tickets screen tries
    /// again what could not be fetched.
    pub(crate) fn on_focus(&mut self, focused: bool) -> Vec<Action> {
        self.focused = Some(focused);
        let mut actions = match self.attached {
            Some(session) if matches!(self.link, Link::Connected { .. }) => {
                vec![Action::Send(ClientFrame::Watch {
                    session,
                    watching: focused,
                })]
            }
            _ => Vec::new(),
        };
        actions.extend(self.looking());
        actions.extend(self.tell_focus());
        if focused && self.view == View::Tickets && matches!(self.link, Link::Connected { .. }) {
            self.forget_failed_tickets();
            actions.extend(self.fetch_tickets());
        }
        actions
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm for each reply the daemon can send"
    )]
    fn on_reply(&mut self, req_id: u64, outcome: Result<Reply, ErrorBody>) -> Vec<Action> {
        let Some(then) = self.pending.remove(&req_id) else {
            return Vec::new();
        };
        match (outcome, then) {
            (Ok(Reply::ProjectAdded { id }), Then::SelectProject) => {
                self.wanted = Some((Row::Project(id), false));
                self.say("Added the project.");
            }
            (Ok(Reply::WorktreeCreated { id }), Then::SelectWorktree) => {
                self.wanted = Some((Row::Worktree(id), false));
                self.say("Created the worktree.");
            }
            (Ok(Reply::SessionStarted { id }), Then::OpenSession)
            | (Ok(Reply::IssueStarted { session: id, .. }), Then::OpenIssueSession) => {
                self.wanted = Some((Row::Session(id), true));
                self.note = None;
            }
            (Ok(Reply::SessionStarted { id }), Then::OpenShell) => {
                self.wanted_shell = Some(id);
                self.note = None;
            }
            (Ok(Reply::SessionStarted { id }), Then::OpenRestarted) => {
                // The stream follows the session to its new program; one the
                // daemon had no terminal for is attached again once the
                // state says it reopened (see `on_frame`).
                self.wanted = Some((Row::Session(id), true));
                self.note = None;
            }
            (Ok(Reply::PullRequestOpened { pr }), Then::ShowPullRequest) => {
                self.say(format!(
                    "Draft pull request #{} is open: {}",
                    pr.number, pr.url
                ));
            }
            (
                Ok(Reply::Issues {
                    issues,
                    tracker,
                    mine,
                    source,
                }),
                Then::Tickets(project),
            ) => {
                self.tickets.insert(
                    project,
                    Tickets::Loaded {
                        tickets: issues,
                        tracker,
                        mine,
                        source,
                    },
                );
            }
            (Err(error), Then::Tickets(project)) => {
                self.tickets.insert(project, Tickets::Failed(error.message));
            }
            (Ok(Reply::IssueViewed { body, .. }), Then::TicketText(project, key)) => {
                self.ticket_texts
                    .insert((project, key), TicketText::Loaded(body));
            }
            (Err(error), Then::TicketText(project, key)) => {
                self.ticket_texts
                    .insert((project, key), TicketText::Failed(error.message));
            }
            (Ok(_), Then::RemovedWorktree(_)) => {
                self.say("Removed the worktree; its branch is kept.");
            }
            (Ok(_), Then::Say(text)) => self.say(text),
            (Ok(_), Then::TicketClosed(project, key)) => self.ticket_closed(project, key),
            (outcome, Then::NotchChanged(on)) => self.notch_changed(on, outcome.map(drop)),
            (outcome, Then::SideBySideChanged(columns)) => {
                self.side_by_side_changed(columns, outcome.map(drop));
            }
            (outcome, Then::TellChanged(kind, tell)) => {
                self.tell_changed(kind, &tell, outcome.map(drop));
            }
            (outcome, Then::LookChanged(change)) => self.look_changed(change, outcome.map(drop)),
            (outcome, Then::PluginPreview(asked)) => self.on_plugin_preview(asked, outcome),
            (outcome, Then::PluginToggled(plugin, on)) => {
                self.plugin_toggled(plugin, on, outcome.map(drop));
            }
            (outcome, Then::PluginInstalled(preview)) => {
                self.plugin_installed(&preview, outcome.map(drop));
            }
            (outcome, Then::PluginRemoved(id)) => self.plugin_removed(&id, outcome.map(drop)),
            (outcome, Then::ListFolded) => self.list_folded_kept(outcome.map(drop)),
            (outcome, Then::PanelsKept) => self.panels_kept(outcome.map(drop)),
            (Ok(Reply::Models(models)), Then::Models(agent)) => self.on_models(&agent, Ok(models)),
            (Err(error), Then::Models(agent)) => self.on_models(&agent, Err(error.message)),
            (Ok(Reply::Changes(changes)), Then::Changes(workspace)) => {
                self.on_changes(workspace, Ok(changes));
            }
            (Err(error), Then::Changes(workspace)) => {
                self.on_changes(workspace, Err(error.message));
            }
            (Ok(Reply::FileDiff(diff)), Then::Diff(workspace, pick)) => {
                self.on_diff(workspace, &pick, Ok(diff));
            }
            (Err(error), Then::Diff(workspace, pick)) => {
                self.on_diff(workspace, &pick, Err(error.message));
            }
            (Ok(Reply::Timeline(timeline)), Then::Timeline(workspace)) => {
                self.on_timeline(workspace, Ok(timeline));
            }
            (Err(error), Then::Timeline(workspace)) => {
                self.on_timeline(workspace, Err(error.message));
            }
            (Ok(Reply::UndoPlan(plan)), Then::UndoPlan(workspace)) => {
                self.on_undo_plan(workspace, Ok(plan));
            }
            (Err(error), Then::UndoPlan(workspace)) => {
                self.on_undo_plan(workspace, Err(error.message));
            }
            (
                Ok(Reply::Undone {
                    entry,
                    moved_branch,
                }),
                Then::Undone(workspace, number),
            ) => {
                return self.on_undone(workspace, number, Ok((entry, moved_branch)));
            }
            (Err(error), Then::Undone(workspace, number)) => {
                return self.on_undone(workspace, number, Err(error.message));
            }
            (
                Ok(Reply::Moved {
                    onto,
                    commits,
                    entry,
                }),
                Then::Moved(workspace),
            ) => self.on_moved(workspace, Ok((onto, commits, entry))),
            (Err(error), Then::Moved(workspace)) => self.on_moved(workspace, Err(error.message)),
            (Ok(Reply::ProjectSettings(settings)), Then::Settings(project)) => {
                self.settings.insert(project, Settings::Loaded(settings));
            }
            (Err(error), Then::Settings(project)) => {
                self.settings
                    .insert(project, Settings::Failed(error.message));
            }
            (Ok(Reply::ProjectSettings(settings)), Then::SettingChanged(project)) => {
                self.setting_changed(project, settings);
            }
            (Ok(Reply::ProjectSettings(settings)), Then::TicketsFromChanged(project)) => {
                self.tickets_from_changed(project, settings);
            }
            (Ok(Reply::ProjectSettings(settings)), Then::BranchesChanged(project)) => {
                self.branches_changed(project, settings);
            }
            (
                Err(error),
                Then::SettingChanged(project)
                | Then::TicketsFromChanged(project)
                | Then::BranchesChanged(project),
            ) => {
                self.setting_refused(project, error.message);
            }
            (Err(error), Then::Closing(session)) => {
                self.closing.retain(|closing| *closing != session);
                self.complain(error.message);
            }
            (Ok(_), _) => {}
            (Err(error), Then::RemovedWorktree(workspace))
                if error.code == ErrorCode::Conflict && error.message.contains("uncommitted") =>
            {
                self.mode = Mode::Dialog(Dialog::Confirm {
                    question: "It has uncommitted changes. Remove it anyway and discard them?"
                        .to_owned(),
                    then: Confirmed::RemoveWorktree {
                        workspace,
                        force: true,
                    },
                });
            }
            (Err(error), _) => self.complain(error.message),
        }
        self.settle()
    }

    /// Asked of a daemon and never answered (the connection went): asked
    /// again once the screen wants it.
    fn forget_unanswered(&mut self) {
        self.tickets
            .retain(|_, tickets| !matches!(tickets, Tickets::Loading));
        self.ticket_texts
            .retain(|_, text| !matches!(text, TicketText::Loading));
        self.settings
            .retain(|_, settings| !matches!(settings, Settings::Loading));
    }
}
