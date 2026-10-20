//! A burst of output shares one frame; input and its first echo are immediate.
//! Synchronized output still holds every draw until the frame ends (ARC-13).

use std::time::Instant;

use super::{FRAME, FrameHold};

pub(super) struct Draws {
    pub(super) dirty: bool,
    next: Instant,
    echo: Option<muster_core::SessionId>,
    hold: FrameHold,
}

impl Draws {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            dirty: true,
            next: now,
            echo: None,
            hold: FrameHold::default(),
        }
    }

    pub(super) fn woke(&mut self, woke: &super::Woke, now: Instant) {
        use crossterm::event::Event;
        use muster_core::protocol::ServerFrame;
        match woke {
            super::Woke::Input(Some(Ok(Event::Key(_) | Event::Paste(_)))) => self.input(now),
            super::Woke::Link(super::LinkEvent::Frame(frame)) => match frame.as_ref() {
                ServerFrame::Output { session, .. } | ServerFrame::Replay { session, .. } => {
                    self.output(*session, now);
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub(super) fn actions(&mut self, actions: &[super::Action]) {
        for action in actions {
            if let super::Action::Send(muster_core::protocol::ClientFrame::Input {
                session, ..
            }) = action
            {
                self.expect_echo(*session);
            }
        }
    }

    pub(super) fn wake(&self) -> Option<Instant> {
        self.dirty.then_some(self.next)
    }

    pub(super) fn input(&mut self, now: Instant) {
        self.next = now;
    }

    pub(super) fn expect_echo(&mut self, session: muster_core::SessionId) {
        self.echo = Some(session);
    }

    pub(super) fn output(&mut self, session: muster_core::SessionId, now: Instant) {
        if self.echo == Some(session) {
            self.echo = None;
            self.next = now;
        }
    }

    pub(super) fn due(&mut self, pending: bool, now: Instant) -> bool {
        if !self.dirty || now < self.next {
            return false;
        }
        if self.hold.may_draw(pending, now) {
            return true;
        }
        self.next = now + FRAME;
        false
    }

    pub(super) fn drawn(&mut self, now: Instant) {
        self.dirty = false;
        self.next = now + FRAME;
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "ARC-03: tests supply monotonic time"
)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn streaming_coalesces_but_input_and_its_echo_draw_at_once_arc_13() {
        let start = Instant::now();
        let mut draws = Draws::new(start);
        let session = muster_core::SessionId::new();
        assert!(draws.due(false, start));
        draws.drawn(start);
        assert_eq!(draws.wake(), None, "no periodic idle draw");
        for ms in 1..16 {
            let now = start + Duration::from_millis(ms);
            draws.output(session, now);
            draws.dirty = true;
            assert!(!draws.due(false, now));
        }
        let key = start + Duration::from_millis(15);
        draws.input(key);
        draws.expect_echo(session);
        draws.output(muster_core::SessionId::new(), key);
        assert!(draws.due(false, key));
        draws.drawn(key);
        let echo = key + Duration::from_millis(1);
        draws.output(session, echo);
        draws.dirty = true;
        assert!(draws.due(false, echo));
        draws.drawn(echo);
        draws.output(session, echo);
        draws.dirty = true;
        assert!(
            !draws.due(false, echo),
            "only the first echo bypasses the interval"
        );
        assert!(draws.due(false, echo + FRAME));
    }

    #[test]
    fn input_never_exposes_a_half_drawn_frame_arc_13() {
        let start = Instant::now();
        let mut draws = Draws::new(start);
        draws.input(start);
        assert!(!draws.due(true, start));
        let end = start + FRAME;
        assert!(draws.due(false, end));
        draws.drawn(end);
        draws.dirty = true;
        assert!(!draws.due(true, end + FRAME));
        assert!(draws.due(true, end + FRAME + super::super::FRAME_WAIT));
    }
}
