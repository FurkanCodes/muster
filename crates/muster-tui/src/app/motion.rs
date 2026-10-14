//! What moves, and when it asks for drawing: running sessions spin, what
//! needs the person pulses, a session's line flashes as it finishes, ends
//! or comes to wait on the person, and Kep blinks, and is glad of a finish.

use muster_core::{SessionId, SessionStatus, Timestamp};

use super::App;
use crate::model::needs_attention;
use crate::motion::{FLASH_STEPS, GLAD_STEPS, NOTE_DIMS, NOTE_GOES};

impl App {
    /// The step of the motion to draw, or `None` to draw everything still.
    pub(crate) fn motion_step(&self) -> Option<u64> {
        self.motion.then_some(self.step)
    }

    /// Whether something on screen moves: a running session, something
    /// waiting on the person, a line flashing or Kep glad. Nothing else
    /// asks for drawing.
    pub(crate) fn animating(&self) -> bool {
        self.motion
            && (!self.flashes.is_empty()
                || self.kep_glad()
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
        let welcomed = self.end_welcome_when_due();
        let glad_over = self.glad_at.is_some() && !self.kep_glad();
        if glad_over {
            self.glad_at = None;
        }
        welcomed
            || glad_over
            || self.flashes.len() < flashing
            || self.fade_note()
            || self.animating()
    }

    /// Whether Kep is glad of a session that has just finished.
    pub(crate) fn kep_glad(&self) -> bool {
        self.motion
            && self
                .glad_at
                .is_some_and(|at| self.step.saturating_sub(at) < GLAD_STEPS)
    }

    /// Lets a note go once it has been read, dimming it first; an error
    /// stays. Whether that changed what is drawn.
    fn fade_note(&mut self) -> bool {
        let Some(age) = self.note_age() else {
            return false;
        };
        if age >= NOTE_GOES {
            self.note = None;
            // The bottom bar may be a line or two shorter now.
            self.keep_in_view();
            return true;
        }
        self.motion && age >= NOTE_DIMS
    }

    /// How many steps a note that fades has been shown, while it is.
    pub(crate) fn note_age(&self) -> Option<u64> {
        let note = self.note.as_ref().filter(|note| !note.error)?;
        Some(self.step.saturating_sub(note.shown_at))
    }

    /// Whether a note is fading out now.
    pub(crate) fn note_fading(&self) -> bool {
        self.motion && self.note_age().is_some_and(|age| age >= NOTE_DIMS)
    }

    /// The runtime read the clock. Whether a time on screen reads
    /// differently now: each second while a session runs or waits, each
    /// minute otherwise; each minute only, with motion off.
    pub(crate) fn on_clock(&mut self, now: Timestamp) -> bool {
        let Some(before) = self.now.replace(now) else {
            return true;
        };
        let sessions = self.model.sessions();
        if sessions.is_empty() {
            return false;
        }
        let live = sessions.iter().any(|session| {
            matches!(
                session.status,
                SessionStatus::Running | SessionStatus::NeedsYou { .. }
            )
        });
        let unit = if live && self.motion { 1_000 } else { 60_000 };
        before.as_millis() / unit != now.as_millis() / unit
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
            if stored.status == SessionStatus::Finished {
                self.glad_at = Some(self.step);
            }
        }
    }
}
