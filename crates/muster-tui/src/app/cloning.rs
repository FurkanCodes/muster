//! Cloning a project from GitHub (`g`): the repositories the person can
//! reach, as the daemon lists them through their own `gh`, narrowed by what
//! they type, or one they name (`owner/name`, or its github.com address).
//! ↑↓ choose one; ⏎ goes on to the folder to clone it into, picked as a
//! folder to add is (`picker`), where ⏎ has the daemon clone it there and
//! add it.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::protocol::Command;
use muster_core::protocol::repositories::{HostedRepository, repository_name};

use super::{Action, App, Dialog, Mode, Then};
use crate::input::{Edit, LineInput};

/// How many repositories the dialog shows at once.
pub(crate) const CLONE_ROWS: usize = 8;

/// The repositories offered and the choice among them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CloneChoice {
    /// What the person typed: narrows the list, or names one.
    pub(crate) typed: LineInput,
    /// The repositories, once the daemon answered, or why it could not.
    pub(crate) listed: Option<Result<Vec<HostedRepository>, String>>,
    /// The one chosen among those shown; none is what is typed.
    pub(crate) selected: Option<usize>,
    /// The first shown: the list moves only when the choice would leave it.
    pub(crate) offset: usize,
    /// Why ⏎ did nothing, said in the dialog.
    pub(crate) refused: Option<String>,
}

impl CloneChoice {
    /// The repositories whose names or descriptions hold what is typed,
    /// ignoring case, those whose names do first.
    pub(crate) fn shown(&self) -> Vec<&HostedRepository> {
        let Some(Ok(listed)) = &self.listed else {
            return Vec::new();
        };
        let typed = self.typed.text().trim().to_lowercase();
        let named = |repository: &HostedRepository| repository.name.to_lowercase().contains(&typed);
        let described = |repository: &HostedRepository| {
            repository
                .description
                .as_deref()
                .is_some_and(|text| text.to_lowercase().contains(&typed))
        };
        let mut shown: Vec<&HostedRepository> = listed
            .iter()
            .filter(|repository| named(repository) || described(repository))
            .collect();
        // Stable, so each keeps GitHub's order, the most recently pushed first.
        shown.sort_by_key(|repository| !named(repository));
        shown
    }

    /// The repository ⏎ clones: the one chosen, else the one typed.
    fn chosen(&self) -> Option<String> {
        match self.selected.and_then(|at| self.shown().get(at).copied()) {
            Some(repository) => Some(repository.name.clone()),
            None => repository_name(self.typed.text()),
        }
    }

    fn step(&mut self, by: isize) {
        let count = self.shown().len();
        self.selected = match (self.selected, by) {
            (_, _) if count == 0 => None,
            (None, by) if by > 0 => Some(0),
            (None, _) => None,
            (Some(0), by) if by < 0 => None,
            (Some(at), by) => Some(at.saturating_add_signed(by).min(count - 1)),
        };
        if let Some(at) = self.selected {
            if at < self.offset {
                self.offset = at;
            } else if at >= self.offset + CLONE_ROWS {
                self.offset = at + 1 - CLONE_ROWS;
            }
        }
    }
}

impl App {
    /// `g`: the repositories the person can reach on GitHub, asked of the daemon.
    pub(super) fn open_clone_project(&mut self) -> Vec<Action> {
        self.mode = Mode::Dialog(Dialog::CloneProject(CloneChoice::default()));
        vec![self.command(Command::ListRepositories, Then::Repositories)]
    }

    /// The daemon's list, for the dialog if it is still open.
    pub(super) fn on_repositories(&mut self, listed: Result<Vec<HostedRepository>, String>) {
        if let Mode::Dialog(Dialog::CloneProject(choice)) = &mut self.mode {
            choice.listed = Some(listed);
        }
    }

    /// A key in the dialog: ↑↓ choose, ⏎ goes on to where to clone it.
    pub(super) fn clone_key(&mut self, mut choice: CloneChoice, key: &KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Down => choice.step(1),
            KeyCode::Up => choice.step(-1),
            _ => match choice.typed.handle(key) {
                Edit::Submit => {
                    let Some(name) = choice.chosen() else {
                        choice.refused = Some(
                            "Choose one, or type its name as owner/name or its github.com address."
                                .to_owned(),
                        );
                        self.mode = Mode::Dialog(Dialog::CloneProject(choice));
                        return Vec::new();
                    };
                    return self.open_clone_into(name);
                }
                Edit::Cancel => {
                    self.mode = Mode::Browse;
                    return Vec::new();
                }
                Edit::Changed => {
                    // A name typed chooses its first match.
                    choice.selected = (!choice.typed.text().trim().is_empty()
                        && !choice.shown().is_empty())
                    .then_some(0);
                    choice.offset = 0;
                    choice.refused = None;
                }
                Edit::Ignored => {}
            },
        }
        self.mode = Mode::Dialog(Dialog::CloneProject(choice));
        Vec::new()
    }

    /// Pasted text goes into what is typed, as typing does.
    pub(super) fn clone_paste(&mut self, mut choice: CloneChoice, text: &str) -> Vec<Action> {
        choice.typed.paste(text);
        choice.selected = None;
        choice.offset = 0;
        choice.refused = None;
        self.mode = Mode::Dialog(Dialog::CloneProject(choice));
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listed() -> CloneChoice {
        let repository = |name: &str, description: Option<&str>| HostedRepository {
            name: name.to_owned(),
            description: description.map(str::to_owned),
            private: false,
        };
        CloneChoice {
            listed: Some(Ok(vec![
                repository("o/site", Some("The app's website")),
                repository("o/app", None),
                repository("o/tools", None),
            ])),
            ..CloneChoice::default()
        }
    }

    #[test]
    fn what_is_typed_narrows_by_name_first_then_description() {
        let mut choice = listed();
        choice.typed = LineInput::with("app");
        let names: Vec<_> = choice.shown().iter().map(|one| one.name.as_str()).collect();
        assert_eq!(names, ["o/app", "o/site"]);
    }

    #[test]
    fn enter_clones_the_one_chosen_else_the_one_named() {
        let mut choice = listed();
        choice.step(1);
        choice.step(1);
        assert_eq!(choice.chosen().as_deref(), Some("o/app"));
        choice.step(-1);
        choice.step(-1);
        assert_eq!(choice.selected, None);
        choice.typed = LineInput::with("https://github.com/x/y.git");
        assert_eq!(choice.chosen().as_deref(), Some("x/y"));
        choice.typed = LineInput::with("not one");
        assert_eq!(choice.chosen(), None);
    }
}
