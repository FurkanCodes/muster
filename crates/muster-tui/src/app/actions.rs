//! What the person can do on the selected line of *Sessions* or *Needs
//! you*, each by its key as the person's keys say (`muster_core::keys`),
//! the panels of the plugins that run among them: the bottom bar's hints
//! and the menu a right-click opens say the same, and choosing one in the
//! menu does what its key does.

use muster_core::WorkspaceKind;
use muster_core::keys::Action as Does;
use muster_core::plugins::PanelOn;

use super::{App, View};
use crate::model::Row;

/// One thing to do on the selected line, and the key that does it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Here {
    /// What it does: `None` for ⏎, which does what is selected, and for a
    /// plugin's panel.
    pub(crate) does: Option<Does>,
    /// The plugin's panel it opens or puts away, by its name in `[keys]`.
    pub(crate) panel: Option<String>,
    /// Its key as the person reads it: `⏎`, `d`, `Shift-R`, or `none`
    /// where the person took its key away.
    pub(crate) key: String,
    /// What it does, in a word or two: `full screen`, `changes`, a
    /// panel's title.
    pub(crate) what: String,
    /// What it does in a menu of spaces (`app::spaces`).
    pub(crate) choice: Option<super::spaces::Choice>,
}

impl Here {
    /// A menu's item from a hint (`⏎ turn on`, `b browser`): its key the
    /// label it starts with, which choosing it presses. None for a hint of
    /// several keys (`↑↓ move`).
    pub(crate) fn of_hint(hint: &str) -> Option<Self> {
        let (key, what) = hint.split_once(' ')?;
        crate::keys::labelled(key)?;
        Some(Self {
            does: None,
            panel: None,
            key: key.to_owned(),
            what: what.to_owned(),
            choice: None,
        })
    }

    /// The key as the person reads it.
    pub(crate) fn key_name(&self) -> String {
        self.key.clone()
    }
}

impl App {
    /// `does`'s key as the person reads it, for what Muster says: `d`,
    /// `Shift-Q`; `none` where they took its key away.
    pub(crate) fn key(&self, does: Does) -> String {
        self.keymap.label(does)
    }

    /// What can be done on the selected line of *Sessions* or *Needs
    /// you*, most used first; nothing on the other screens.
    pub(crate) fn actions_here(&self) -> Vec<Here> {
        let session = self.focused_session().and_then(|id| self.model.session(id));
        match (self.view, self.selected, session) {
            (View::Tickets | View::Settings, _, _) => Vec::new(),
            (_, _, Some(session)) if session.ended_at.is_none() => {
                let mut here = vec![
                    enter("type in it"),
                    self.here(Does::FullScreen, "full screen"),
                    self.here(Does::ShowChanges, "changes"),
                    self.here(Does::Shell, "shell"),
                ];
                here.extend(self.panels_here(true));
                here.extend([
                    self.here(Does::Rename, "rename"),
                    self.here(Does::Close, "close"),
                    self.here(Does::StopSession, "stop"),
                    self.pin_here(session.id),
                    self.here(Does::NewSession, "session"),
                ]);
                self.spaces_here(&mut here);
                here
            }
            (_, _, Some(session)) => {
                let mut here = vec![
                    enter("open"),
                    self.here(Does::FullScreen, "full screen"),
                    self.here(Does::ShowChanges, "changes"),
                    self.here(Does::Rename, "rename"),
                    self.here(Does::RestartSession, "restart"),
                    self.here(Does::Close, "remove"),
                    self.pin_here(session.id),
                    self.here(Does::NewSession, "session"),
                ];
                self.spaces_here(&mut here);
                here
            }
            (View::Sessions, Some(Row::Worktree(id)), None) => self.worktree_actions(id),
            (View::Sessions, Some(Row::Project(id)), None) => {
                let mut here = vec![enter(if self.model.is_plain_folder(id) {
                    "session"
                } else {
                    "worktree"
                })];
                here.extend(self.fold_action(Row::Project(id)));
                if !self.model.is_plain_folder(id) {
                    here.extend([
                        self.here(Does::NewSession, "session"),
                        self.here(Does::ShowTickets, "tickets"),
                    ]);
                }
                here.push(self.here(Does::Close, "remove"));
                here
            }
            (View::Sessions, Some(Row::Space(id)), None) => {
                let mut here = self.space_heading_here();
                here.extend(self.fold_action(Row::Space(id)));
                here
            }
            _ => Vec::new(),
        }
    }

