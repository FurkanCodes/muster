//! News for a person who looks elsewhere, with fewer false alarms, and a
//! way back to it. A session that finishes is told of only once it has
//! stayed finished a moment (`HOLD_STEPS`, timed by the steps the runtime
//! counts from its own clock, `ARC-03`): an agent often ends a turn and
//! starts another at once, as when a hook or a subagent sends it on, and
//! that is no news. A wait, or a program that ends, is told at once. `l`
//! goes to the session told of last.

use muster_core::{SessionId, SessionStatus};

use super::{Action, App, Mode, View};
use crate::model::{Row, needs_attention};

/// How long a finish is held before it is told: two seconds of steps.
pub(crate) const HOLD_STEPS: u64 = 20;

impl App {
    /// `session` has just come to need the person: told at once, or for a
    /// finish, held a moment.
    pub(super) fn on_news(&mut self, session: SessionId) -> Option<Action> {
        let stored = self.model.session(session)?;
        if !needs_attention(stored) {
            return None;
        }
        if stored.status == SessionStatus::Finished {
            self.alarms.held.retain(|(held, _)| *held != session);
            self.alarms.held.push((session, self.moving.step));
            return None;
        }
        self.alarms.last = Some(session);
        self.news(session)
    }

    /// The finishes held long enough, told now if the sessions are still
    /// finished and unseen; those at work again by then are dropped. The
    /// runtime asks after each step.
    pub(crate) fn due_news(&mut self) -> Vec<Action> {
        let step = self.moving.step;
        let (due, held): (Vec<_>, Vec<_>) = std::mem::take(&mut self.alarms.held)
            .into_iter()
            .partition(|(_, at)| step.saturating_sub(*at) >= HOLD_STEPS);
        self.alarms.held = held;
        let mut actions = Vec::new();
        for (session, _) in due {
            let still = self.model.session(session).is_some_and(|stored| {
                stored.status == SessionStatus::Finished && needs_attention(stored)
            });
            if still {
                self.alarms.last = Some(session);
                actions.extend(self.news(session));
            }
        }
        // One gone meanwhile is never told.
        let model = &self.model;
        self.alarms
            .held
            .retain(|(session, _)| model.session(*session).is_some());
        actions
    }

    /// Whether a finish is held, waiting on the steps to be told.
    pub(crate) fn news_held(&self) -> bool {
        !self.alarms.held.is_empty()
    }

    /// `l`: the session told of last, selected on the Sessions screen,
    /// whatever folds it.
    pub(super) fn go_to_last_alarm(&mut self) -> Vec<Action> {
        let Some(session) = self
            .alarms
            .last
            .filter(|session| self.model.session(*session).is_some())
        else {
            self.complain("No session has needed you since Muster started.");
            return Vec::new();
        };
        self.view = View::Sessions;
        self.mode = Mode::Browse;
        self.unfold_to(Row::Session(session));
        self.selected = Some(Row::Session(session));
        self.settle()
    }
}
