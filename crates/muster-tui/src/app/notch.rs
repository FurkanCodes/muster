//! Muster's notch (`ARC-22`): on a Mac, the UI starts it, and it shows the
//! agents in the notch at the top of the screen while the person looks
//! elsewhere. So the UI tells the daemon whether the person is looking at
//! its window, shows a session the person picks in the notch, and turns the
//! notch on and off on the Settings screen.

use muster_core::SessionId;
use muster_core::protocol::{ClientFrame, Command, ErrorBody};
use muster_core::settings::PreferenceChange;

use super::{Action, App, Link, Then, View};
use crate::model::Row;

impl App {
    /// Whether the daemon knows the notch, and Muster's own settings.
    pub(crate) fn notch_known(&self) -> bool {
        self.notch_here
    }

    /// Tells the daemon whether the person is looking at this window; a
    /// terminal that never says counts as looked at.
    pub(super) fn looking(&self) -> Option<Action> {
        (matches!(self.link, Link::Connected { .. })).then(|| {
            Action::Send(ClientFrame::Looking {
                looking: self.focused != Some(false),
            })
        })
    }

    /// Starts Muster's helper once the state says it is wanted, for the
    /// notch or for notifications of the Mac's own; turned off, it stops
    /// itself, and is started again once turned back on.
    pub(super) fn start_notch(&mut self) -> Option<Action> {
        let preferences = self.model.preferences();
        let notifies = preferences.notifications;
        let wanted = self.notch_known() && self.model.loaded() && (preferences.notch || notifies);
        if !wanted {
            self.notch_started = false;
            return None;
        }
        if self.notch_started {
            return None;
        }
        self.notch_started = true;
        Some(Action::StartNotch)
    }

    /// The person picked a session in the notch: it opens here, with the
    /// keys beside the list, and the terminal comes to the front.
    pub(super) fn reveal(&mut self, session: SessionId) -> Vec<Action> {
        if self.model.session(session).is_none() {
            return Vec::new();
        }
        self.welcome = false;
        self.view = View::Sessions;
        self.wanted = Some((Row::Session(session), true));
        let mut actions = vec![Action::Activate];
        actions.extend(self.settle());
        actions
    }

    /// `muster attach`: the session opens as one picked in the notch does,
    /// with the keys in it, once the state shows it.
    pub(crate) fn open_at_start(&mut self, session: SessionId) {
        self.welcome = false;
        self.view = View::Sessions;
        self.wanted = Some((Row::Session(session), true));
    }

    /// ⏎ on *Notifications*: on, or off.
    pub(super) fn toggle_notifications(&mut self) -> Vec<Action> {
        let on = !self.model.preferences().notifications;
        let said = if on {
            "You are told of what newly needs you while you look away."
        } else {
            "Nothing is told: the waiting count and Needs you still say it."
        };
        vec![self.command(
            Command::ChangePreference(PreferenceChange::Notifications(on)),
            Then::Say(said),
        )]
    }

    /// Turns the notch on or off.
    pub(super) fn toggle_notch(&mut self) -> Vec<Action> {
        if !self.notch_known() || self.notch_changing() {
            return Vec::new();
        }
        let on = !self.model.preferences().notch;
        vec![self.command(
            Command::ChangePreference(PreferenceChange::Notch(on)),
            Then::NotchChanged(on),
        )]
    }

    /// The notch was turned on (`on`) or off, or could not be. The
    /// settings as they are now come as a change to the state, which may
    /// come before this answer or after it.
    pub(super) fn notch_changed(&mut self, on: bool, outcome: Result<(), ErrorBody>) {
        match outcome {
            Ok(()) if on => {
                self.say("The notch shows your agents while you look elsewhere.");
            }
            Ok(()) => self.say("Muster leaves the notch alone."),
            Err(error) => self.complain(error.message),
        }
    }
}
