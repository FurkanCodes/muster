//! Which notifications, and how each sounds, on the Settings screen
//! (`PRODUCT.md` §6): a row for questions, finishes and errors, each told
//! or not with ⏎, its sound chosen with ← →, heard as it is chosen on a
//! Mac. Kept in Muster's own settings by a daemon of protocol 40 or later.

use muster_core::protocol::{Command, ErrorBody};
use muster_core::settings::PreferenceChange;
use muster_core::telling::{Kind, Sound, Tell};

use super::{Action, App, Then};

impl App {
    /// ⏎ on `kind`'s row: told, or not, with the sound it has.
    pub(super) fn toggle_tell(&mut self, kind: Kind) -> Vec<Action> {
        let tell = self.model.preferences().telling.of(kind).clone();
        self.change_tell(
            kind,
            Tell {
                on: !tell.on,
                ..tell
            },
        )
    }

    /// ← → on `kind`'s row: the sound before or after its own, heard on a
    /// Mac as it is chosen; told again if it was not.
    pub(super) fn step_sound(&mut self, kind: Kind, by: isize) -> Vec<Action> {
        let sound = self.model.preferences().telling.of(kind).sound.step(by);
        let mut actions = self.change_tell(
            kind,
            Tell {
                on: true,
                sound: sound.clone(),
            },
        );
        // The Mac's own sound is the person's alert sound, which only a
        // notification plays.
        if !actions.is_empty() && self.notch_here && matches!(sound, Sound::Named(_)) {
            actions.push(Action::Hear(sound));
        }
        actions
    }

    fn change_tell(&mut self, kind: Kind, tell: Tell) -> Vec<Action> {
        if self.telling_changing() {
            return Vec::new();
        }
        vec![self.command(
            Command::ChangePreference(PreferenceChange::Tell {
                kind,
                tell: tell.clone(),
            }),
            Then::TellChanged(kind, tell),
        )]
    }

    /// `kind` is told as `tell` now, or could not be. The settings as they
    /// are now come as a change to the state.
    pub(super) fn tell_changed(&mut self, kind: Kind, tell: &Tell, outcome: Result<(), ErrorBody>) {
        match outcome {
            Ok(()) if tell.on => self.say(format!(
                "{} are told, with {}.",
                kind.label(),
                match &tell.sound {
                    Sound::Silent => "no sound".to_owned(),
                    sound => sound.to_string(),
                }
            )),
            Ok(()) => self.say(format!("{} are not told.", kind.label())),
            Err(error) => self.complain(error.message),
        }
    }
}
