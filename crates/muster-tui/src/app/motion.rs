//! What moves, and when it asks for drawing: running sessions spin, what
//! needs the person pulses, and a session's line flashes as it finishes,
//! ends or comes to wait on the person.

use muster_core::{SessionId, SessionStatus};

use super::App;
use crate::model::needs_attention;
use crate::motion::FLASH_STEPS;

impl App {
    /// The step of the motion to draw, or `None` to draw everything still.
    pub(crate) fn motion_step(&self) -> Option<u64> {
        self.motion.then_some(self.step)
    }

    /// Whether something on screen moves: a running session, something
    /// waiting on the person, or a line flashing. Nothing else asks for
    /// drawing.
    pub(crate) fn animating(&self) -> bool {
        self.motion
            && (!self.flashes.is_empty()
                || self.model.sessions().iter().any(|session| {
                    session.status == SessionStatus::Running || needs_attention(session)
                }))
    }

    /// The runtime's next step. Whether to draw for it: something moves,
    /// or a flash has just ended and its line must settle.
    pub(crate) fn on_step(&mut self, step: u64) -> bool {
        self.step = step;
        let flashing = self.flashes.len();
        let model = &self.model;
        self.flashes.retain(|session, began| {
            step.saturating_sub(*began) < FLASH_STEPS && model.session(*session).is_some()
        });
        self.flashes.len() < flashing || self.animating()
    }

    /// How many steps ago `session`'s line began to flash, while it does.
    pub(crate) fn flash_age(&self, session: SessionId) -> Option<u64> {
        if !self.motion {
            return None;
        }
        let age = self.step.saturating_sub(*self.flashes.get(&session)?);
        (age < FLASH_STEPS).then_some(age)
    }

    /// Flashes `session`'s line if its status has just changed to one
    /// worth a look: finished, ended, or waiting on the person. A snapshot
    /// flashes nothing, so reconnecting does not light up every line.
    pub(super) fn flash_if_changed(&mut self, session: SessionId, before: SessionStatus) {
        let Some(stored) = self.model.session(session) else {
            return;
        };
        let worth_a_look = matches!(
            stored.status,
            SessionStatus::Finished | SessionStatus::NeedsYou { .. } | SessionStatus::Exited { .. }
        );
        if self.motion && worth_a_look && stored.status != before {
            self.flashes.insert(session, self.step);
        }
    }
}
