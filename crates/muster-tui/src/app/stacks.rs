//! Stacked work (`PRODUCT.md` §6, `PRD-14`): a ticket's new worktree made
//! on top of another worktree's branch, chosen with ←→ in the dialog
//! assigning an agent; and `u`, once the person says yes, moving a
//! worktree's branch onto the newest of the branch it is stacked on, or of
//! its base once that has merged, or of its base where it is not stacked.

use muster_core::protocol::Command;
use muster_core::stacks::STACKS_PROTOCOL;
use muster_core::text::one_line;
use muster_core::{ProjectId, PullRequestState, TicketKey, WorkspaceId};

use super::{Action, App, Confirmed, Dialog, Mode, Then};

impl App {
    /// Whether the daemon knows stacked work.
    pub(crate) fn knows_stacks(&self) -> bool {
        self.daemon_protocol >= STACKS_PROTOCOL
    }

    /// The worktrees of `project` a new one can be made on top of.
    pub(crate) fn stack_choices(&self, project: ProjectId) -> Vec<WorkspaceId> {
        self.model
            .workspaces_of(project)
            .filter(|workspace| workspace.kind.is_worktree())
            .filter(|workspace| {
                let branch = workspace.kind.branch();
                !branch.is_empty() && !branch.starts_with("detached at ")
            })
            .map(|workspace| workspace.id)
            .collect()
    }

    /// Whether the dialog assigning an agent to ticket `key` in `into`
    /// offers to make its worktree on top of another's: only where it
    /// makes one.
    pub(crate) fn offers_stack(&self, into: ProjectId, key: &TicketKey, another: bool) -> bool {
        self.knows_stacks()
            && (another || self.model.worktrees_for(&[into], key).is_empty())
            && !self.stack_choices(into).is_empty()
    }

    /// The choice after `on` (before it with `back`), round through the
    /// base, which is `None`.
    pub(super) fn next_stack(
        &self,
        into: ProjectId,
        on: Option<WorkspaceId>,
        back: bool,
    ) -> Option<WorkspaceId> {
        let mut choices: Vec<Option<WorkspaceId>> = vec![None];
        choices.extend(self.stack_choices(into).into_iter().map(Some));
        let at = choices.iter().position(|choice| *choice == on).unwrap_or(0);
        let next = if back {
            (at + choices.len() - 1) % choices.len()
        } else {
            (at + 1) % choices.len()
        };
        choices[next]
    }

    /// A worktree's line in a dialog: `⎇ muster/issue-12-fix-the-login`.
    pub(crate) fn worktree_label(&self, workspace: WorkspaceId) -> String {
        format!(
            "⎇ {}",
            one_line(self.model.branch(workspace).unwrap_or("?"))
        )
    }

    /// Whether the branch `workspace` is stacked on has merged, as its
    /// worktree's pull request last said, or is no longer a workspace's.
    pub(crate) fn stack_merged(&self, workspace: WorkspaceId) -> bool {
        let Some(found) = self.model.workspace(workspace) else {
            return false;
        };
        let Some(stacked) = &found.stacked_on else {
            return false;
        };
        let parent = self
            .model
            .workspaces_of(found.project)
            .find(|other| other.kind.branch() == stacked.branch);
        parent.is_none_or(|parent| {
            parent
                .pull_request
                .as_ref()
                .is_some_and(|pull| pull.state == PullRequestState::Merged)
        })
    }

    /// `u`: asks whether to move the selected worktree's branch.
    pub(super) fn ask_move(&mut self) -> Vec<Action> {
        let Some(workspace) = self.hands_on_workspace("move its branch onto its base") else {
            return Vec::new();
        };
        if !self.knows_stacks() {
            self.complain(
                "The daemon runs an older muster that cannot move a branch: `muster reload` \
                 moves it onto this one, every session running on.",
            );
            return Vec::new();
        }
        let Some(found) = self.model.workspace(workspace) else {
            return Vec::new();
        };
        if !found.kind.in_git() {
            self.complain("It is not a git repository, so it has no branch to move.");
            return Vec::new();
        }
        let branch = one_line(found.kind.branch());
        let onto = match &found.stacked_on {
            Some(_) if self.stack_merged(workspace) => {
                "the newest of its base, as the branch it was \
                 stacked on has merged"
                    .to_owned()
            }
            Some(stacked) => format!("the newest of ⎇ {}", one_line(&stacked.branch)),
            None => "the newest of its base".to_owned(),
        };
        let question = format!(
            "Move {branch} onto {onto}? Its own commits are replayed there; if one would \
             conflict, nothing changes. What is there now is kept first, so z in d puts it back."
        );
        self.mode = Mode::Dialog(Dialog::Confirm {
            question,
            then: Confirmed::MoveOnto(workspace),
        });
        Vec::new()
    }

    /// The person said yes to moving the branch.
    pub(super) fn confirmed_move(&mut self, workspace: WorkspaceId) -> Action {
        self.say("Moving the branch…");
        self.command(Command::MoveOnto { workspace }, Then::Moved(workspace))
    }

    /// The branch moved, or why not.
    pub(super) fn on_moved(
        &mut self,
        workspace: WorkspaceId,
        outcome: Result<(String, u64, u64), String>,
    ) {
        let (onto, commits, entry) = match outcome {
            Ok(done) => done,
            Err(error) => {
                self.complain(error);
                return;
            }
        };
        let branch = one_line(self.model.branch(workspace).unwrap_or("The branch"));
        let commits = match commits {
            1 => "its 1 commit".to_owned(),
            count => format!("its {count} commits"),
        };
        let pushed = self
            .model
            .workspace(workspace)
            .and_then(|found| found.pull_request.as_ref())
            .is_some_and(|pull| pull.state.is_open());
        let push = if pushed {
            " Its pull request follows once it is pushed with --force-with-lease."
        } else {
            ""
        };
        self.say(format!(
            "{branch} is on {} now, {commits} replayed there; z on #{entry} in d puts it back.{push}",
            one_line(&onto)
        ));
    }
}

#[cfg(test)]
#[path = "stacks_tests.rs"]
mod tests;
