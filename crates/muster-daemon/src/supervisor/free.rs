//! Free terminals, in no project (`ARC-33`), and how one comes to be in a
//! project. Muster never types into a person's shell: a terminal whose
//! shell is already in a project's folder is filed there in place, the
//! same shell running on; anywhere else, a new terminal is started there
//! and the old one closed, saying first what ends.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::spaces::Member;
use muster_core::{SessionId, SpaceId, Workspace, WorkspaceId};

use super::sessions::{Asked, home_folder};
use super::{Supervisor, store_closed, store_error};
use crate::fronts;

/// Terminals closed by a restart elsewhere, taken off the list as they end
/// (`ARC-14`: each leaves as its program does).
#[derive(Debug, Default)]
pub(super) struct Moving(HashSet<SessionId>);

impl Supervisor {
    /// The folder `session`'s shell is in now, and what runs in front of
    /// it; each `None` where it cannot be read. Read only when asked.
    async fn shell_now(&self, session: SessionId) -> (Option<PathBuf>, Option<String>) {
        let Some(pty) = self
            .live
            .get(&session)
            .filter(|live| !live.machine.status().is_exited())
            .map(|live| live.pty.clone())
        else {
            return (None, None);
        };
        let Some(shell) = pty.pid() else {
            return (None, None);
        };
        let folder = fronts::shell_folder(shell).await;
        let front = match pty.foreground().await {
            Some(group) if group != shell => fronts::program(group).await,
            _ => None,
        };
        (folder, front)
    }

    /// The workspace of this daemon's projects whose folder holds `folder`,
    /// the deepest where several do.
    fn workspace_holding(&self, folder: &Path) -> Option<&Workspace> {
        holding(folder, &self.state.workspaces)
    }

    pub(super) async fn read_place(&mut self, session: SessionId) -> Result<Reply, ErrorBody> {
        self.stored_session(session)?;
        let (folder, front) = self.shell_now(session).await;
        if folder.is_none() {
            tracing::debug!(%session, "a terminal's folder could not be read");
        }
        let workspace = folder
            .as_deref()
            .and_then(|folder| self.workspace_holding(folder))
            .map(|workspace| workspace.id);
        Ok(Reply::Place {
            folder,
            workspace,
            front,
        })
    }

