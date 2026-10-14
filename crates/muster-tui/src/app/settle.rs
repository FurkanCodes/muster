//! After anything changed: the selections kept on what exists and in
//! view, the Needs you queue in its order, closed sessions taken off, and
//! the terminal streamed to this client in line with what is shown.

use muster_core::SessionId;
use muster_core::protocol::{ClientFrame, Command};

use super::{Action, App, Link, Mode, Then, View};
use crate::model::Row;

impl App {
    /// Brings the selection and the streamed terminal in line with the
    /// state, after anything changed.
    pub(super) fn settle(&mut self) -> Vec<Action> {
        self.take_wanted();
        self.follow_answered();
        self.keep_selection();
        self.keep_ticket_selection();
        self.keep_setting_selection();
        self.keep_in_view();
        if let Mode::Terminal(session) = self.mode
            && self.model.session(session).is_none()
        {
            self.mode = Mode::Browse;
            self.full_screen = false;
        }
        if !matches!(self.link, Link::Connected { .. }) {
            return Vec::new();
        }
        let mut actions = self.fetch_tickets();
        actions.extend(self.fetch_settings());
        actions.extend(self.remove_closed());
        actions.extend(self.remove_idle_worktrees());
        let focus = self
            .focused_session()
            .filter(|s| self.model.session(*s).is_some());
        if focus == self.attached {
            return actions;
        }
        if let Some(old) = self.attached.take() {
            actions.push(Action::Send(ClientFrame::Detach { session: old }));
        }
        if let Some(session) = focus {
            let size = self.session_size();
            let screen = self.screen(session);
            screen.resize(size.cols, size.rows);
            let from_seq = screen.next_seq();
            let running = self
                .model
                .session(session)
                .is_some_and(|stored| stored.ended_at.is_none());
            if running {
                actions.push(Action::Send(ClientFrame::Resize { session, size }));
            }
            actions.push(Action::Send(ClientFrame::Attach {
                session,
                from_seq,
                // Attached while the window is out of focus: nobody looks.
                watching: self.focused != Some(false),
            }));
            self.attached = Some(session);
        }
        actions
    }

    /// Takes closed sessions off the list once their programs have ended.
    fn remove_closed(&mut self) -> Vec<Action> {
        let (ended, waiting): (Vec<SessionId>, Vec<SessionId>) = std::mem::take(&mut self.closing)
            .into_iter()
            .partition(|session| {
                self.model
                    .session(*session)
                    .is_none_or(|stored| stored.ended_at.is_some())
            });
        self.closing = waiting;
        // One already gone from the state needs nothing more.
        let ended: Vec<SessionId> = ended
            .into_iter()
            .filter(|session| self.model.session(*session).is_some())
            .collect();
        ended
            .into_iter()
            .map(|session| {
                self.command(
                    Command::RemoveSession { session },
                    Then::Say("Closed the session."),
                )
            })
            .collect()
    }

    /// Selects what a reply named, once the state shows it.
    fn take_wanted(&mut self) {
        let Some((row, open)) = self.wanted else {
            return;
        };
        if !self.model.rows().contains(&row) {
            return;
        }
        self.wanted = None;
        // A session opened from the Tickets screen leaves the person there
        // once they come back from it.
        if !(open && self.view == View::Tickets) {
            self.view = View::Sessions;
        }
        self.selected = Some(row);
        if let (true, Row::Session(session)) = (open, row) {
            self.mode = Mode::Terminal(session);
        }
    }

    /// A session of the queue the person is in, back at work, has had
    /// their answer or their message, and waits on them no more: the list
    /// beside it goes to the Sessions view, on that session among its
    /// project's. The keys stay with the session, where they were typing.
    fn follow_answered(&mut self) {
        if self.view == View::NeedsYou
            && let Mode::Terminal(session) = self.mode
            && self
                .model
                .session(session)
                .is_some_and(|stored| stored.status == muster_core::SessionStatus::Running)
        {
            self.view = View::Sessions;
            self.selected = Some(Row::Session(session));
            self.needs_selected = None;
        }
    }

    /// Keeps the selection on something that exists: the line that took
    /// the place of one that went away, or the last line.
    fn keep_selection(&mut self) {
        let rows = self.model.rows();
        match self
            .selected
            .and_then(|row| rows.iter().position(|r| *r == row))
        {
            Some(at) => self.selected_at = at,
            None => {
                let at = self.selected_at.min(rows.len().saturating_sub(1));
                self.selected = rows.get(at).copied();
            }
        }
        // Screens of sessions that are gone are not drawn again.
        let model = &self.model;
        self.screens.retain(|id, _| model.session(*id).is_some());
        // Questions first, then news, oldest first; but the selected session
        // keeps its place after looking at it has cleared it. Back at work,
        // it was dealt with: kept, it would stay there with nothing left to
        // move to.
        let mut list = self.model.needs_you();
        if let Some(selected) = self.needs_selected
            && !list.contains(&selected)
            && let Some(stored) = self.model.session(selected)
            && stored.status != muster_core::SessionStatus::Running
            && let Some(was) = self.needs_list.iter().position(|s| *s == selected)
        {
            list.insert(was.min(list.len()), selected);
        }
        if !self
            .needs_selected
            .is_some_and(|session| list.contains(&session))
        {
            self.needs_selected = list.first().copied();
        }
        self.needs_list = list;
    }

    /// Scrolls the lists just enough to show their selections.
    pub(super) fn keep_in_view(&mut self) {
        let full =
            ratatui::layout::Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let height = usize::from(
            crate::view::browse_areas(self, full)
                .list
                .height
                .saturating_sub(2),
        )
        .max(1);
        let rows = self.model.rows();
        let selected = self
            .selected
            .and_then(|row| rows.iter().position(|r| *r == row));
        self.list_offset = in_view(self.list_offset, selected, rows.len(), height);
        let needs = self
            .needs_selected
            .and_then(|session| self.needs_list.iter().position(|s| *s == session));
        self.needs_offset = in_view(self.needs_offset, needs, self.needs_list.len(), height);
        let tickets = self.ticket_rows();
        let ticket = self
            .tickets_selected
            .as_ref()
            .and_then(|row| tickets.iter().position(|r| r == row));
        self.tickets_offset = in_view(self.tickets_offset, ticket, tickets.len(), height);
        let settings = self.setting_rows();
        let setting = self
            .settings_selected
            .and_then(|row| settings.iter().position(|r| *r == row));
        self.settings_offset = in_view(self.settings_offset, setting, settings.len(), height);
    }
}

/// The first line to show of a list of `len` lines, `height` at a time,
/// moved from `offset` only as far as needed to show `selected`, and never
/// leaving empty space below the last line.
fn in_view(offset: usize, selected: Option<usize>, len: usize, height: usize) -> usize {
    let mut offset = offset.min(len.saturating_sub(height));
    if let Some(selected) = selected {
        if selected < offset {
            offset = selected;
        } else if selected >= offset + height {
            offset = selected + 1 - height;
        }
    }
    offset
}