    fn worktree_actions(&self, id: muster_core::WorkspaceId) -> Vec<Here> {
        let workspace = self.model.workspace(id);
        let kind = workspace.map(|workspace| &workspace.kind);
        let mut here = vec![enter("session")];
        if kind.is_some_and(WorkspaceKind::is_worktree) {
            here.push(self.here(Does::Close, "remove"));
        }
        if kind.is_some_and(|kind| !kind.in_git()) {
            // A plain folder: its checks, and nothing that needs git.
            here.push(self.here(Does::Checks, "check"));
            here.extend(self.fold_action(Row::Worktree(id)));
            return here;
        }
        if kind.is_some_and(|kind| kind.run().is_none()) {
            here.extend([
                self.here(Does::ShowChanges, "changes"),
                self.here(Does::Shell, "shell"),
            ]);
            here.extend(self.panels_here(false));
            here.extend([
                self.here(Does::Checks, "check"),
                self.here(Does::Review, "review"),
                self.here(Does::Tidy, "tidy"),
                self.here(Does::MoveBranch, "onto base"),
                self.here(Does::OpenPullRequest, "PR"),
            ]);
        }
        if workspace.is_some_and(|workspace| workspace.pull_request.is_some()) {
            here.push(self.here(Does::PullRequestInBrowser, "browser"));
        }
        here.extend(self.fold_action(Row::Worktree(id)));
        here.extend([
            self.here(Does::NewWorktree, "worktree"),
            self.here(Does::ShowTickets, "tickets"),
        ]);
        here
    }

    /// Placing `session` in a panel of its own, or taking it off the
    /// screen.
    fn pin_here(&self, session: muster_core::SessionId) -> Here {
        let placed = self.panels().placed().contains(&session);
        self.here(
            Does::PinSession,
            if placed {
                "off the screen"
            } else {
                "pin to the right"
            },
        )
    }

    /// `does`, by its key.
    fn here(&self, does: Does, what: &'static str) -> Here {
        Here {
            does: Some(does),
            panel: None,
            key: self.keymap.label(does),
            what: what.to_owned(),
            choice: None,
        }
    }

    /// The panels of the plugins that run that open on the line: beside a
    /// session and on its workspace where `on_a_session`, else on the
    /// workspace only; each by its key.
    fn panels_here(&self, on_a_session: bool) -> Vec<Here> {
        self.keymap
            .panels()
            .filter(|(action, _)| {
                self.manifest_panel(action)
                    .is_some_and(|panel| on_a_session || panel.on == PanelOn::Workspace)
            })
            .map(|(action, _)| {
                let name = action.name();
                Here {
                    does: None,
                    key: self.keymap.panel_label(&name),
                    panel: Some(name),
                    what: self.keymap.panel_title(action),
                    choice: None,
                }
            })
            .collect()
    }

    /// What `e` does on a project's or a worktree's line, where it can.
    fn fold_action(&self, row: Row) -> Option<Here> {
        if self.is_folded(row) {
            Some(self.here(Does::Fold, "unfold"))
        } else {
            self.foldable(row).then(|| self.here(Does::Fold, "fold"))
        }
    }
}

/// What ⏎ does here.
fn enter(what: &'static str) -> Here {
    Here {
        does: None,
        panel: None,
        key: "⏎".to_owned(),
        what: what.to_owned(),
        choice: None,
    }
}
