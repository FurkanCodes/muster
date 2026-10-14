//! The welcome: Kep and Muster's name fill the window as the UI starts,
//! for a second at least and until the state has come (or the daemon is
//! found unreachable, which the screens behind say more of). A key lets it
//! go and does what it says, as a person who knows the screens types
//! ahead; a click or the wheel only lets it go, as it points at nothing
//! the person can see.

use super::{App, Link};

/// The steps the welcome stays at least: a second.
pub(crate) const WELCOME_STEPS: u64 = 10;

impl App {
    /// Ends the welcome once it is due to go; whether it went.
    pub(super) fn end_welcome_when_due(&mut self) -> bool {
        let ready = self.model.loaded() || matches!(self.link, Link::Lost(_));
        if self.welcome && ready && self.step >= WELCOME_STEPS {
            self.welcome = false;
            return true;
        }
        false
    }

    /// The person did something: the welcome goes, if it is up. Whether
    /// it was.
    pub(super) fn skip_welcome(&mut self) -> bool {
        std::mem::take(&mut self.welcome)
    }
}
