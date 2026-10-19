//! When the list opens and closes: once the pointer has rested on the
//! notch a moment, so passing over it on the way to the menu bar opens
//! nothing, and a moment after the pointer leaves the open list, so a slip
//! past its edge does not close it. Counted in the link's steps of 100 ms;
//! pure, so it is tested anywhere.

/// Steps the pointer rests on the notch before the list opens: 200 ms.
pub const OPEN_AFTER: u64 = 2;
/// Steps the pointer is off the open list before it closes: 300 ms.
pub const CLOSE_AFTER: u64 = 3;

/// Whether the list is open, and since when the pointer has been asking
/// otherwise.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hover {
    open: bool,
    /// The step the pointer came onto the closed notch, or left the open
    /// list.
    since: Option<u64>,
}

impl Hover {
    pub fn is_open(self) -> bool {
        self.open
    }

    /// At `step`, with the pointer over what is drawn (`over`): the notch
    /// while closed, the list while open. Returns whether it is open now.
    pub fn at(&mut self, over: bool, step: u64) -> bool {
        if over == self.open {
            self.since = None;
            return self.open;
        }
        let since = *self.since.get_or_insert(step);
        let wait = if self.open { CLOSE_AFTER } else { OPEN_AFTER };
        if step.saturating_sub(since) >= wait {
            self.open = over;
            self.since = None;
        }
        self.open
    }

    /// A click on the closed notch: the list opens at once, as the person
    /// clearly asks for it.
    pub fn open_now(&mut self) {
        self.open = true;
        self.since = None;
    }

    /// Closed at once: the notch hidden, or a session picked.
    pub fn close(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_opens_once_the_pointer_rests_and_not_as_it_passes() {
        let mut hover = Hover::default();
        // Passing over the notch for a step: nothing opens.
        assert!(!hover.at(true, 10));
        assert!(!hover.at(false, 11));
        assert!(!hover.at(false, 12));
        // Resting on it: open after `OPEN_AFTER` steps.
        assert!(!hover.at(true, 20));
        assert!(!hover.at(true, 21));
        assert!(hover.at(true, 22));
        assert!(hover.is_open());
    }

    #[test]
    fn the_open_list_closes_a_moment_after_the_pointer_leaves_it() {
        let mut hover = Hover::default();
        hover.open_now();
        // A slip off its edge and back: still open.
        assert!(hover.at(false, 5));
        assert!(hover.at(false, 6));
        assert!(hover.at(true, 7));
        // Gone for `CLOSE_AFTER` steps: closed.
        assert!(hover.at(false, 8));
        assert!(hover.at(false, 10));
        assert!(!hover.at(false, 11));
        assert!(!hover.is_open());
    }

    #[test]
    fn steps_counted_again_from_nought_never_open_it_early() {
        let mut hover = Hover::default();
        assert!(!hover.at(true, 50));
        // The link reconnected and counts from nought again.
        assert!(!hover.at(true, 0));
        hover.close();
        assert!(!hover.at(true, 0));
        assert!(hover.at(true, 2));
    }
}