    /// The folder free terminal `session`'s shell is in, read again now,
    /// or why it cannot be filed.
    async fn free_folder(&self, session: SessionId) -> Result<PathBuf, ErrorBody> {
        if !self.stored_session(session)?.is_free() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "it is in a project already",
            ));
        }
        self.shell_now(session).await.0.ok_or_else(|| {
            tracing::info!(%session, "refused to file a terminal: its folder cannot be read");
            ErrorBody::new(
                ErrorCode::Conflict,
                "the folder its shell is in cannot be read: start a new terminal in the project",
            )
        })
    }

    pub(super) async fn file_session(
        &mut self,
        session: SessionId,
        workspace: WorkspaceId,
    ) -> Result<Reply, ErrorBody> {
        let folder = self.free_folder(session).await?;
        let target = self.workspace(workspace)?;
        let inside = target
            .kind
            .host_path()
            .is_some_and(|path| within(&folder, path));
        if !inside {
            tracing::info!(%session, "refused to file a terminal: its shell is elsewhere");
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!(
                    "its shell is in {}, not in that folder: change folder in it first, or \
                     start a new terminal there",
                    folder.display()
                ),
            ));
        }
        self.store
            .call(move |store| store.file_session(session, workspace))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        tracing::info!(%session, %workspace, "a free terminal filed under a project");
        if let Some(stored) = self.session_mut(session) {
            stored.workspace = Some(workspace);
            let stored = stored.clone();
            self.broadcast(Delta::SessionUpserted(stored));
        }
        self.forget_members(|member| member == Member::Session(session));
        Ok(Reply::Done)
    }

    pub(super) async fn add_project_here(
        &mut self,
        session: SessionId,
        space: Option<SpaceId>,
    ) -> Result<Reply, ErrorBody> {
        let folder = self.free_folder(session).await?;
        let home = home_folder()?;
        if within(&home, &folder) {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!(
                    "{} holds the home folder: change to a project's folder in it first",
                    folder.display()
                ),
            ));
        }
        if let Some(found) = self.workspace_holding(&folder) {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!(
                    "{} is in a project already: file the terminal under it",
                    found.kind.host_path().unwrap_or(&folder).display()
                ),
            ));
        }
        // The project goes in the terminal's space, unless another is asked.
        let space = space.or_else(|| self.space_of(Member::Session(session)));
        let reply = self.add_project(folder, None, space).await?;
        let Reply::ProjectAdded { id } = reply else {
            return Ok(reply);
        };
        let checkout = self
            .state
            .workspaces
            .iter()
            .find(|workspace| workspace.project == id && workspace.kind.is_checkout())
            .map(|workspace| workspace.id);
        if let Some(checkout) = checkout
            && let Err(error) = self.file_session(session, checkout).await
        {
            // The project stays: it was asked for, and is there.
            tracing::info!(%session, error = %error.message, "the terminal was not filed");
            return Err(error);
        }
        Ok(Reply::ProjectAdded { id })
    }

    pub(super) async fn restart_in(
        &mut self,
        session: SessionId,
        workspace: WorkspaceId,
    ) -> Result<Reply, ErrorBody> {
        let old = self.stored_session(session)?;
        if !old.is_terminal() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "an agent's work is in its workspace: start another where you want one",
            ));
        }
        if old.agent.is_some() {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                "an agent runs in it, whose conversation would end: quit it first",
            ));
        }
        if old.is_in(workspace) {
            return Err(ErrorBody::new(ErrorCode::BadRequest, "it is there already"));
        }
        let (title, harness) = (old.title.clone(), old.harness.clone());
        // Its clients give the new one their size as they show it.
        let size = TermSize::default();
        let asked = Asked::new(Some(title), None, None);
        let Reply::SessionStarted { id } = self
            .start_session(Some(workspace), harness, size, asked)
            .await?
        else {
            return Err(ErrorBody::new(ErrorCode::Internal, "no terminal started"));
        };
        // A tab or window showing the old one shows the new one now; none
        // may show it.
        let _ = self.switch(session, id);
        if self.is_running(session) {
            self.moving.0.insert(session);
            self.stop_session(session).await?;
        } else {
            self.remove_session(session).await?;
        }
        tracing::info!(%session, new = %id, "a terminal started again elsewhere");
        Ok(Reply::SessionStarted { id })
    }

    /// `session` ended: one closed by a restart elsewhere is taken off the
    /// list.
    pub(super) async fn remove_if_moved(&mut self, session: SessionId) {
        if self.moving.0.remove(&session)
            && let Err(error) = self.remove_session(session).await
        {
            tracing::warn!(%session, error = %error.message, "a moved terminal stayed listed");
        }
    }
}

/// Whether `folder` is `root` or inside it, as the file system has them:
/// `/tmp` and `/private/tmp` are one folder on a Mac.
fn within(folder: &Path, root: &Path) -> bool {
    let real = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    real(folder).starts_with(real(root))
}

/// The workspace of `workspaces` whose folder holds `folder`, the deepest
/// where several do (a worktree inside its project's folder).
fn holding<'a>(folder: &Path, workspaces: &'a [Workspace]) -> Option<&'a Workspace> {
    workspaces
        .iter()
        .filter_map(|workspace| Some((workspace, workspace.kind.host_path()?)))
        .filter(|(_, path)| within(folder, path))
        .max_by_key(|(_, path)| path.components().count())
        .map(|(workspace, _)| workspace)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muster_core::{ProjectId, Timestamp, WorkspaceKind};

    use super::*;

    fn workspace(path: &str) -> Workspace {
        Workspace {
            id: WorkspaceId::new(),
            project: ProjectId::new(),
            kind: WorkspaceKind::Checkout {
                path: PathBuf::from(path),
                branch: String::new(),
            },
            created_at: Timestamp::from_millis(0),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: None,
            ticket: None,
        }
    }

    #[test]
    fn the_deepest_workspace_holding_a_folder_is_chosen_arc_33() {
        let outer = workspace("/nowhere/app");
        let inner = workspace("/nowhere/app/.worktrees/fix");
        let other = workspace("/nowhere/lib");
        let all = [outer.clone(), inner.clone(), other.clone()];
        let found = |folder: &str| holding(Path::new(folder), &all).map(|found| found.id);
        assert_eq!(found("/nowhere/app/src"), Some(outer.id));
        assert_eq!(found("/nowhere/app/.worktrees/fix/src"), Some(inner.id));
        assert_eq!(found("/nowhere/lib"), Some(other.id));
        assert_eq!(
            found("/nowhere/application"),
            None,
            "a name's start is no folder"
        );
        assert_eq!(found("/elsewhere"), None);
    }
}
