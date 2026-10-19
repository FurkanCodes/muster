//! What moves, and when it asks for drawing: running sessions spin, what
//! needs the person pulses, a session's line flashes as it finishes, ends
//! or comes to wait on the person, and Kep blinks, and is glad of a finish.
//! A name cut short on the selected line slides along, so it can be read.

use muster_core::{SessionId, SessionStatus, Timestamp};

use super::{App, Mode, View};
use crate::model::{Row, needs_attention};
use crate::motion::{FLASH_STEPS, GLAD_STEPS, NOTE_DIMS, NOTE_GOES};

/// The selected line whose name slides, from the step it was first seen
/// selected, and how far its name has slid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Slide {
    line: (View, Row),
    since: u64,
    by: usize,
}

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

    /// Whether anything waits on the steps going by: something moving, a
    /// flash or a note to let go, Kep's gladness to end, the welcome to
    /// leave. While nothing does, the runtime takes no steps and waits for
    /// what comes next.
    pub(crate) fn wants_steps(&self) -> bool {
        self.animating()
            || self.welcome
            || self.glad_at.is_some()
            || !self.flashes.is_empty()
            || self.note_age().is_some()
            || self.news_held()
            || (self.sliding_line().is_some() && crate::view::cut_name(self).is_some())
    }

    /// The runtime's next step. Whether to draw for it: something moves,
    /// or a flash has just ended and its line must settle.
    pub(crate) fn on_step(&mut self, step: u64) -> bool {
        self.step = step;
        let slid = self.slide_on(step);
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
            || slid
            || self.flashes.len() < flashing
            || self.fade_note()
            || self.animating()
    }

    /// The line whose name may slide: the selected one of *Sessions* or
    /// *Needs you*, while the list has the keys and the person looks at
    /// the window. Nothing slides with motion off.
    fn sliding_line(&self) -> Option<(View, Row)> {
        if !self.motion || self.welcome || self.mode != Mode::Browse || self.focused == Some(false)
        {
            return None;
        }
        let row = match self.view {
            View::Sessions => self.selected,
            View::NeedsYou => self.needs_selected.map(Row::Session),
            View::Tickets | View::Settings => None,
        }?;
        Some((self.view, row))
    }

    /// Slides the selected line's name on at `step`, from its start for a
    /// line newly selected: whether it moved. The runtime hands over each
    /// new step as it wakes, before what woke it, so a line left and come
    /// back to is seen to have gone, and starts again.
    fn slide_on(&mut self, step: u64) -> bool {
        let Some(line) = self.sliding_line() else {
            self.slide = None;
            return false;
        };
        let slide = match self.slide {
            Some(slide) if slide.line == line => slide,
            _ => Slide {
                line,
                since: step,
                by: 0,
            },
        };
        let by = crate::view::cut_name(self).map_or(0, |(len, room)| {
            crate::motion::slide(len, room, step.saturating_sub(slide.since))
        });
        self.slide = Some(Slide { by, ..slide });
        by != slide.by
    }

    /// How far the name on `row` of `view` has slid, as drawn now: only
    /// the line that slides has.
    pub(crate) fn slid_by(&self, view: View, row: Row) -> usize {
        self.slide
            .filter(|slide| slide.line == (view, row) && self.sliding_line() == Some(slide.line))
            .map_or(0, |slide| slide.by)
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
        self.clock_every()
            .is_some_and(|unit| before.as_millis() / unit != now.as_millis() / unit)
    }

    /// How often, in milliseconds, a time on screen reads differently, as
    /// [`on_clock`](Self::on_clock) tells; `None` while no session shows one.
    pub(crate) fn clock_every(&self) -> Option<i64> {
        let sessions = self.model.sessions();
        if sessions.is_empty() {
            return None;
        }
        let live = sessions.iter().any(|session| {
            matches!(
                session.status,
                SessionStatus::Running | SessionStatus::NeedsYou { .. }
            )
        });
        Some(if live && self.motion { 1_000 } else { 60_000 })
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
