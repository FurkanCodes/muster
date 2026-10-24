//! Tidying a branch's commits (`t`, `PRODUCT.md` §6, `PRD-14`): which
//! agent tidies them is asked first, and only then is the daemon asked,
//! which refuses while an agent works there. The session's line says what
//! the tidy came to once its agent stops.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::WorkspaceId;
use muster_core::protocol::Command;

use super::{Action, App, Dialog, HarnessChoice, Mode, Then};

impl App {
    /// `t`: asks which agent tidies the selected workspace's commits.
    pub(super) fn ask_tidy(&mut self) -> Vec<Action> {
        let Some(workspace) = self.hands_on_workspace("tidy its commits") else {
            return Vec::new();
        };
        if self
            .model
            .workspace(workspace)
            .is_some_and(|found| !found.kind.in_git())
        {
            self.complain("It is not a git repository, so it has no commits to tidy.");
            return Vec::new();
        }
        let harness = if self.last_harness.takes_prompt() {
            self.last_harness.clone()
        } else {
            HarnessChoice::Claude
        };
        self.mode = Mode::Dialog(Dialog::Tidy { workspace, harness });
        Vec::new()
    }

    /// A key in the dialog asking which agent tidies the commits.
    pub(super) fn tidy_key(
        &mut self,
        workspace: WorkspaceId,
        harness: HarnessChoice,
        key: &KeyEvent,
    ) -> Vec<Action> {
        let harness = match key.code {
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                harness.next_agent(self.added_agents())
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                harness.previous_agent(self.added_agents())
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                self.say("Starting the tidy…");
                let size = self.session_size();
                return vec![self.command(
                    Command::TidyCommits {
                        workspace,
                        harness: harness.id().to_owned(),
                        size,
                    },
                    Then::OpenSession,
                )];
            }
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                return Vec::new();
            }
            _ => harness,
        };
        self.mode = Mode::Dialog(Dialog::Tidy { workspace, harness });
        Vec::new()
    }
}

#[cfg(test)]
#[path = "tidy_tests.rs"]
mod tests;
